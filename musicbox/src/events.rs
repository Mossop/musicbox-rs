use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use futures::stream::{FusedStream, Stream};
use serde::{Deserialize, Serialize};

use crate::track::Track;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    Error(String),
    PlaybackStarted(Track),
    PlaybackEnded,
    PlaybackDuration(Duration),
    Shutdown,
}

#[derive(Clone, Debug)]
pub struct Message<T> {
    pub payload: T,
    pub instant: Instant,
}

impl<T> Message<T> {
    pub fn new(instant: Instant, payload: T) -> Message<T> {
        Message { instant, payload }
    }
}

impl<T> From<T> for Message<T> {
    fn from(payload: T) -> Message<T> {
        Message {
            payload,
            instant: Instant::now(),
        }
    }
}

#[derive(Default)]
struct MessageStreamState<T> {
    streams: Vec<Pin<Box<dyn Stream<Item = Message<T>>>>>,
    waker: Option<Waker>,
}

// An asynchronous event loop.
///
/// Polls for events from asynchronous event streams.
pub struct MessageStream<T> {
    state: Arc<Mutex<MessageStreamState<T>>>,
}

impl<T> MessageStream<T> {
    pub fn add_stream<S>(&self, stream: S)
    where
        S: Stream<Item = Message<T>> + 'static,
    {
        // Push the stream onto the list of pending streams.
        let mut streams = self.state.lock().unwrap();
        streams.streams.push(Box::pin(stream));
        if let Some(waker) = streams.waker.take() {
            waker.wake();
        }
    }
}

impl<T> Default for MessageStream<T> {
    fn default() -> MessageStream<T> {
        MessageStream {
            state: Arc::new(Mutex::new(MessageStreamState::<T> {
                streams: Default::default(),
                waker: None,
            })),
        }
    }
}

impl<T> Stream for MessageStream<T> {
    type Item = Message<T>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Message<T>>> {
        let mut state = self.state.lock().unwrap();

        // Loop over all the streams moving those checked to the back so on the
        // next check we pick up where we left off. Give up after checking them
        // all and wait for wake-up.
        for _ in 0..state.streams.len() {
            match state.streams.pop() {
                Some(mut stream) => match stream.as_mut().poll_next(cx) {
                    Poll::Ready(Some(message)) => {
                        state.streams.insert(0, stream);
                        return Poll::Ready(Some(message));
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

impl<T> FusedStream for MessageStream<T> {
    fn is_terminated(&self) -> bool {
        self.state.lock().unwrap().streams.is_empty()
    }
}

#[derive(Default)]
pub struct SyncMessageChannel<T> {
    messages: Vec<Message<T>>,
    waker: Option<Waker>,
}

impl<T> SyncMessageChannel<T> {
    pub fn init() -> (MessageSender<T>, impl Stream<Item = Message<T>>) {
        let channel = SyncMessageChannel {
            messages: Vec::new(),
            waker: None,
        };

        let channel = Arc::new(Mutex::new(channel));
        (
            MessageSender {
                channel: channel.clone(),
            },
            MessageReceiver { channel },
        )
    }
}

struct MessageReceiver<T> {
    channel: Arc<Mutex<SyncMessageChannel<T>>>,
}

impl<T> Stream for MessageReceiver<T> {
    type Item = Message<T>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Message<T>>> {
        match self.channel.lock() {
            Ok(ref mut channel) => {
                if channel.messages.is_empty() {
                    channel.waker = Some(cx.waker().clone());
                    return Poll::Pending;
                }

                Poll::Ready(Some(channel.messages.remove(0)))
            }
            Err(_e) => Poll::Ready(None),
        }
    }
}

#[derive(Clone)]
pub struct MessageSender<T> {
    channel: Arc<Mutex<SyncMessageChannel<T>>>,
}

impl<T> MessageSender<T> {
    pub fn send(&self, message: Message<T>) {
        if let Ok(ref mut channel) = self.channel.lock() {
            channel.messages.push(message);

            if let Some(waker) = channel.waker.take() {
                waker.wake();
            }
        }
    }
}
