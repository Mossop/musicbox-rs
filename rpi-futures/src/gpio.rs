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

const BUTTON_DEBOUNCE: u64 = 50;

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
/// Retrieve this by calling [`changes()`](trait.InputPinEvents.html#method.changes)
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

/// An event from a button.
#[derive(Debug)]
pub enum ButtonEvent {
    /// The button was pushed.
    Press(Instant),
    /// The button was released.
    Release(Instant),
    /// The push was interpreted as a click.
    Click(Instant),
    /// The push was interpreted as a hold.
    Hold(Instant),
}

/// A stream of debounced [`ButtonEvent`](enum.ButtonEvent.html)s.
///
/// Retrieve this by calling [`button_events()`](trait.InputPinEvents.html#method.button_events)
/// on an [`rppal`](https://docs.golemparts.com/rppal) `InputPin`.
///
/// Pressing and releasing a button will return [`Press`](enum.ButtonEvent.html#variant.Press)
/// and [`Release`](enum.ButtonEvent.html#variant.Release) events. Either a
/// [`Click`](enum.ButtonEvent.html#variant.Click) or [`Hold`](enum.ButtonEvent.html#variant.Hold)
/// event is returned in between:
///
/// If no hold timeout was given then a [`Click`](enum.ButtonEvent.html#variant.Click)
/// event is returned immediately after the [`Press`](enum.ButtonEvent.html#variant.Press)
/// event (it will have the same instant).
///
/// If a hold timeout is given and the button is pressed for less than the
/// timeout then a [`Click`](enum.ButtonEvent.html#variant.Click) event is
/// returned immediately before the [`Release`](enum.ButtonEvent.html#variant.Release)
/// event (it will have the same instant).
///
/// If a hold timeout is given and the button is pressed for longer than the
/// timeout then a [`Hold`](enum.ButtonEvent.html#variant.Hold) event is
/// returned after the timeout expires (with an instant that is the timeout
/// duration after the button press) and then whenever the button is released
/// later the [`Press`](enum.ButtonEvent.html#variant.Press) event is returned.
pub struct ButtonEventStream {
    hold_timeout: Option<Duration>,
    events: Pin<Box<PinChangeStream>>,
    timer: Option<Pin<Box<Delay>>>,
    pressed_level: Level,
    pending: Option<ButtonEvent>,
}

impl ButtonEventStream {
    fn new(
        pin: &mut InputPin,
        pressed_level: Level,
        hold_timeout: Option<Duration>,
    ) -> Result<ButtonEventStream> {
        Ok(ButtonEventStream {
            hold_timeout,
            events: Box::pin(pin.changes(Duration::from_millis(BUTTON_DEBOUNCE))?),
            pressed_level,
            timer: None,
            pending: None,
        })
    }
}

impl Stream for ButtonEventStream {
    type Item = Result<ButtonEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Result<ButtonEvent>>> {
        // If there was a release event pending from last time send it now.
        if let Some(event) = self.pending.take() {
            return Poll::Ready(Some(Ok(event)));
        }

        match self.events.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(event))) => {
                if event.level == self.pressed_level {
                    // Button was pressed.
                    match self.hold_timeout {
                        Some(timeout) => {
                            // Need to wait until timeout has passed to
                            // determine whether this is a click or a hold.
                            self.timer = Some(Box::pin(delay(event.instant + timeout)));
                        }
                        None => {
                            // Definitely a click, return the event the next
                            // time around.
                            self.pending = Some(ButtonEvent::Click(event.instant));
                        }
                    }
                    Poll::Ready(Some(Ok(ButtonEvent::Press(event.instant))))
                } else if self.timer.take().is_some() {
                    // Released before the click timeout, this was a click.
                    // Need to send the click event then queue a release
                    // event.
                    self.pending = Some(ButtonEvent::Release(event.instant));
                    Poll::Ready(Some(Ok(ButtonEvent::Click(event.instant))))
                } else {
                    // Already sent a hold event (or this is an initial
                    // transition), just send the release event now.
                    Poll::Ready(Some(Ok(ButtonEvent::Release(event.instant))))
                }
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => {
                if let Some(mut timer) = self.timer.take() {
                    if let Poll::Ready(_) = timer.as_mut().poll(cx) {
                        // We've hit the hold threshold. Call this a hold.
                        Poll::Ready(Some(Ok(ButtonEvent::Hold(Instant::now()))))
                    } else {
                        self.timer = Some(timer);
                        Poll::Pending
                    }
                } else {
                    Poll::Pending
                }
            }
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
    /// occur within the given `timeout`.
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
    ///
    /// Requesting any other mechanism of interrupt from this pin will cause
    /// this stream to stop returning events.
    fn changes(&mut self, timeout: Duration) -> Result<PinChangeStream>;

    /// Returns a stream of button events.
    ///
    /// Provides what you likely want for most inputs. Debounced button events
    /// including click/hold differentiation.
    ///
    /// If a button has been pressed for `hold_timeout` (if passed) then the
    /// press will be considered to be a hold instead of a click.
    ///
    /// Requesting any other mechanism of interrupt from this pin will cause
    /// this stream to stop returning events.
    fn button_events(
        &mut self,
        pressed_level: Level,
        hold_timeout: Option<Duration>,
    ) -> Result<ButtonEventStream>;
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

    fn button_events(
        &mut self,
        pressed_level: Level,
        hold_timeout: Option<Duration>,
    ) -> Result<ButtonEventStream> {
        ButtonEventStream::new(self, pressed_level, hold_timeout)
    }
}
