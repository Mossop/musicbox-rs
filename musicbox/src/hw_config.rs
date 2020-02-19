use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HwConfig {
    pub keyboard: Vec<crate::hardware::keyboard::KeyConfig>,
    #[cfg(target_arch = "arm")]
    pub buttons: Vec<crate::hardware::gpio::button::ButtonConfig>,
    pub playlists: Vec<crate::playlist::PlaylistConfig>,
}
