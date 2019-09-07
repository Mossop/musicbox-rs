use futures::stream::StreamExt;
use rpi_futures::gpio::InputPinEvents;
use rppal::gpio::{Gpio, Level, Result};
use tokio::runtime::Runtime;

async fn run() -> Result<()> {
    let gpio = Gpio::new()?;
    let pin = gpio.get(4)?;
    let mut input = pin.into_input_pullup();
    let mut stream = input.button_events(Level::Low, None)?;

    loop {
        let event = match stream.next().await {
            Some(Ok(e)) => e,
            Some(Err(e)) => return Err(e),
            None => return Ok(()),
        };

        println!("{:?}", event);
    }
}

fn main() {
    env_logger::init();

    let runtime = Runtime::new().expect("Failed to create async runtime.");
    runtime.block_on(run()).expect("Should not have failed.");
}
