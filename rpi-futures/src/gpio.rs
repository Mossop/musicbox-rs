//! Futures functionality for GPIO.
use std::future::Future;
use std::pin::Pin;
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use futures::stream::Stream;
use log::trace;
use rppal::gpio::{InputPin, Level, Result, Trigger};
use tokio_timer::{delay, Delay};

/// An event reported from an input pin.
///
/// Reports the instant it occured and the pin's level at that instant.
pub struct PinEvent {
    /// The instant the event occured.
    pub instant: Instant,
    /// The level of the pin at the instant this event was recorded.
    ///
    /// This may not match the current level of the pin.
    pub level: Level,
}

/// A simple stream of input pin events as `rppal` reports them.
///
/// Retrieve this by calling [`events()`](trait.InputPinEvents.html#method.events)
/// on an [`rppal`](https://docs.golemparts.com/rppal) `InputPin`.
///
/// In testing it is possible to see the pin report a transition to the same
/// level twice or more so you must check against any previous level before
/// assuming that an event means something has changed. [`PinChangeStream`](struct.PinChangeStream.html)
/// handles this for you.
pub struct PinEventStream {
    receiver: mpsc::Receiver<PinEvent>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl PinEventStream {
    fn new(pin: &mut InputPin, trigger: Trigger) -> Result<PinEventStream> {
        let waker = Arc::new(Mutex::new(None));
        let (sender, receiver) = mpsc::channel::<PinEvent>();

        let interrupt_waker = waker.clone();
        pin.set_async_interrupt(trigger, move |level| {
            let event = PinEvent {
                level,
                instant: Instant::now(),
            };

            trace!("Saw pin transition to {:?}", level);

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

        Ok(PinEventStream { receiver, waker })
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

/// Debounces an `InputPin`.
///
/// Retrieve this by calling [`debounced_events()`](trait.InputPinEvents.html#method.debounced_events)
/// on an [`rppal`](https://docs.golemparts.com/rppal) `InputPin`.
///
/// Depending on the physical switch attached to a pin pressing or releasing may
/// not result in a clean transition between levels. Sometimes the level may
/// bounce from high to low or vice versa a few times before settling. This is
/// similar to [`PinEventStream`](struct.PinEventStream.html) however it defers
/// delivery of events until the pin hasn't transitioned for a set timout. At
/// that point it delivers a single event with the most recent state transition.
/// Similar to [`PinEventStream`](struct.PinEventStream.html) this means you may
/// receive sequention events for the same level.
///
/// To be clear, this will delay event delivery by the timeout (though the
/// instant of the event will be set correctly).
pub struct DebouncedPinEventStream {
    event_stream: Pin<Box<PinEventStream>>,
    timeout: Duration,
    pending: Option<(Pin<Box<Delay>>, PinEvent)>,
}

impl DebouncedPinEventStream {
    fn new(
        pin: &mut InputPin,
        trigger: Trigger,
        timeout: Duration,
    ) -> Result<DebouncedPinEventStream> {
        Ok(DebouncedPinEventStream {
            event_stream: Box::pin(pin.events(trigger)?),
            timeout,
            pending: None,
        })
    }
}

impl Stream for DebouncedPinEventStream {
    type Item = Result<PinEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Result<PinEvent>>> {
        match self.event_stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(event))) => {
                self.pending = Some((Box::pin(delay(event.instant + self.timeout)), event));
                self.poll_next(cx)
            }
            Poll::Pending => match self.pending.take() {
                Some((mut timeout, event)) => match timeout.as_mut().poll(cx) {
                    Poll::Ready(_) => Poll::Ready(Some(Ok(event))),
                    Poll::Pending => {
                        self.pending = Some((timeout, event));
                        Poll::Pending
                    }
                },
                None => Poll::Pending,
            },
            other => other,
        }
    }
}

/// A stream of debounced pin level changes.
///
/// Retrieve this by calling [`changes()()`](trait.InputPinEvents.html#method.changes)
/// on an [`rppal`](https://docs.golemparts.com/rppal) `InputPin`.
///
/// Takes a pin, debounces its level change events and only returns actual
/// changes to the level. You should never see two events reporting the same new
/// level.
pub struct PinChangeStream {
    last_level: Level,
    events: Pin<Box<DebouncedPinEventStream>>,
}

impl PinChangeStream {
    fn new(pin: &mut InputPin, timeout: Duration) -> Result<PinChangeStream> {
        Ok(PinChangeStream {
            last_level: pin.read(),
            events: Box::pin(pin.debounced_events(Trigger::Both, timeout)?),
        })
    }
}

impl Stream for PinChangeStream {
    type Item = Result<PinEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Result<PinEvent>>> {
        match self.events.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(event))) => {
                if event.level != self.last_level {
                    self.last_level = event.level;
                    Poll::Ready(Some(Ok(event)))
                } else {
                    self.poll_next(cx)
                }
            }
            other => other,
        }
    }
}

/// Extends [`rppal`](https://docs.golemparts.com/rppal)'s `InputPin` with
/// functions to return various streams.
pub trait InputPinEvents {
    /// Returns a raw stream of level change events.
    ///
    /// Similar to calling `set_async_interrupt` on the pin but you get an
    /// asynchronous stream instead of needing to supply a callback function.
    ///
    /// Requesting any other mechanism of interrupt from this pin will cause
    /// this stream to stop returning events.
    fn events(&mut self, trigger: Trigger) -> Result<PinEventStream>;

    /// Returns a debounced stream of level change events.
    ///
    /// Similar to [`events()`](#method.events) but drops any level changes that
    /// occur within the given timeout.
    ///
    /// Requesting any other mechanism of interrupt from this pin will cause
    /// this stream to stop returning events.
    fn debounced_events(
        &mut self,
        trigger: Trigger,
        timeout: Duration,
    ) -> Result<DebouncedPinEventStream>;

    /// Returns a stream of debounced level changes.
    ///
    /// Guaranteed to only return level changes. Changes are debounced by
    /// `timeout`.
    fn changes(&mut self, timeout: Duration) -> Result<PinChangeStream>;
}

impl InputPinEvents for InputPin {
    fn events(&mut self, trigger: Trigger) -> Result<PinEventStream> {
        PinEventStream::new(self, trigger)
    }

    fn debounced_events(
        &mut self,
        trigger: Trigger,
        timeout: Duration,
    ) -> Result<DebouncedPinEventStream> {
        DebouncedPinEventStream::new(self, trigger, timeout)
    }

    fn changes(&mut self, timeout: Duration) -> Result<PinChangeStream> {
        PinChangeStream::new(self, timeout)
    }
}
