use std::pin::Pin;
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Waker};
use std::time::Instant;

use futures::stream::Stream;
use rppal::gpio::{InputPin, Level, Result, Trigger};

pub struct PinEvent {
    pub instant: Instant,
    pub level: Level,
}

pub struct PinEventStream {
    receiver: mpsc::Receiver<PinEvent>,
    pin: Option<InputPin>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl PinEventStream {
    fn new(mut pin: InputPin, trigger: Trigger) -> Result<PinEventStream> {
        let waker = Arc::new(Mutex::new(None));
        let (sender, receiver) = mpsc::channel::<PinEvent>();

        let interrupt_waker = waker.clone();
        pin.set_async_interrupt(trigger, move |level| {
            let event = PinEvent {
                level,
                instant: Instant::now(),
            };

            // Both the callback and poll_next functions must lock first. Should
            // be cheap.
            let mut waker: MutexGuard<Option<Waker>> = match interrupt_waker.lock() {
                Ok(w) => w,
                _ => panic!("Unable to lock."),
            };

            sender.send(event).expect("Should never fail.");
            if let Some(w) = waker.take() {
                w.wake();
            }
        })?;

        Ok(PinEventStream {
            receiver,
            pin: Some(pin),
            waker,
        })
    }

    pub fn close(mut self) -> Result<InputPin> {
        match self.pin.take() {
            Some(mut pin) => {
                pin.clear_async_interrupt()?;
                Ok(pin)
            }
            None => panic!("Should be unable to own a PinEventStream that doesn't have a pin."),
        }
    }
}

impl Stream for PinEventStream {
    type Item = Result<PinEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Result<PinEvent>>> {
        // Both the callback and poll_next functions must lock first. Should be
        // cheap.
        let mut waker = match self.waker.lock() {
            Ok(w) => w,
            Err(_) => return Poll::Ready(Some(Err(rppal::gpio::Error::ThreadPanic))),
        };

        match self.receiver.try_recv() {
            Ok(event) => Poll::Ready(Some(Ok(event))),
            Err(mpsc::TryRecvError::Empty) => {
                waker.replace(cx.waker().clone());
                Poll::Pending
            }
            Err(mpsc::TryRecvError::Disconnected) => Poll::Ready(None),
        }
    }
}

pub trait InputPinEvents {
    fn events(self, trigger: Trigger) -> Result<PinEventStream>;
}

impl InputPinEvents for InputPin {
    fn events(self, trigger: Trigger) -> Result<PinEventStream> {
        PinEventStream::new(self, trigger)
    }
}
