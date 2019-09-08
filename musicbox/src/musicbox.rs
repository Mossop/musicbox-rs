use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use daemonize::{Daemonize, DaemonizeError};
use futures::compat::*;
use futures::stream::StreamExt;
use log::{error, info};
use rppal::gpio::Gpio;
use serde_json::from_reader;
use signal_hook::iterator::Signals;
use tokio::runtime::Runtime;

use crate::events::{Event, EventStream};
use crate::hardware::event_stream;
use crate::hw_config::HwConfig;
use crate::playlist::Playlist;
use crate::ResultErrorLogger;

const HW_CONFIG_NAME: &str = "hwconfig.json";

pub struct MusicBox {
    data_dir: PathBuf,
    playlists: HashMap<String, Playlist>,
    gpio: Gpio,

    events: Pin<Box<EventStream>>,
}

impl MusicBox {
    // Should perform any privileged actions before the daemon reduces
    // privileges.
    async fn init(data_dir: &Path) -> Result<MusicBox, String> {
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

        let mut events = EventStream::new();
        let mut map = HashMap::with_capacity(hw_config.playlists.len());
        for config in hw_config.playlists {
            let playlist = match Playlist::new(data_dir, &gpio, &config, &mut events).await {
                Ok(p) => p,
                Err(_) => continue,
            };
            map.insert(playlist.name(), playlist);
        }

        for event in hw_config.events {
            match event_stream(&gpio, &event.button, event.event.clone(), None) {
                Ok(s) => events.add_event_stream(s),
                Err(e) => {
                    error!("Failed to initialize event {:?} button: {}", event.event, e);
                    continue;
                }
            }
        }

        match Signals::new(&[
            signal_hook::SIGTERM,
            signal_hook::SIGINT,
            signal_hook::SIGQUIT,
        ])
        .and_then(|s| s.into_async())
        {
            Ok(signals) => {
                events.add_event_stream(signals.compat().map(|r| match r {
                    Ok(_) => Event::Shutdown,
                    Err(e) => Event::Error(e.to_string()),
                }));
            }
            Err(e) => {
                error!("Unable to attach signal handler: {}", e);
            }
        }

        Ok(MusicBox {
            gpio,
            data_dir: data_dir.to_owned(),
            playlists: map,
            events: Box::pin(events),
        })
    }

    async fn run_loop(&mut self) -> Result<(), String> {
        info!("Music box startup.");

        while let Some(event) = self.events.next().await {
            info!("Saw event {:?}", event);

            match event {
                Event::Shutdown => {
                    break;
                }
                Event::Error(s) => {
                    error!("{}", s);
                }
                _ => (),
            }
        }

        info!("Music box clean shutdown.");
        Ok(())
    }

    async fn run(data_dir: &Path) -> Result<(), String> {
        info!("Music box initialization.");
        let mut music_box = MusicBox::init(data_dir)
            .await
            .log_error(|e| format!("Music box initialization failed: {}", e))?;
        music_box
            .run_loop()
            .await
            .log_error(|e| format!("Music box unclean shutdown: {}", e))
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

        let mut music_box = match result {
            Ok(musicbox) => {
                // In the forked process.
                musicbox
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
        runtime
            .block_on(music_box.run_loop())
            .log_error(|e| format!("Music box unclean shutdown: {}", e))
    }
}
