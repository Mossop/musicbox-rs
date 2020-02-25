use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Serialize, Serializer};

use crate::playlist::StoredPlaylist;
use crate::track::Track;

#[derive(Serialize)]
pub struct PlayState {
    position: usize,
    duration: Duration,
    paused: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InnerState {
    stored_playlists: HashMap<String, StoredPlaylist>,
    playlist: Vec<Track>,
    play_state: Option<PlayState>,
    volume: f64,
}

#[derive(Clone)]
pub struct AppState {
    state: Arc<Mutex<InnerState>>,
}

impl Serialize for AppState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.state.lock().unwrap().serialize(serializer)
    }
}

#[derive(Clone)]
pub struct MutableAppState {
    state: Arc<Mutex<InnerState>>,
}

impl MutableAppState {
    pub fn new(playlists: Vec<StoredPlaylist>) -> MutableAppState {
        let stored_playlists = playlists
            .into_iter()
            .map(|playlist| (playlist.name(), playlist))
            .collect();

        MutableAppState {
            state: Arc::new(Mutex::new(InnerState {
                stored_playlists,
                playlist: Default::default(),
                play_state: None,
                volume: 0.0,
            })),
        }
    }

    pub fn as_immutable(&self) -> AppState {
        AppState {
            state: self.state.clone(),
        }
    }

    pub fn playlist(&self) -> Vec<Track> {
        self.state.lock().unwrap().playlist.clone()
    }

    pub fn volume(&self) -> f64 {
        self.state.lock().unwrap().volume
    }

    pub fn set_volume(&mut self, volume: f64) {
        self.state.lock().unwrap().volume = volume
    }

    pub fn paused(&self) -> Option<bool> {
        self.state
            .lock()
            .unwrap()
            .play_state
            .as_ref()
            .map(|state| state.paused)
    }

    pub fn set_paused(&mut self, paused: bool) {
        let mut state = self.state.lock().unwrap();
        if let Some(ref mut play_state) = state.play_state {
            play_state.paused = paused;
        }
    }

    pub fn playback_position(&self) -> Option<usize> {
        self.state
            .lock()
            .unwrap()
            .play_state
            .as_ref()
            .map(|state| state.position)
    }

    pub fn playback_duration(&self) -> Option<Duration> {
        self.state
            .lock()
            .unwrap()
            .play_state
            .as_ref()
            .map(|state| state.duration)
    }

    pub fn set_playback_position(&mut self, position: Option<usize>) {
        let mut state = self.state.lock().unwrap();
        state.play_state = position.map(|position| PlayState {
            position,
            duration: Default::default(),
            paused: false,
        });
    }

    pub fn is_playing_playlist(&self, name: &str) -> bool {
        let state = self.state.lock().unwrap();
        if state.play_state.is_some() {
            if let Some(playlist) = state.stored_playlists.get(name) {
                playlist.equals(&state.playlist)
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn stored_playlist(&self, name: &str) -> Option<StoredPlaylist> {
        self.state
            .lock()
            .unwrap()
            .stored_playlists
            .get(name)
            .cloned()
    }

    pub fn set_playlist(&mut self, tracks: Vec<Track>) {
        self.state.lock().unwrap().playlist = tracks;
    }
}
