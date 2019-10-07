use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use futures::stream::Stream;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    Error(String),
    PreviousTrack,
    NextTrack,
    PlayPause,
    VolumeUp,
    VolumeDown,
    StartPlaylist(String, bool),
    Shutdown,
    Reload,
    Status,
}

#[derive(Default)]
struct State {
    streams: Vec<Pin<Box<dyn Stream<Item = Event>>>>,
    waker: Option<Waker>,
}

// An asynchronous event loop.
///
/// Polls for events from asynchronous event streams.
pub struct EventStream {
    state: Arc<Mutex<State>>,
}

impl EventStream {
    pub fn new() -> EventStream {
        EventStream {
            state: Default::default(),
        }
    }

    pub fn add_event_stream<S>(&self, stream: S)
    where
        S: Stream<Item = Event> + 'static,
    {
        // Push the stream onto the list of pending streams.
        let mut streams = self.state.lock().unwrap();
        streams.streams.push(Box::pin(stream));
        if let Some(waker) = streams.waker.take() {
            waker.wake();
        }
    }
}

impl Stream for EventStream {
    type Item = Event;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Event>> {
        let mut state = self.state.lock().unwrap();

        // Loop over all the streams moving those checked to the back so on the
        // next check we pick up where we left off. Give up after checking them
        // all and wait for wake-up.
        for _ in 0..state.streams.len() {
            match state.streams.pop() {
                Some(mut stream) => match stream.as_mut().poll_next(cx) {
                    Poll::Ready(Some(event)) => {
                        state.streams.insert(0, stream);
                        return Poll::Ready(Some(event));
                    }
                    Poll::Ready(None) => {
                        // Drop this stream.
                    }
                    Poll::Pending => {
                        state.streams.insert(0, stream);
                    }
                },
                None => return Poll::Ready(None),
            }
        }

        if state.streams.is_empty() {
            Poll::Ready(None)
        } else {
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
