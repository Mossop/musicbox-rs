use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use daemonize::{Daemonize, DaemonizeError};
use futures::compat::*;
use futures::stream::StreamExt;
use log::{error, info};
use rppal::gpio::Gpio;
use serde_json::from_reader;
use signal_hook::iterator::Signals;
use tokio::runtime::Runtime;

use crate::events::{Event, EventListener, EventLoop, InputEvent};
use crate::hardware::event_stream;
use crate::hw_config::HwConfig;
use crate::player::Player;
use crate::playlist::Playlist;
use crate::ResultErrorLogger;

const HW_CONFIG_NAME: &str = "hwconfig.json";

pub struct MusicBox {
    data_dir: PathBuf,
    playlists: HashMap<String, Playlist>,
    gpio: Gpio,
    player: Player,
}

impl MusicBox {
    // Should perform any privileged actions before the daemon reduces
    // privileges.
    async fn init(data_dir: &Path) -> Result<EventLoop, String> {
        let mut hw_config_file = data_dir.to_owned();
        hw_config_file.push(HW_CONFIG_NAME.parse::<PathBuf>().unwrap());

        let file = File::open(&hw_config_file)
            .map_err(|_| format!("Could not open config file '{}'.", hw_config_file.display()))?;

        let hw_config: HwConfig = from_reader(BufReader::new(file)).map_err(|e| {
            format!(
                "Unable to parse config file '{}': {}",
                hw_config_file.display(),
                e
            )
        })?;

        let gpio = Gpio::new().map_err(|e| e.to_string())?;

        let mut event_loop = EventLoop::new();
        let mut map = HashMap::with_capacity(hw_config.playlists.len());
        for config in hw_config.playlists {
            let playlist = match Playlist::new(data_dir, &gpio, &config, &mut event_loop).await {
                Ok(p) => p,
                Err(_) => continue,
            };
            map.insert(playlist.name(), playlist);
        }

        for event in hw_config.inputs {
            match event_stream(&gpio, &event.button, event.action.clone(), None) {
                Ok(s) => event_loop.add_event_stream(s),
                Err(e) => {
                    error!(
                        "Failed to initialize event {:?} button: {}",
                        event.action, e
                    );
                    continue;
                }
            }
        }

        match Signals::new(&[
            signal_hook::SIGHUP,
            signal_hook::SIGTERM,
            signal_hook::SIGINT,
            signal_hook::SIGQUIT,
        ])
        .and_then(|s| s.into_async())
        {
            Ok(signals) => {
                event_loop.add_event_stream(signals.compat().map(|r| match r {
                    Ok(signal_hook::SIGHUP) => Event::Input(InputEvent::Reload),
                    Ok(_) => Event::Input(InputEvent::Shutdown),
                    Err(e) => Event::Error(e.to_string()),
                }));
            }
            Err(e) => {
                error!("Unable to attach signal handler: {}", e);
            }
        }

        let music_box = MusicBox {
            player: Player::new(&mut event_loop),
            gpio,
            data_dir: data_dir.to_owned(),
            playlists: map,
        };

        event_loop.add_listener(music_box);

        Ok(event_loop)
    }

    async fn run(data_dir: &Path) -> Result<(), String> {
        info!("Music box initialization.");
        let mut event_loop = MusicBox::init(data_dir)
            .await
            .log_error(|e| format!("Music box initialization failed: {}", e))?;
        Ok(event_loop.run().await)
    }

    pub fn block(data_dir: &Path) -> Result<(), String> {
        let runtime = Runtime::new().map_err(|e| e.to_string())?;

        match runtime.block_on(MusicBox::run(data_dir)) {
            Ok(()) => {
                runtime.shutdown_on_idle();
                Ok(())
            }
            Err(e) => {
                runtime.shutdown_now();
                Err(e)
            }
        }
    }

    pub fn daemonize(data_dir: &Path) -> Result<(), String> {
        let path = data_dir.to_owned();

        // If forking fails we still run in the parent process. If it succeeds
        // the parent process exits immediately and any other results are being
        // handled in the forked process.
        let result = Daemonize::new()
            .privileged_action(move || {
                // This runs in the forked process.
                let runtime = Runtime::new().unwrap();
                info!("Music box initialization.");
                runtime
                    .block_on(MusicBox::init(&path))
                    .log_error(|e| format!("Music box initialization failed: {}", e))
                    .expect("Initialization failed.")
            })
            .start();

        let mut event_loop = match result {
            Ok(event_loop) => {
                // In the forked process.
                event_loop
            }
            Err(DaemonizeError::Fork) => {
                // Failed to fork at all.
                error!("Failed to launch daemon.");
                return Err(String::from("Failed to launch daemon."));
            }
            Err(e) => {
                // In the forked process but something went wrong.
                error!("Failed during fork: {}", e);
                panic!(e);
            }
        };

        let runtime = Runtime::new().unwrap();
        Ok(runtime.block_on(event_loop.run()))
    }
}

impl EventListener for MusicBox {
    fn event(&mut self, event: &Event) {
        info!("Saw event {:?}", event);

        match event {
            Event::Startup => {
                info!("Music box startup.");
            }
            Event::Shutdown => {
                info!("Music box clean shutdown.");
            }
            Event::Error(s) => {
                error!("{}", s);
            }
            _ => (),
        }
    }
}
