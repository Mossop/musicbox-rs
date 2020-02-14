use std::fs::File;
use std::iter::Iterator;
use std::time::Duration;

use cpal::traits::DeviceTrait;
use cpal::Device;
use log::{error, info, trace};
use rodio::decoder::Decoder;
use rodio::source::{from_iter, UniformSourceIterator};
use rodio::{default_output_device, output_devices, Sample, Sink, Source};

use crate::events::{Event, MessageSender};
use crate::track::Track;
use crate::MusicResult;

pub struct PlaylistSource {
    event_sender: MessageSender<Event>,
    tracks: Vec<Track>,
}

impl PlaylistSource {
    pub fn init(
        tracks: Vec<Track>,
        sender: MessageSender<Event>,
    ) -> impl Source<Item = i16> + Send {
        let iterator = PlaylistSource {
            event_sender: sender,
            tracks,
        };

        from_iter(iterator)
    }
}

impl Iterator for PlaylistSource {
    type Item = Box<dyn Source<Item = i16> + Send>;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.tracks.is_empty() {
            let track = self.tracks.remove(0);

            match track.decode() {
                Ok(decoded) => {
                    let uniform =
                        UniformSourceIterator::<Decoder<File>, i16>::new(decoded, 1, 22050);
                    let sender = self.event_sender.clone();
                    let mut millis = 0;
                    let periodic = uniform.periodic_access(Duration::from_millis(500), move |_s| {
                        millis += 500;
                        sender.send(Event::PlaybackDuration(Duration::from_millis(millis)).into());
                    });
                    self.event_sender.send(Event::PlaybackStarted(track).into());
                    return Some(Box::new(periodic));
                }
                Err(e) => {
                    error!("Failed to decode '{}': {}", track.path().display(), e);
                }
            }
        }

        self.event_sender.send(Event::PlaybackEnded.into());

        None
    }
}

pub struct Player {
    sink: Option<Sink>,
    device: Device,
    volume: f32,
}

impl Player {
    pub fn new(volume: f32) -> MusicResult<Player> {
        let devices =
            output_devices().map_err(|_e| String::from("Unable to enumerate output devices."))?;
        for device in devices {
            let name = device
                .name()
                .map_err(|_e| String::from("Unable to retrieve device name."))?;
            trace!("Found device '{}'", name);
        }

        if let Some(device) = default_output_device() {
            info!(
                "Using device '{}'.",
                device
                    .name()
                    .map_err(|_e| String::from("Unable to retrieve device name."))?,
            );

            Ok(Player {
                sink: None,
                device,
                volume,
            })
        } else {
            Err(String::from("Unable to find default output device."))
        }
    }

    pub fn start<S>(&mut self, source: S)
    where
        S: Source + Send + 'static,
        S::Item: Sample,
        S::Item: Send,
    {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }

        let sink = Sink::new(&self.device);
        sink.set_volume(self.volume);
        sink.append(source);

        self.sink = Some(sink);
    }

    pub fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
    }

    pub fn play(&self) {
        if let Some(ref sink) = self.sink {
            sink.play();
        }
    }

    pub fn pause(&self) {
        if let Some(ref sink) = self.sink {
            sink.pause();
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume;
        if let Some(ref sink) = self.sink {
            sink.set_volume(volume);
        }
    }
}
