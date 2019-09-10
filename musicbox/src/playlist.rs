use std::io;
use std::path::{Path, PathBuf};

use log::{debug, error};
use rppal::gpio::Gpio;
use tokio::fs::{create_dir_all, metadata};

use crate::events::{EventLoop, InputEvent};
use crate::hardware::{event_stream, LED};
use crate::hw_config::PlaylistConfig;
use crate::ResultErrorLogger;

pub struct Playlist {
    root: PathBuf,
    led: LED,
    name: String,
}

impl Playlist {
    pub async fn new(
        data_dir: &Path,
        gpio: &Gpio,
        config: &PlaylistConfig,
        events: &mut EventLoop,
    ) -> Result<Playlist, String> {
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

        let mut led = LED::new(gpio, &config.display).map_err(|e| e.to_string())?;
        let button = event_stream(
            gpio,
            &config.start,
            InputEvent::StartPlaylist(config.name.clone()),
            Some(InputEvent::RestartPlaylist(config.name.clone())),
        )
        .map_err(|e| e.to_string())
        .log_error(|e| format!("Failed to create playlist {} button: {}", config.name, e))?;

        events.add_event_stream(button);
        led.on();

        Ok(Playlist {
            root,
            led,
            name: config.name.clone(),
        })
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }
}
