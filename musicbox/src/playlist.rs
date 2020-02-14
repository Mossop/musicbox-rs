use std::io;
use std::path::{Path, PathBuf};

use futures::stream::{Stream, StreamExt};
use log::{debug, error, info};
use rppal::gpio::Gpio;
use serde::Serialize;
use tokio::fs::{create_dir_all, metadata, read_dir};

use crate::events::{Command, Message};
use crate::hardware::{event_stream, LED};
use crate::hw_config::PlaylistConfig;
use crate::track::Track;
use crate::{MusicResult, ResultErrorLogger, VoidResult};

#[derive(Serialize)]
pub struct StoredPlaylist {
    root: PathBuf,
    name: String,
    tracks: Vec<Track>,
    #[serde(skip)]
    led: LED,
}

impl StoredPlaylist {
    pub async fn new(
        data_dir: &Path,
        gpio: &Gpio,
        config: &PlaylistConfig,
    ) -> MusicResult<(StoredPlaylist, impl Stream<Item = Message<Command>>)> {
        let mut root = data_dir.to_owned();
        root.push("playlists".parse::<PathBuf>().map_err(|e| e.to_string())?);
        root.push(config.name.parse::<PathBuf>().map_err(|e| e.to_string())?);

        debug!(
            "Creating playlist {}, data: '{}', button pin: {}, led pin: {}",
            config.name,
            root.display(),
            config.start.pin,
            config.display.pin
        );

        if let Err(e) = metadata(&root).await {
            if e.kind() == io::ErrorKind::NotFound {
                if let Err(e) = create_dir_all(&root).await {
                    error!(
                        "Failed to create playlist {} data directory: {}",
                        config.name, e
                    );
                    return Err(e.to_string());
                }
            } else {
                error!(
                    "Failed to access playlist {} data directory: {}",
                    config.name,
                    root.display()
                );
                return Err(format!("{}", e));
            }
        }

        let mut playlist = StoredPlaylist {
            root,
            led: LED::new(gpio, &config.display)?,
            name: config.name.clone(),
            tracks: Default::default(),
        };
        playlist.rescan().await?;

        let button = event_stream(
            gpio,
            &config.start,
            Command::StartPlaylist(config.name.clone(), false),
            Some(Command::StartPlaylist(config.name.clone(), true)),
        )
        .log_error(|e| format!("Failed to create playlist {} button: {}", config.name, e))?;

        Ok((playlist, button))
    }

    pub async fn rescan(&mut self) -> VoidResult {
        self.tracks = read_dir(self.root.clone())
            .await
            .map_err(|e| e.to_string())?
            .filter_map(|r| async {
                let entry = match r {
                    Ok(r) => r,
                    _ => return None,
                };

                let metadata = match entry.metadata().await {
                    Ok(m) => m,
                    _ => return None,
                };

                if !metadata.is_file() {
                    return None;
                }

                if let Some(extension) = entry.path().extension() {
                    if extension == "mp3" {
                        Some(Track::new(&entry.path()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<Track>>()
            .await;

        if self.tracks.is_empty() {
            info!("{} playlist has no tracks.", self.name);
            self.led.off();
        } else {
            info!("{} playlist has {} tracks.", self.name, self.tracks.len());
            self.led.on();
        }

        Ok(())
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn tracks(&self) -> Vec<Track> {
        self.tracks.clone()
    }

    pub fn equals(&self, tracks: &Vec<Track>) -> bool {
        &self.tracks == tracks
    }
}