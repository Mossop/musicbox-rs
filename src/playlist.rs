use std::io;
use std::path::{Path, PathBuf};

use futures::stream::StreamExt;
use log::{debug, error, info};
use serde::Deserialize;
use tokio::fs::{create_dir_all, metadata, read_dir};

use crate::error::{MusicResult, VoidResult};
#[cfg(feature = "rpi")]
use crate::hardware::gpio::led::{LEDConfig, LED};
use crate::track::Track;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistConfig {
    pub name: String,
    pub title: String,
    #[cfg(feature = "rpi")]
    pub led: LEDConfig,
}

pub struct StoredPlaylist {
    root: PathBuf,
    name: String,
    tracks: Vec<Track>,
    #[cfg(feature = "rpi")]
    pub led: LED,
}

impl StoredPlaylist {
    pub async fn init(
        data_dir: &Path,
        configs: Vec<PlaylistConfig>,
    ) -> MusicResult<Vec<StoredPlaylist>> {
        let mut collection = Vec::with_capacity(configs.len());
        for config in configs {
            let playlist = StoredPlaylist::new(data_dir, &config).await?;
            collection.push(playlist);
        }
        Ok(collection)
    }

    pub async fn new(data_dir: &Path, config: &PlaylistConfig) -> MusicResult<StoredPlaylist> {
        let mut root = data_dir.to_owned();
        root.push("playlists".parse::<PathBuf>().map_err(|e| e.to_string())?);
        root.push(config.name.parse::<PathBuf>().map_err(|e| e.to_string())?);

        debug!(
            "Creating playlist {}, data: '{}'",
            config.name,
            root.display(),
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
            name: config.name.clone(),
            tracks: Vec::new(),
            #[cfg(feature = "rpi")]
            led: LED::new(&config.led)?,
        };
        playlist.rescan().await?;

        Ok(playlist)
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
            #[cfg(feature = "rpi")]
            self.led.off();
        } else {
            info!("{} playlist has {} tracks.", self.name, self.tracks.len());
            #[cfg(feature = "rpi")]
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

    pub fn equals(&self, tracks: &[Track]) -> bool {
        self.tracks == tracks
    }
}
