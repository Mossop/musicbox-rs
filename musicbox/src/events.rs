use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Instant;

use futures::stream::Stream;
use serde::{Deserialize, Serialize};

use crate::player::Track;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Startup,
    Shutdown,
    Error(String),
    Input(InputEvent),
    Start((Track, Instant)),
    Finish((Track, Instant)),
    Pause((Track, Instant)),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputEvent {
    PreviousTrack,
    NextTrack,
    PlayPause,
    VolumeUp,
    VolumeDown,
    StartPlaylist(String),
    RestartPlaylist(String),
    Shutdown,
    Reload,
}

pub trait EventListener {
    fn event(&mut self, event: &Event);
}

// Explicitely don't implement clone.
#[derive(Default)]
struct LoopReferences {
    streams: Vec<Pin<Box<dyn Stream<Item = Event>>>>,
    listeners: Vec<Box<dyn EventListener>>,
}

// Explicitely don't implement clone.
#[derive(Default)]
struct LoopPending {
    references: Mutex<LoopReferences>,
    has_pending: AtomicBool,
}

/// An asynchronous event loop.
///
/// Polls for events from asynchronous event streams and provides a means to
/// synchronously send and listen for events.
///
///
#[derive(Clone)]
pub struct EventLoop {
    pending: Arc<LoopPending>,
    running: Arc<AtomicBool>,
}

impl EventLoop {
    pub fn new() -> EventLoop {
        EventLoop {
            pending: Default::default(),
            running: Default::default(),
        }
    }

    pub fn add_event_stream<S>(&mut self, stream: S)
    where
        S: Stream<Item = Event> + 'static,
    {
        // Push the stream onto the list of pending streams.
        let mut references = self.pending.references.lock().unwrap();
        references.streams.push(Box::pin(stream));

        // Notify that there are pending streams.
        self.pending.has_pending.store(true, Ordering::SeqCst);
    }

    pub fn add_listener<L>(&mut self, listener: L)
    where
        L: EventListener + 'static,
    {
        // Push the listener onto the list of pending listener.
        let mut references = self.pending.references.lock().unwrap();
        references.listeners.push(Box::new(listener));

        // Notify that there are pending streams.
        self.pending.has_pending.store(true, Ordering::SeqCst);
    }

    pub fn run(self) -> LoopFuture {
        if self.running.swap(true, Ordering::SeqCst) {
            panic!("Cannot run an event loop twice.");
        }

        let mut references = self.pending.references.lock().unwrap();
        self.pending.has_pending.store(false, Ordering::SeqCst);

        LoopFuture {
            references: LoopReferences {
                streams: references.streams.drain(..).collect(),
                listeners: references.listeners.drain(..).collect(),
            },
            pending: self.pending.clone(),
            started: false,
        }
    }
}

pub struct LoopFuture {
    references: LoopReferences,
    pending: Arc<LoopPending>,
    started: bool,
}

impl LoopFuture {
    fn handle_event(&mut self, event: &Event) {
        for listener in self.references.listeners.iter_mut() {
            listener.event(event);
        }
    }
}

impl Future for LoopFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {
        if !self.started {
            self.started = true;
            self.handle_event(&Event::Startup);
        }

        // Check for pending items.
        if self.pending.has_pending.load(Ordering::SeqCst) {
            let (streams, listeners): (
                Vec<Pin<Box<dyn Stream<Item = Event>>>>,
                Vec<Box<dyn EventListener>>,
            ) = {
                let mut references = self.pending.references.lock().unwrap();
                let result = (
                    references.streams.drain(..).collect(),
                    references.listeners.drain(..).collect(),
                );

                // Do this while references is still locked.
                self.pending.has_pending.store(false, Ordering::SeqCst);
                result
            };

            self.references.streams.extend(streams);
            self.references.listeners.extend(listeners);
        }

        let mut events: Vec<Event> = Default::default();
        for stream in self.references.streams.iter_mut() {
            match stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(event)) => {
                    // Re-visit this stream.
                    events.push(event);
                }
                Poll::Ready(None) => {
                    // Nothing left from this stream. Drop it.
                }
                Poll::Pending => {
                    // Re-visit this stream.
                }
            }
        }

        for event in events {
            self.handle_event(&event);

            if event == Event::Input(InputEvent::Shutdown) {
                self.handle_event(&Event::Shutdown);
                return Poll::Ready(());
            }
        }

        // If there are streams left then we aren't done yet.
        if !self.references.streams.is_empty() {
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}
