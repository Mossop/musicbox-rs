use rppal::gpio::{Level, PullUpDown};
use serde::{Deserialize, Serialize};

use crate::events::Command;

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
    pub title: String,
    pub start: ButtonConfig,
    pub display: OutputConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputConfig {
    pub action: Command,
    pub button: ButtonConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HwConfig {
    pub inputs: Vec<InputConfig>,
    pub playlists: Vec<PlaylistConfig>,
}
