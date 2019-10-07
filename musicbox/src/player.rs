use std::fmt;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::{DefaultFormatError, Format, Sample};
use log::{debug, error, info, trace};
use rodio::decoder::Decoder;
use rodio::source::Source;
use rodio::{default_output_device, play_raw};
use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

const PREVIOUS_DELAY: u64 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    path: PathBuf,
    title: String,
}

impl Track {
    pub fn new(path: &Path) -> Track {
        let title = match path.file_stem() {
            Some(name) => name.to_string_lossy().to_string(),
            None => path.display().to_string(),
        };

        Track {
            path: path.to_owned(),
            title,
        }
    }

    pub fn title(&self) -> String {
        self.title.clone()
    }

    pub fn decode(&self) -> Result<Decoder<File>, String> {
        Decoder::new(File::open(&self.path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }
}

impl fmt::Display for Track {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.path.display().fmt(f)
    }
}

#[derive(Debug, Default)]
struct Playlist {
    position: usize,
    tracks: Vec<Track>,
}

#[derive(Default)]
struct PlayerState {
    playlist: Option<Playlist>,
    paused: bool,
    volume: f32,
    resync: bool,

    previous_duration: Option<Duration>,
    last_start: Option<Instant>,
}

impl PlayerState {
    pub fn current_track(&self) -> Option<Track> {
        self.playlist
            .as_ref()
            .and_then(|p| p.tracks.get(p.position).cloned())
    }

    pub fn next_track(&mut self) -> Option<Track> {
        debug!("Starting next track.");
        self.previous_duration = None;
        self.last_start = None;

        if let Some(ref mut playlist) = self.playlist {
            playlist.position += 1;

            if playlist.position >= playlist.tracks.len() {
                info!("Reached end of playlist.");
                self.playlist = None;
            }
        }

        self.current_track()
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn track_position(&self) -> Option<Duration> {
        match (self.last_start, self.previous_duration) {
            (Some(instant), Some(duration)) => Some((Instant::now() - instant) + duration),
            (Some(instant), None) => Some(Instant::now() - instant),
            (None, Some(duration)) => Some(duration),
            (None, None) => None,
        }
    }
}

pub struct Player {
    state: Arc<Mutex<PlayerState>>,
}

impl Serialize for Player {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let state = self.state.lock().unwrap();

        let mut map = serializer.serialize_map(Some(5))?;
        map.serialize_entry("current_track", &state.current_track())?;
        map.serialize_entry("duration", &state.track_position())?;
        map.serialize_entry("volume", &state.volume)?;
        map.serialize_entry("paused", &state.paused)?;
        map.serialize_entry("resync", &state.resync)?;
        map.end()
    }
}

impl Player {
    pub fn new() -> Result<Player, String> {
        if let Some(device) = default_output_device() {
            let format = match device.default_output_format() {
                Ok(f) => f,
                Err(DefaultFormatError::DeviceNotAvailable) => {
                    return Err(String::from("Device not available."))
                }
                Err(DefaultFormatError::StreamTypeNotSupported) => {
                    return Err(String::from("No supported output formats."))
                }
            };

            info!(
                "Using device '{}'. Sample rate: {}, channels: {}.",
                device.name(),
                format.sample_rate.0,
                format.channels
            );

            let state = Arc::new(Mutex::new(PlayerState {
                playlist: None,
                paused: false,
                volume: 1.0,
                resync: false,

                previous_duration: None,
                last_start: None,
            }));

            let source = PlayerSource::new(&format, state.clone());
            play_raw(&device, source);

            let player = Player { state };

            Ok(player)
        } else {
            Err(String::from("No output device exists."))
        }
    }

    pub fn start_tracks(&self, tracks: Vec<Track>, force: bool) {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut playlist) = state.playlist {
            if !force && playlist.tracks == tracks {
                info!("Not replacing tracks that are already playing.");
                state.paused = false;
                return;
            }
        }

        info!("Starting {} tracks.", tracks.len());
        state.playlist = Some(Playlist {
            tracks,
            position: 0,
        });
        state.paused = false;
        state.resync = true;
    }

    pub fn play_pause(&mut self) {
        if self.is_paused() {
            self.play();
        } else {
            self.pause();
        }
    }

    pub fn play(&self) {
        self.state.lock().unwrap().paused = false;
    }

    pub fn pause(&self) {
        self.state.lock().unwrap().paused = true;
    }

    pub fn is_paused(&self) -> bool {
        self.state.lock().unwrap().paused()
    }

    pub fn next(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut playlist) = state.playlist {
            if playlist.position + 1 < playlist.tracks.len() {
                playlist.position += 1;
                state.resync = true;
            }
        }
    }

    pub fn previous(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(duration) = state.track_position() {
            if let Some(ref mut playlist) = state.playlist {
                if playlist.position > 0 && duration.as_secs() < PREVIOUS_DELAY {
                    playlist.position -= 1;
                }
                state.resync = true;
            }
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.state.lock().unwrap().volume = volume;
    }

    pub fn volume(&self) -> f32 {
        self.state.lock().unwrap().volume
    }
}

struct PlayerSource {
    sample_rate: u32,
    channels: u16,
    inner: Option<Decoder<File>>,
    state: Arc<Mutex<PlayerState>>,
}

impl PlayerSource {
    fn new(format: &Format, state: Arc<Mutex<PlayerState>>) -> PlayerSource {
        PlayerSource {
            inner: None,
            sample_rate: format.sample_rate.0,
            channels: format.channels,
            state,
        }
    }

    fn start_play(&mut self, track: Track) {
        info!("Starting track '{}'.", track.title());
        let mut state = self.state.lock().unwrap();
        state.previous_duration = None;
        let decoder = match track.decode() {
            Ok(d) => d,
            Err(e) => {
                error!("Error decoding {}: {}", track.title(), e);
                self.inner = None;
                state.last_start = None;
                return;
            }
        };
        // .map(|source| UniformSourceIterator::new(source, self.channels, self.sample_rate));
        self.inner = Some(decoder);
        state.last_start = Some(Instant::now());
    }
}

impl Source for PlayerSource {
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.as_ref().and_then(|i| i.current_frame_len())
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.as_ref().and_then(|i| i.total_duration())
    }
}

impl Iterator for PlayerSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        loop {
            let next: Option<Track> = {
                let mut state = self.state.lock().unwrap();

                if state.resync {
                    debug!("Syncing Source.");
                    state.resync = false;
                    state.current_track()
                } else if !state.paused {
                    if let Some(ref mut inner) = self.inner {
                        if let Some(sample) = inner.next() {
                            return Some(sample.to_f32() * state.volume);
                        }

                        state.next_track()
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            if let Some(track) = next {
                self.start_play(track);
            } else {
                return Some(0.0);
            }
        }
    }
}
