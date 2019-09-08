use std::pin::Pin;
use std::task::{Context, Poll};

use futures::stream::Stream;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    Error(String),
    PreviousTrack,
    NextTrack,
    PlayPause,
    VolumeUp,
    VolumeDown,
    StartPlaylist(String),
    RestartPlaylist(String),
    Shutdown,
}

pub struct EventStream {
    used: bool,
    streams: Vec<Pin<Box<dyn Stream<Item = Event>>>>,
}

impl EventStream {
    pub fn new() -> EventStream {
        EventStream {
            used: false,
            streams: Default::default(),
        }
    }

    pub fn add_event_stream<S>(&mut self, stream: S)
    where
        S: Stream<Item = Event> + 'static,
    {
        if self.used {
            panic!("Adding new streams after use is not supported.");
        }

        self.streams.push(Box::pin(stream));
    }
}

impl Stream for EventStream {
    type Item = Event;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Event>> {
        self.used = true;
        let mut count = self.streams.len();

        while count > 0 {
            let mut stream = self.streams.remove(0);
            match stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(event)) => {
                    // Re-visit this stream.
                    self.streams.push(stream);
                    return Poll::Ready(Some(event));
                }
                Poll::Ready(None) => {
                    // Nothing left from this stream. Drop it.
                }
                Poll::Pending => {
                    // Re-visit this stream.
                    self.streams.push(stream);
                }
            }

            count -= 1;
        }

        // If there are streams left then they all returned Poll::Pending and so
        // will wake up the task when ready.
        if !self.streams.is_empty() {
            Poll::Pending
        } else {
            Poll::Ready(None)
        }
    }
}
