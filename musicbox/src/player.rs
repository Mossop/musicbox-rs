use std::fs::File;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use log::warn;
use rodio::decoder::Decoder;
use rodio::source::{Source, UniformSourceIterator, Zero};
use rodio::Sample;
use serde::{Deserialize, Serialize};

use crate::events::EventLoop;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    path: PathBuf,
}

impl Track {
    pub fn decode(&self) -> Result<impl Source<Item = i16>, String> {
        match File::open(&self.path) {
            Ok(f) => match Decoder::new(f) {
                Ok(s) => Ok(s),
                Err(e) => Err(e.to_string()),
            },
            Err(e) => Err(e.to_string()),
        }
    }
}

#[derive(Debug, Default)]
struct Playlist {
    position: usize,
    tracks: Vec<Track>,
}

pub struct Player {
    playlist: Arc<Mutex<Playlist>>,
    volume: f32,
}

impl Player {
    pub fn new(event_loop: &mut EventLoop) -> Player {
        Player {
            playlist: Default::default(),
            volume: 1.0,
        }
    }

    pub fn start_playback(&mut self, tracks: Vec<Track>) {}

    pub fn play(&self) {}

    pub fn pause(&self) {}

    pub fn is_paused(&self) -> bool {
        false
    }

    pub fn next(&mut self) {}

    pub fn previous(&mut self) {}

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_volume(&mut self, volume: f32) {}
}

struct PlayerSource {
    sample_rate: u32,
    channels: u16,
    playlist: Option<Playlist>,
    inner: Box<dyn Source<Item = f32>>,
    paused: bool,
    volume: f32,
}

impl PlayerSource {
    fn new(channels: u16, sample_rate: u32) -> PlayerSource {
        PlayerSource {
            inner: Box::new(Zero::new(channels, sample_rate)),
            sample_rate,
            channels,
            volume: 1.0,
            paused: false,
            playlist: Default::default(),
        }
    }

    fn convert_source<I>(&self, source: I) -> impl Source<Item = f32>
    where
        I: Source,
        <I as std::iter::Iterator>::Item: Sample,
    {
        UniformSourceIterator::<I, f32>::new(source, self.channels, self.sample_rate)
    }
}

impl Source for PlayerSource {
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.inner.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

impl Iterator for PlayerSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        // TODO check for pending command

        // Simple case first.
        if self.paused {
            return Some(0.0);
        }

        // Common case, keep playing.
        if let Some(f) = self.inner.next() {
            return Some(f * self.volume);
        }

        // Track has ended. Try to find a new one.
        if let Some(ref mut playlist) = self.playlist {
            let instant = Instant::now();

            if let Some(ref track) = playlist.tracks.get(playlist.position) {
                // Send the end track event.
            }

            playlist.position += 1;

            if let Some(ref track) = playlist.tracks.get(playlist.position) {
                // Send the start track event.
            } else {
                self.playlist = None;
                self.inner = Box::new(Zero::new(self.channels, self.sample_rate));
            }
        } else {
            // Odd, generally the zero source should never end.
            warn!("Reached end of source with no current playlist.");
            self.inner = Box::new(Zero::new(self.channels, self.sample_rate));
        }

        self.next()
    }
}
