use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::id;

use daemonize::{Daemonize, DaemonizeError};
use futures::compat::*;
use futures::stream::StreamExt;
use log::{error, info};
use rppal::gpio::Gpio;
use serde::Serialize;
use serde_json::{from_reader, to_string_pretty};
use signal_hook::iterator::Signals;
use tokio::runtime::Runtime;

use crate::events::{Event, EventStream};
use crate::hardware::event_stream;
use crate::hw_config::HwConfig;
use crate::player::Player;
use crate::playlist::Playlist;
use crate::ResultErrorLogger;

const HW_CONFIG_NAME: &str = "hwconfig.json";
const VOLUME_INTERVAL: f32 = 0.1;

#[derive(Serialize)]
pub struct MusicBox {
    playlists: HashMap<String, Playlist>,
    player: Player,
    #[serde(skip)]
    event_stream: EventStream,
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

        let stream = EventStream::new();
        let mut map = HashMap::with_capacity(hw_config.playlists.len());
        for config in hw_config.playlists {
            let playlist = match Playlist::new(data_dir, &gpio, &config, &stream).await {
                Ok(p) => p,
                Err(_) => continue,
            };
            map.insert(playlist.name(), playlist);
        }

        for event in hw_config.inputs {
            stream.add_event_stream(event_stream(
                &gpio,
                &event.button,
                event.action.clone(),
                None,
            )?);
        }

        let player = Player::new()?;

        match Signals::new(&[
            signal_hook::SIGHUP,
            signal_hook::SIGTERM,
            signal_hook::SIGINT,
            signal_hook::SIGQUIT,
            signal_hook::SIGUSR1,
            signal_hook::SIGUSR2,
        ])
        .and_then(|s| s.into_async())
        {
            Ok(signals) => {
                stream.add_event_stream(signals.compat().map(|r| match r {
                    Ok(signal_hook::SIGHUP) => Event::Reload,
                    Ok(signal_hook::SIGTERM) => Event::Shutdown,
                    Ok(signal_hook::SIGINT) => Event::Shutdown,
                    Ok(signal_hook::SIGQUIT) => Event::Shutdown,
                    Ok(signal_hook::SIGUSR1) => Event::Status,
                    Ok(signal_hook::SIGUSR2) => Event::StartPlaylist(String::from("red"), true),
                    Ok(signal) => Event::Error(format!("Received unexpected signal {}.", signal)),
                    Err(e) => Event::Error(e.to_string()),
                }));
            }
            Err(e) => {
                error!("Unable to attach signal handler: {}", e);
            }
        }

        let music_box = MusicBox {
            player,
            playlists: map,
            event_stream: stream,
        };

        Ok(music_box)
    }

    async fn run(mut self) -> Result<(), String> {
        info!("Music box startup. Running as process {}.", id());

        loop {
            let event = match self.event_stream.next().await {
                Some(e) => e,
                None => return Ok(()),
            };

            info!("Saw event {:?}", event);

            match event {
                Event::PreviousTrack => self.player.previous(),
                Event::NextTrack => self.player.next(),
                Event::PlayPause => self.player.play_pause(),
                Event::VolumeUp => self
                    .player
                    .set_volume(self.player.volume() + VOLUME_INTERVAL),
                Event::VolumeDown => self
                    .player
                    .set_volume(self.player.volume() - VOLUME_INTERVAL),
                Event::Shutdown => {
                    info!("Music box clean shutdown.");
                    return Ok(());
                }
                Event::StartPlaylist(name, force) => {
                    if let Some(playlist) = self.playlists.get(&name) {
                        self.player.start_tracks(playlist.tracks(), force);
                    } else {
                        error!(
                            "Received a request to start playlist {} but that list does not exist.",
                            name
                        );
                    }
                }
                Event::Reload => {
                    for playlist in self.playlists.values_mut() {
                        playlist.rescan().await?;
                    }
                }
                Event::Status => match to_string_pretty(&self) {
                    Ok(json) => println!("{}", json),
                    Err(e) => error!("Error generating status: {}.", e),
                },
                Event::Error(s) => {
                    error!("{}", s);
                }
            }
        }
    }

    async fn init_and_run(data_dir: &Path) -> Result<(), String> {
        let music_box = MusicBox::init(data_dir).await?;
        music_box.run().await
    }

    pub fn block(data_dir: &Path) -> Result<(), String> {
        let runtime = Runtime::new().map_err(|e| e.to_string())?;

        match runtime.block_on(MusicBox::init_and_run(data_dir)) {
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

        let music_box = match result {
            Ok(music_box) => {
                // In the forked process.
                music_box
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
        runtime.block_on(music_box.run()).map_err(|e| e.to_string())
    }
}
