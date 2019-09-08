use rppal::gpio::{Level, PullUpDown};
use serde::{Deserialize, Serialize};

use crate::events::Event;

#[derive(Serialize, Deserialize)]
#[serde(remote = "PullUpDown")]
enum PullUpDownDef {
    Off,
    PullDown,
    PullUp,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "Level")]
enum LevelDef {
    Low,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ButtonConfig {
    pub pin: u8,

    #[serde(with = "PullUpDownDef")]
    pub kind: PullUpDown,

    #[serde(with = "LevelDef")]
    pub on: Level,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputConfig {
    pub pin: u8,

    #[serde(with = "LevelDef")]
    pub on: Level,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistConfig {
    pub name: String,
    pub default_title: String,
    pub start: ButtonConfig,
    pub display: OutputConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputConfig {
    pub event: Event,
    pub button: ButtonConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HwConfig {
    pub events: Vec<InputConfig>,
    pub playlists: Vec<PlaylistConfig>,
}
