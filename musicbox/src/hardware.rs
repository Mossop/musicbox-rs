use std::time::Duration;

use futures::future::ready;
use futures::stream::{Stream, StreamExt};
use log::{debug, error};
use rpi_futures::gpio::{ButtonEvent, InputPinEvents};
use rppal::gpio::{Gpio, Level, OutputPin, PullUpDown};

use crate::events::{Command, Message};
use crate::hw_config::{ButtonConfig, OutputConfig};
use crate::MusicResult;

const BUTTON_HOLD_TIMEOUT: Duration = Duration::from_secs(1);

pub struct LED {
    pin: OutputPin,
    on: Level,
}

impl LED {
    pub fn new(gpio: &Gpio, config: &OutputConfig) -> MusicResult<LED> {
        debug!(
            "Creating LED for pin {}, on level: {}",
            config.pin, config.on
        );

        let pin = match gpio.get(config.pin) {
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

pub fn event_stream(
    gpio: &Gpio,
    config: &ButtonConfig,
    click_event: Command,
    hold_event: Option<Command>,
) -> MusicResult<impl Stream<Item = Message<Command>>> {
    debug!("Creating event button for pin {}, type {}, on level: {}, click event {:?}, hold event {:?}", config.pin, config.kind, config.on, click_event, hold_event);
    let pin = match gpio.get(config.pin) {
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

    let events =
        match input.button_events(config.on, hold_event.as_ref().map(|_| BUTTON_HOLD_TIMEOUT)) {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to open button stream for pin {}: {}", config.pin, e);
                return Err(e.to_string());
            }
        };

    let pin: u8 = config.pin;
    Ok(events.filter_map(move |r| {
        ready(match r {
            Ok(ButtonEvent::Click(i)) => Some(Message::new(i, click_event.clone())),
            Ok(ButtonEvent::Hold(i)) => hold_event.as_ref().map(|e| Message::new(i, e.clone())),
            Err(e) => {
                error!("Failure while polling button on pin {}: {}", pin, e);
                None
            }
            _ => None,
        })
    }))
}
