use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::id;
use std::time::Duration;

use daemonize::{Daemonize, DaemonizeError};
use futures::channel::mpsc::{channel, Sender};
use futures::compat::*;
use futures::future::{join_all, poll_fn, ready};
use futures::select;
use futures::stream::{Stream, StreamExt};
use log::{error, info};
use rppal::gpio::Gpio;
use serde_json::from_reader;
use signal_hook::iterator::Signals;
use tokio::runtime::Runtime;

use crate::events::{Command, Event, Message, MessageSender, MessageStream, SyncMessageChannel};
use crate::hardware::event_stream;
use crate::hw_config::HwConfig;
use crate::player::{Player, PlaylistSource};
use crate::playlist::StoredPlaylist;
use crate::track::Track;
use crate::ResultErrorLogger;
use crate::{MusicResult, VoidResult};

const HW_CONFIG_NAME: &str = "hwconfig.json";
const VOLUME_INTERVAL: f32 = 0.1;

pub struct PlayState {
    position: usize,
    duration: Duration,
    paused: bool,
}

pub struct MusicBox {
    events: MessageStream<Event>,
    commands: MessageStream<Command>,
    stored_playlists: HashMap<String, StoredPlaylist>,
    event_listeners: Vec<Sender<Message<Event>>>,
    playlist: Vec<Track>,
    play_state: Option<PlayState>,
    player: Player,
    volume: f32,
    audio_events: MessageSender<Event>,
}

impl MusicBox {
    pub fn new(volume: f32) -> MusicResult<MusicBox> {
        let (sender, receiver) = SyncMessageChannel::init();

        let mut music_box = MusicBox {
            volume,
            player: Player::new(volume)?,
            events: Default::default(),
            commands: Default::default(),
            stored_playlists: Default::default(),
            event_listeners: Default::default(),
            playlist: Default::default(),
            play_state: Default::default(),
            audio_events: sender,
        };

        music_box.add_event_stream(receiver);
        Ok(music_box)
    }

    pub fn add_event_stream<S>(&mut self, stream: S)
    where
        S: Stream<Item = Message<Event>> + 'static,
    {
        self.events.add_stream(stream)
    }

    pub fn add_command_stream<S>(&mut self, stream: S)
    where
        S: Stream<Item = Message<Command>> + 'static,
    {
        self.commands.add_stream(stream)
    }

    fn play(&mut self, tracks: Vec<Track>, position: usize) {
        self.play_state = Some(PlayState {
            paused: false,
            position,
            duration: Duration::from_secs(0),
        });

        self.player
            .start(PlaylistSource::init(tracks, self.audio_events.clone()));
    }

    async fn send_event(sender: &mut Sender<Message<Event>>, event: Message<Event>) {
        let ready = poll_fn(|cx| sender.poll_ready(cx));
        if let Err(_e) = ready.await {
            return;
        }
        if let Err(_e) = sender.start_send(event) {
            return;
        }
    }

    async fn dispatch_event(&mut self, event: Message<Event>) {
        join_all(
            self.event_listeners
                .iter_mut()
                .map(|sender| MusicBox::send_event(sender, event.clone())),
        )
        .await;
    }

    async fn handle_command(&mut self, command: Message<Command>) {
        info!("Saw command {:?}", command.payload);

        match command.payload {
            Command::PreviousTrack => {
                let (tracks, position) = match self.play_state {
                    Some(ref mut state) => {
                        if state.position > 0 && state.duration.as_secs() < 2 {
                            state.position -= 1;
                        }
                        (
                            self.playlist.iter().skip(state.position).cloned().collect(),
                            state.position,
                        )
                    }
                    None => return,
                };
                self.play(tracks, position);
            }
            Command::NextTrack => {
                let (tracks, position) = match self.play_state {
                    Some(ref mut state) => {
                        state.position += 1;
                        if state.position >= self.playlist.len() {
                            self.play_state = None;
                            self.player.stop();
                            return;
                        }
                        state.paused = false;
                        (
                            self.playlist.iter().skip(state.position).cloned().collect(),
                            state.position,
                        )
                    }
                    None => return,
                };
                self.play(tracks, position);
            }
            Command::PlayPause => {
                if let Some(ref mut state) = self.play_state {
                    state.paused = !state.paused;
                    if state.paused {
                        self.player.pause();
                    } else {
                        self.player.play();
                    }
                } else if !self.playlist.is_empty() {
                    self.play(self.playlist.clone(), 0);
                }
            }
            Command::VolumeUp => {
                self.volume += VOLUME_INTERVAL;
                if self.volume > 1.0 {
                    self.volume = 1.0;
                }
                self.player.set_volume(self.volume);
            }
            Command::VolumeDown => {
                self.volume -= VOLUME_INTERVAL;
                if self.volume < 0.0 {
                    self.volume = 0.0;
                }
                self.player.set_volume(self.volume);
            }
            Command::Shutdown => {
                info!("Music box clean shutdown.");
                self.player.stop();
                self.dispatch_event(Event::Shutdown.into()).await;
            }
            Command::StartPlaylist(name, force) => {
                if let Some(playlist) = self.stored_playlists.get(&name) {
                    if playlist.equals(&self.playlist) && !force {
                        return;
                    }

                    self.playlist = playlist.tracks();
                    self.play_state = Some(PlayState {
                        position: 0,
                        duration: Duration::from_secs(0),
                        paused: false,
                    });

                    self.play(self.playlist.clone(), 0);
                } else {
                    error!(
                        "Received a request to start playlist {} but that list does not exist.",
                        name
                    );
                }
            }
            Command::Reload => {}
            Command::Status => {}
        }
    }

    async fn handle_event(&mut self, event: Message<Event>) {
        info!("Saw event {:?}", event.payload);

        self.dispatch_event(event).await;
    }

    pub fn get_event_stream(&mut self) -> impl Stream<Item = Message<Event>> {
        let (sender, receiver) = channel(20);
        self.event_listeners.push(sender);
        receiver
    }

    async fn run(mut self) -> VoidResult {
        info!("Music box startup. Running as process {}.", id());

        loop {
            select! {
                c = self.commands.next() => if let Some(command) = c {
                    self.handle_command(command.clone()).await;
                    if command.payload == Command::Shutdown {
                        break;
                    }
                },
                e = self.events.next() => if let Some(event) = e {
                    self.handle_event(event).await
                },
                complete => break,
            }
        }

        Ok(())
    }

    // Should perform any privileged actions before the daemon reduces
    // privileges.
    async fn init(data_dir: &Path) -> MusicResult<MusicBox> {
        let mut hw_config_file = data_dir.to_owned();
        hw_config_file.push(HW_CONFIG_NAME.parse::<PathBuf>().unwrap());

        let file = File::open(&hw_config_file)
            .map_err(|_| format!("Could not open config file '{}'.", hw_config_file.display()))?;

        let hw_config: HwConfig = from_reader(BufReader::new(file)).map_err(|e| {
            format!(
                "Unable to parse config file '{}': {}",
                hw_config_file.display(),
                e
            )
        })?;

        let gpio = Gpio::new().map_err(|e| e.to_string())?;

        let mut music_box: MusicBox = MusicBox::new(0.5)?;

        music_box
            .stored_playlists
            .reserve(hw_config.playlists.len());
        for config in hw_config.playlists {
            let (playlist, stream) = match StoredPlaylist::new(data_dir, &gpio, &config).await {
                Ok(p) => p,
                Err(_) => continue,
            };
            music_box.stored_playlists.insert(playlist.name(), playlist);
            music_box.add_command_stream(stream);
        }

        for event in hw_config.inputs {
            music_box.add_command_stream(event_stream(
                &gpio,
                &event.button,
                event.action.clone(),
                None,
            )?);
        }

        match Signals::new(&[
            signal_hook::SIGHUP,
            signal_hook::SIGTERM,
            signal_hook::SIGINT,
            signal_hook::SIGQUIT,
            signal_hook::SIGUSR1,
            signal_hook::SIGUSR2,
        ])
        .and_then(|s| s.into_async())
        {
            Ok(signals) => {
                music_box.add_command_stream(signals.compat().filter_map(|r| match r {
                    Ok(signal_hook::SIGHUP) => ready(Some(Command::Reload.into())),
                    Ok(signal_hook::SIGTERM) => ready(Some(Command::Shutdown.into())),
                    Ok(signal_hook::SIGINT) => ready(Some(Command::Shutdown.into())),
                    Ok(signal_hook::SIGQUIT) => ready(Some(Command::Shutdown.into())),
                    Ok(signal_hook::SIGUSR1) => ready(Some(Command::Status.into())),
                    Ok(signal_hook::SIGUSR2) => ready(Some(
                        Command::StartPlaylist(String::from("red"), true).into(),
                    )),
                    Ok(signal) => {
                        error!("Received unexpected signal {}.", signal);
                        ready(None)
                    }
                    Err(e) => {
                        error!("Received unknown error: {}", e);
                        ready(None)
                    }
                }));
            }
            Err(e) => {
                error!("Unable to attach signal handler: {}", e);
            }
        }

        Ok(music_box)
    }

    async fn init_and_run(data_dir: &Path) -> VoidResult {
        let music_box = MusicBox::init(data_dir).await?;
        music_box.run().await
    }

    pub fn block(data_dir: &Path) -> VoidResult {
        let mut runtime = Runtime::new().map_err(|e| e.to_string())?;

        runtime.block_on(MusicBox::init_and_run(data_dir))
    }

    pub fn daemonize(data_dir: &Path) -> VoidResult {
        let path = data_dir.to_owned();

        // If forking fails we still run in the parent process. If it succeeds
        // the parent process exits immediately and any other results are being
        // handled in the forked process.
        let result = Daemonize::new()
            .privileged_action(move || {
                // This runs in the forked process.
                let mut runtime = Runtime::new().unwrap();
                info!("Music box initialization.");
                runtime
                    .block_on(MusicBox::init(&path))
                    .log_error(|e| format!("Music box initialization failed: {}", e))
                    .expect("Initialization failed.")
            })
            .start();

        let music_box = match result {
            Ok(music_box) => {
                // In the forked process.
                music_box
            }
            Err(DaemonizeError::Fork) => {
                // Failed to fork at all.
                error!("Failed to launch daemon.");
                return Err(String::from("Failed to launch daemon."));
            }
            Err(e) => {
                // In the forked process but something went wrong.
                error!("Failed during fork: {}", e);
                panic!(e);
            }
        };

        let mut runtime = Runtime::new().unwrap();
        runtime.block_on(music_box.run())
    }
}
