use futures::future::ready;
use futures::stream::{Stream, StreamExt};
use log::{debug, error};
use rpi_futures::gpio::{ButtonEvent, InputPinEvents};
use rppal::gpio::{Level, PullUpDown};
use serde::Deserialize;

use crate::error::{MusicResult, VoidResult};
use crate::events::{Command, Message};
use crate::hardware::gpio::{LevelDef, PullUpDownDef, GPIO};
use crate::musicbox::MusicBox;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ButtonConfig {
    pub pin: u8,

    #[serde(with = "PullUpDownDef")]
    pub kind: PullUpDown,

    #[serde(with = "LevelDef")]
    pub on: Level,

    pub command: Command,
}

pub struct Buttons;

impl Buttons {
    pub fn init(music_box: &mut MusicBox, buttons: &Vec<ButtonConfig>) -> VoidResult {
        for config in buttons {
            music_box.add_command_stream(Buttons::new(config.to_owned())?);
        }

        Ok(())
    }

    fn new(config: ButtonConfig) -> MusicResult<impl Stream<Item = Message<Command>>> {
        debug!(
            "Creating event button for pin {}, type {}, on level: {}, command {:?}",
            config.pin, config.kind, config.on, config.command
        );
        let pin = match GPIO.get(config.pin) {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to get pin {}: {}", config.pin, e);
                return Err(e.to_string());
            }
        };

        let input = match config.kind {
            PullUpDown::PullUp => pin.into_input_pullup(),
            PullUpDown::PullDown => pin.into_input_pulldown(),
            PullUpDown::Off => pin.into_input(),
        };

        let events = match input.button_events(config.on, None) {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to open button stream for pin {}: {}", config.pin, e);
                return Err(e.to_string());
            }
        };

        let pin: u8 = config.pin;
        Ok(events.filter_map(move |r| {
            ready(match r {
                Ok(ButtonEvent::Click(i)) => Some(Message::new(i, config.command.clone())),
                Err(e) => {
                    error!("Failure while polling button on pin {}: {}", pin, e);
                    None
                }
                _ => None,
            })
        }))
    }
}
