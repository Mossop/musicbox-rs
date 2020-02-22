use std::net::SocketAddr;

use serde::Deserialize;
use serde_json::from_slice;

use crate::assets::Config;
use crate::error::{ErrorExt, MusicResult};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HwConfig {
    pub server: SocketAddr,
    pub keyboard: Vec<crate::hardware::keyboard::KeyConfig>,
    #[cfg(feature = "rpi")]
    pub buttons: Vec<crate::hardware::gpio::button::ButtonConfig>,
    pub playlists: Vec<crate::playlist::PlaylistConfig>,
}

impl HwConfig {
    pub fn load() -> MusicResult<HwConfig> {
        Config::get("hw_config.json")
            .ok_or_else(|| String::from("Could not load hardware config."))
            .and_then(|slice| from_slice(&slice).prefix("Failed to parse hardware config"))
    }
}
