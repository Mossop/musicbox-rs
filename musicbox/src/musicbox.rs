use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::id;
use std::time::Duration;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use daemonize::{Daemonize, DaemonizeError};
use futures::channel::mpsc::{channel, Sender};
use futures::compat::*;
use futures::future::{join_all, poll_fn, ready, TryFutureExt};
use futures::select;
use futures::stream::{Stream, StreamExt};
use log::{error, info, trace};
use serde_json::from_reader;
use signal_hook::iterator::Signals;
use tokio::runtime::Runtime;

use crate::error::{ErrorExt, MusicResult, VoidResult};
use crate::events::{Command, Event, Message, MessageStream};
#[cfg(target_arch = "arm")]
use crate::hardware::gpio::button::Buttons;
use crate::hardware::keyboard::Keyboard;
use crate::hw_config::HwConfig;
use crate::player::Player;
use crate::playlist::StoredPlaylist;
use crate::term_logger::TermLogger;
use crate::track::Track;

const HW_CONFIG_NAME: &str = "hwconfig.json";
const VOLUME_INTERVAL: f64 = 0.1;

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
    volume: f64,
}

impl MusicBox {
    pub fn new(volume: f64) -> MusicResult<MusicBox> {
        let (player, playback_events) = Player::new(volume)?;

        let mut music_box = MusicBox {
            volume,
            player,
            events: Default::default(),
            commands: Default::default(),
            stored_playlists: Default::default(),
            event_listeners: Default::default(),
            playlist: Default::default(),
            play_state: Default::default(),
        };

        music_box.add_event_stream(playback_events);
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

    async fn play(&mut self, position: usize) {
        if let Some(track) = self.playlist.get(position) {
            self.player.start(&track.path()).log().drop();
            self.play_state = Some(PlayState {
                position,
                duration: Duration::from_secs(0),
                paused: false,
            });
        } else {
            self.play_state = None;
            self.player.stop().log().drop();
            self.playlist.clear();
            self.dispatch_event(Event::PlaylistUpdated.into()).await;
        }
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
                let position = match self.play_state {
                    Some(ref state) => {
                        if state.position > 0 && state.duration.as_secs() < 2 {
                            state.position - 1
                        } else {
                            state.position
                        }
                    }
                    None => return,
                };
                self.play(position).await;
            }
            Command::NextTrack => {
                let position = match self.play_state {
                    Some(ref state) => state.position + 1,
                    None => return,
                };
                self.play(position).await;
            }
            Command::PlayPause => {
                if let Some(ref mut state) = self.play_state {
                    if state.paused {
                        trace!("Play");
                        self.player.play().log().drop();
                    } else {
                        trace!("Pause");
                        self.player.pause().log().drop();
                    }
                } else if !self.playlist.is_empty() {
                    self.play(0).await;
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
                self.player.stop().log().drop();
                self.dispatch_event(Event::Shutdown.into()).await;
            }
            Command::StartPlaylist(name, force) => {
                if let Some(playlist) = self.stored_playlists.get(&name) {
                    if playlist.equals(&self.playlist) && !force {
                        return;
                    }

                    self.playlist = playlist.tracks();
                    self.dispatch_event(Event::PlaylistUpdated.into()).await;
                    self.play(0).await;
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
        match &event.payload {
            Event::PlaybackPosition(_) => {}
            payload => info!("Saw event {:?}", payload),
        };

        match event.payload {
            Event::PlaybackPaused => {
                if let Some(ref mut state) = self.play_state {
                    state.paused = true;
                }
            }
            Event::PlaybackUnpaused => {
                if let Some(ref mut state) = self.play_state {
                    state.paused = false;
                }
            }
            Event::PlaybackEnded => {
                if let Some(pos) = self.play_state.as_ref().map(|state| state.position) {
                    self.play(pos + 1).await;
                }
            }
            _ => {}
        }

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
    async fn init(data_dir: &Path, has_console: bool) -> MusicResult<MusicBox> {
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

        let mut music_box: MusicBox = MusicBox::new(0.5)?;
        let playlists = StoredPlaylist::init(data_dir, hw_config.playlists).await?;
        for playlist in playlists {
            music_box.stored_playlists.insert(playlist.name(), playlist);
        }

        #[cfg(target_arch = "arm")]
        Buttons::init(&mut music_box, &hw_config.buttons)?;

        if has_console {
            music_box.add_command_stream(Keyboard::init(hw_config.keyboard));
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
        // This is a non-daemonized run, set up the terminal for interactive use.
        enable_raw_mode().unwrap();
        TermLogger::init().unwrap();

        let result = MusicBox::init(data_dir, true)
            .and_then(|music_box| music_box.run())
            .await;

        disable_raw_mode().unwrap();
        println!();

        result
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
                    .block_on(MusicBox::init(&path, false))
                    .format_log(|e| format!("Music box initialization failed: {}", e))
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
