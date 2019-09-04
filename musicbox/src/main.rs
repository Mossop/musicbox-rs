use futures::executor::block_on;
use futures::stream::StreamExt;
use rpi_futures::gpio::InputPinEvents;
use rppal::gpio::{Gpio, Result, Trigger};

async fn run() -> Result<()> {
    let gpio = Gpio::new()?;
    let pin = gpio.get(4)?;
    let input = pin.into_input_pullup();
    let mut stream = input.events(Trigger::Both)?;

    loop {
        let event = match stream.next().await {
            Some(Ok(e)) => e,
            Some(Err(e)) => return Err(e),
            None => return Ok(()),
        };

        println!("{:?} {}", event.instant, event.level);
    }
}

fn main() {
    block_on(run()).expect("Should not have failed.");
}
