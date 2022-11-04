use std::path::Path;
use std::process::id;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use daemonize::{Daemonize, DaemonizeError};
use futures::compat::*;
use futures::future::{ready, TryFutureExt};
use futures::select;
use futures::stream::{Stream, StreamExt};
use log::{error, info, trace};
use signal_hook::iterator::Signals;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;

use crate::appstate::MutableAppState;
use crate::error::{ErrorExt, MusicResult, VoidResult};
use crate::events::{Command, Event, Message, MessageReceiver, MessageSender};
#[cfg(feature = "rpi")]
use crate::hardware::gpio::button::Buttons;
use crate::hardware::keyboard::Keyboard;
use crate::hw_config::HwConfig;
use crate::player::Player;
use crate::playlist::StoredPlaylist;
use crate::server::{serve, ClientInfo};
use crate::term_logger::TermLogger;

const VOLUME_INTERVAL: f64 = 0.1;

pub struct MusicBox {
    server: Option<TcpListener>,
    events: MessageReceiver<Event>,
    commands: MessageReceiver<Command>,
    event_listeners: MessageSender<Event>,
    player: Player,
    state: MutableAppState,
}

impl MusicBox {
    pub fn add_command_stream<S: Send>(&mut self, stream: S)
    where
        S: Stream<Item = Message<Command>> + 'static,
    {
        tokio::spawn(
            stream
                .map(|message| Ok(message))
                .forward(self.commands.sender()),
        );
    }

    async fn play(&mut self, position: usize) {
        if let Some(track) = self.state.playlist().get(position) {
            self.player.start(&track.path()).log().drop();
            self.state.set_playback_position(Some(position))
        } else {
            self.state.set_playback_position(None);
            self.player.stop().log().drop();
            self.state.set_playlist(Default::default());
            self.dispatch_event(Event::PlaylistUpdated.into());
        }
    }

    fn dispatch_event(&mut self, event: Message<Event>) {
        self.event_listeners.send(event);
    }

    async fn handle_command(&mut self, command: Message<Command>) {
        info!("Saw command {:?}", command.payload);

        match command.payload {
            Command::PreviousTrack => {
                let position = match (
                    self.state.playback_position(),
                    self.state.playback_duration(),
                ) {
                    (Some(position), Some(duration)) => {
                        if position > 0 && duration.as_secs() < 2 {
                            position - 1
                        } else {
                            position
                        }
                    }
                    _ => return,
                };
                self.play(position).await;
            }
            Command::NextTrack => {
                let position = match self.state.playback_position() {
                    Some(position) => position + 1,
                    None => return,
                };
                self.play(position).await;
            }
            Command::PlayPause => {
                if let Some(paused) = self.state.paused() {
                    if paused {
                        trace!("Play");
                        self.player.play().log().drop();
                    } else {
                        trace!("Pause");
                        self.player.pause().log().drop();
                    }
                } else {
                    self.play(0).await;
                }
            }
            Command::VolumeUp => {
                let mut volume = self.state.volume() + VOLUME_INTERVAL;
                if volume > 1.0 {
                    volume = 1.0;
                }
                self.state.set_volume(volume);
                self.player.set_volume(volume);
            }
            Command::VolumeDown => {
                let mut volume = self.state.volume() - VOLUME_INTERVAL;
                if volume < 0.0 {
                    volume = 0.0;
                }
                self.state.set_volume(volume);
                self.player.set_volume(volume);
            }
            Command::Shutdown => {
                info!("Music box clean shutdown.");
                self.player.stop().log().drop();
                self.dispatch_event(Event::Shutdown.into());
            }
            Command::StartPlaylist { name, force: _ } => {
                if self.state.is_playing_playlist(&name) {
                    return;
                }

                if let Some(playlist) = self.state.stored_playlist(&name) {
                    self.state.set_playlist(playlist.tracks());
                    self.dispatch_event(Event::PlaylistUpdated.into());
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
            Event::PlaybackPosition { duration: _ } => {}
            payload => info!("Saw event {:?}", payload),
        };

        match event.payload {
            Event::PlaybackPaused => {
                self.state.set_paused(true);
            }
            Event::PlaybackUnpaused => {
                self.state.set_paused(false);
            }
            Event::PlaybackEnded => {
                if let Some(pos) = self.state.playback_position() {
                    self.play(pos + 1).await;
                }
            }
            _ => {}
        }

        self.dispatch_event(event);
    }

    pub fn get_event_stream(&mut self) -> MessageReceiver<Event> {
        self.event_listeners.receiver()
    }

    async fn run(mut self) -> VoidResult {
        info!("Music box startup. Running as process {}.", id());

        if let Some(listener) = self.server.take() {
            serve(
                listener,
                ClientInfo {
                    app_state: self.state.as_immutable(),
                    event_receiver: self.event_listeners.receiver(),
                    command_sender: self.commands.sender(),
                },
            );
        }

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
        let hw_config = HwConfig::load()?;

        let app_state =
            MutableAppState::new(StoredPlaylist::init(data_dir, hw_config.playlists).await?);

        let events = MessageReceiver::new();

        let mut music_box = MusicBox {
            server: Some(
                TcpListener::bind(hw_config.server)
                    .await
                    .prefix("Unable to bind to server socket")?,
            ),
            player: Player::new(events.sender(), 0.5)?,
            events,
            commands: Default::default(),
            event_listeners: MessageSender::new(),
            state: app_state,
        };

        #[cfg(feature = "rpi")]
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
                music_box.add_command_stream(signals.compat().filter_map(|r| {
                    match r {
                        Ok(signal_hook::SIGHUP) => ready(Some(Command::Reload.into())),
                        Ok(signal_hook::SIGTERM) => ready(Some(Command::Shutdown.into())),
                        Ok(signal_hook::SIGINT) => ready(Some(Command::Shutdown.into())),
                        Ok(signal_hook::SIGQUIT) => ready(Some(Command::Shutdown.into())),
                        Ok(signal_hook::SIGUSR1) => ready(Some(Command::Status.into())),
                        Ok(signal_hook::SIGUSR2) => ready(Some(
                            Command::StartPlaylist {
                                name: String::from("red"),
                                force: true,
                            }
                            .into(),
                        )),
                        Ok(signal) => {
                            error!("Received unexpected signal {}.", signal);
                            ready(None)
                        }
                        Err(e) => {
                            error!("Received unknown error: {}", e);
                            ready(None)
                        }
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
