use lazy_static::lazy_static;
use rppal::gpio::{Gpio, Level, PullUpDown};
use serde::Deserialize;

pub mod button;
pub mod led;

lazy_static! {
    pub static ref GPIO: Gpio = Gpio::new().unwrap();
}

#[derive(Deserialize)]
#[serde(remote = "PullUpDown")]
pub enum PullUpDownDef {
    Off,
    PullDown,
    PullUp,
}

#[derive(Deserialize)]
#[serde(remote = "Level")]
pub enum LevelDef {
    Low,
    High,
}
