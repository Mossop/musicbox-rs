use rppal::gpio::{Level, OutputPin};

use log::{debug, error};
use serde::Deserialize;

use crate::error::MusicResult;
use crate::hardware::gpio::{LevelDef, GPIO};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LEDConfig {
    pub pin: u8,

    #[serde(with = "LevelDef")]
    pub on: Level,
}

pub struct LED {
    pin: OutputPin,
    on: Level,
}

impl LED {
    pub fn new(config: &LEDConfig) -> MusicResult<LED> {
        debug!(
            "Creating LED for pin {}, on level: {}",
            config.pin, config.on
        );

        let pin = match GPIO.get(config.pin) {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to get pin {}: {}", config.pin, e);
                return Err(e.to_string());
            }
        };

        let mut led = LED {
            pin: pin.into_output(),
            on: config.on,
        };
        led.off();
        Ok(led)
    }

    pub fn on(&mut self) {
        self.pin.write(self.on);
    }

    pub fn off(&mut self) {
        self.pin.write(!self.on);
    }
}

impl Drop for LED {
    fn drop(&mut self) {
        self.off();
    }
}
