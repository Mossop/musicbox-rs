use std::convert::Infallible;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use futures::sink::Sink;
use futures::stream::{FusedStream, Stream};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Event {
    PlaylistUpdated,
    PlaybackStarted,
    PlaybackPaused,
    PlaybackUnpaused,
    PlaybackEnded,
    PlaybackPosition(Duration),
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

struct Channel<T> {
    messages: Vec<Message<T>>,
    waker: Option<Waker>,
}

impl<T> Default for Channel<T> {
    fn default() -> Self {
        Channel {
            messages: Vec::new(),
            waker: None,
        }
    }
}

#[derive(Clone)]
pub struct MessageSender<T>
where
    T: Clone,
{
    channels: Arc<Mutex<Vec<Arc<Mutex<Channel<T>>>>>>,
}

impl<T> MessageSender<T>
where
    T: Clone,
{
    pub fn new() -> MessageSender<T> {
        MessageSender {
            channels: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn send(&self, message: Message<T>) {
        let channels = self.channels.lock().unwrap();
        for mut channel in channels.iter().map(|c| c.lock().unwrap()) {
            channel.messages.push(message.clone());
            if let Some(waker) = channel.waker.take() {
                waker.wake();
            }
        }
    }

    pub fn receiver(&self) -> MessageReceiver<T> {
        let mut channels = self.channels.lock().unwrap();
        let channel = Arc::new(Mutex::new(Default::default()));
        channels.push(channel.clone());

        MessageReceiver {
            channels: self.channels.clone(),
            channel,
        }
    }
}

impl<T> Default for MessageSender<T>
where
    T: Clone,
{
    fn default() -> Self {
        MessageSender::new()
    }
}

impl<T> Sink<Message<T>> for MessageSender<T>
where
    T: Clone,
{
    type Error = Infallible;

    fn poll_ready(self: Pin<&mut Self>, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, item: Message<T>) -> Result<(), Self::Error> {
        self.send(item);
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

pub struct MessageReceiver<T>
where
    T: Clone,
{
    channels: Arc<Mutex<Vec<Arc<Mutex<Channel<T>>>>>>,
    channel: Arc<Mutex<Channel<T>>>,
}

impl<T> MessageReceiver<T>
where
    T: Clone,
{
    pub fn new() -> MessageReceiver<T> {
        let channel = Arc::new(Mutex::new(Default::default()));
        let mut vec = Vec::new();
        vec.push(channel.clone());

        MessageReceiver {
            channels: Arc::new(Mutex::new(vec)),
            channel,
        }
    }

    pub fn sender(&self) -> MessageSender<T> {
        MessageSender {
            channels: self.channels.clone(),
        }
    }
}

impl<T> Default for MessageReceiver<T>
where
    T: Clone,
{
    fn default() -> Self {
        MessageReceiver::new()
    }
}

impl<T> Clone for MessageReceiver<T>
where
    T: Clone,
{
    fn clone(&self) -> MessageReceiver<T> {
        let mut channels = self.channels.lock().unwrap();
        let channel = Arc::new(Mutex::new(Default::default()));
        channels.push(channel.clone());

        MessageReceiver {
            channels: self.channels.clone(),
            channel,
        }
    }
}

impl<T> Drop for MessageReceiver<T>
where
    T: Clone,
{
    fn drop(&mut self) {
        let mut channels = self.channels.lock().unwrap();
        for (i, ref channel) in channels.iter().enumerate() {
            if Arc::ptr_eq(channel, &self.channel) {
                channels.remove(i);
                return;
            }
        }
    }
}

impl<T> Stream for MessageReceiver<T>
where
    T: Clone,
{
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

impl<T> FusedStream for MessageReceiver<T>
where
    T: Clone,
{
    fn is_terminated(&self) -> bool {
        false
    }
}
