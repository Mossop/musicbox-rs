use std::path::Path;
use std::thread;
use std::time::Duration;

use futures::Stream;
use glib::error::Error;
use glib::object::{Cast, ObjectExt};
use glib::value::Value;
use gstreamer::message;
use gstreamer::message::MessageView;
use gstreamer::{
    init, Bus, ClockTime, ElementExt, ElementExtManual, ElementFactory, GstBinExt, GstObjectExt,
    Pipeline, State,
};
use gstreamer_audio::{StreamVolume, StreamVolumeExt, StreamVolumeFormat};
use log::{error, info, trace, warn};

use crate::error::{ErrorExt, MusicResult, VoidResult};
use crate::events::{Event, Message, MessageSender, SyncMessageChannel};

const BUS_POLL_TIMEOUT: u64 = 500;

#[derive(Debug, PartialEq)]
enum PlaybackState {
    NotStarted,
    Paused,
    Playing,
    Finished,
}

struct Playback {
    pipeline: Pipeline,
    volume: StreamVolume,
}

pub struct Player {
    playback: Option<Playback>,
    event_sender: MessageSender<Event>,
    volume: f64,
}

impl Player {
    pub fn new(vol: f64) -> MusicResult<(Player, impl Stream<Item = Message<Event>>)> {
        init().prefix("Unable to initialize gstreamer")?;

        let (sender, receiver) = SyncMessageChannel::<Event>::init();

        let player = Player {
            playback: None,
            event_sender: sender,
            volume: vol,
        };

        Ok((player, receiver))
    }

    pub fn start(&mut self, path: &Path) -> VoidResult {
        info!("Starting playback of {}.", path.display());
        if let Some(playback) = self.playback.take() {
            playback
                .pipeline
                .set_state(State::Null)
                .prefix("Unable to cancel existing playback pipeline.")
                .log()
                .drop();
        }

        let pipeline = Pipeline::new(None);
        let playbin =
            ElementFactory::make("playbin", None).prefix("Unable to create playback element")?;
        pipeline
            .add(&playbin)
            .prefix("Unable to add playback element to pipeline")?;

        playbin
            .set_property("uri", &Value::from(&format!("file://{}", path.display())))
            .prefix("Unable to load source file")?;

        let volume = playbin
            .dynamic_cast::<StreamVolume>()
            .map_err(|_| String::from("Unable to get volume controller."))?;
        self.playback = Some(Playback {
            pipeline: pipeline.clone(),
            volume,
        });
        self.set_volume(self.volume);

        PlaybackListener::init(pipeline.clone(), self.event_sender.clone())?;

        pipeline
            .set_state(State::Playing)
            .prefix("Unable to start playback")?;

        Ok(())
    }

    pub fn stop(&mut self) -> VoidResult {
        if let Some(playback) = self.playback.take() {
            playback
                .pipeline
                .set_state(State::Null)
                .prefix("Unable to stop playback")?;
        }
        Ok(())
    }

    pub fn play(&mut self) -> VoidResult {
        if let Some(ref playback) = self.playback {
            playback
                .pipeline
                .set_state(State::Playing)
                .prefix("Unable to unpause playback")?;
        }
        Ok(())
    }

    pub fn pause(&mut self) -> VoidResult {
        if let Some(ref playback) = self.playback {
            playback
                .pipeline
                .set_state(State::Paused)
                .prefix("Unable to pause playback")?;
        }
        Ok(())
    }

    pub fn set_volume(&mut self, volume: f64) {
        self.volume = volume;
        if let Some(ref playback) = self.playback {
            playback
                .volume
                .set_volume(StreamVolumeFormat::Cubic, volume);
        }
    }
}

struct PlaybackListener {
    sender: MessageSender<Event>,
    pipeline: Pipeline,
    bus: Bus,
    state: PlaybackState,
}

impl PlaybackListener {
    pub fn init(pipeline: Pipeline, sender: MessageSender<Event>) -> VoidResult {
        let listener = PlaybackListener {
            sender,
            bus: pipeline
                .get_bus()
                .ok_or_else(|| String::from("Unable to get playback bus."))?,
            state: PlaybackState::NotStarted,
            pipeline,
        };

        thread::spawn(move || listener.listen());

        Ok(())
    }

    fn info(&self, error: Error) -> Option<Message<Event>> {
        info!("Bus reported message: {}", error);
        None
    }

    fn warning(&self, error: Error) -> Option<Message<Event>> {
        warn!("Bus reported warning: {}", error);
        None
    }

    fn error(&self, error: Error) -> Option<Message<Event>> {
        error!("Bus reported error: {}", error);
        None
    }

    fn state_changed(&mut self, sc: message::StateChanged) -> Option<Message<Event>> {
        if let Some(element) = sc.get_src() {
            if let Some(parent) = element.get_parent() {
                if parent != self.pipeline {
                    return None;
                }
            } else {
                return None;
            }
        } else {
            return None;
        }

        match (&self.state, sc.get_current()) {
            // This is part of the transition to playing. Ignore it.
            (PlaybackState::NotStarted, State::Paused) => None,
            (PlaybackState::NotStarted, State::Ready) => None,
            (PlaybackState::NotStarted, State::Playing) => {
                self.state = PlaybackState::Playing;
                Some(Event::PlaybackStarted.into())
            }
            (PlaybackState::Paused, State::Playing) => {
                self.state = PlaybackState::Playing;
                Some(Event::PlaybackUnpaused.into())
            }
            (PlaybackState::Playing, State::Paused) => {
                self.state = PlaybackState::Paused;
                Some(Event::PlaybackPaused.into())
            }
            (_, State::Ready) => {
                self.state = PlaybackState::Finished;
                Some(Event::PlaybackEnded.into())
            }
            _ => {
                trace!(
                    "Unexpected state transition from {:?} to {:?}.",
                    self.state,
                    sc.get_current()
                );
                None
            }
        }
    }

    fn end_of_stream(&mut self, eos: message::Eos) -> Option<Message<Event>> {
        if Some(self.pipeline.clone().upcast()) != eos.get_src() {
            return None;
        }

        self.state = PlaybackState::Finished;
        Some(Event::PlaybackEnded.into())
    }

    pub fn listen(mut self) {
        while self.state != PlaybackState::Finished {
            let to_send = match self
                .bus
                .timed_pop(ClockTime::from_mseconds(BUS_POLL_TIMEOUT))
            {
                Some(message) => match message.view() {
                    MessageView::Info(m) => self.info(m.get_error()),
                    MessageView::Warning(m) => self.warning(m.get_error()),
                    MessageView::Error(m) => self.error(m.get_error()),
                    MessageView::StateChanged(sc) => self.state_changed(sc),
                    MessageView::Eos(eos) => self.end_of_stream(eos),

                    MessageView::DurationChanged(_) => None,
                    MessageView::StreamStart(_) => None,
                    MessageView::StreamStatus(_) => None,
                    MessageView::AsyncDone(_) => None,
                    MessageView::NewClock(_) => None,
                    MessageView::Tag(_) => None,
                    MessageView::Latency(_) => None,
                    _ => {
                        trace!(
                            "Saw bus message {:?} from {:?}.",
                            message.get_type(),
                            message.get_src().map(|o| o.get_name().to_string())
                        );
                        None
                    }
                },
                None => self
                    .pipeline
                    .query_position::<ClockTime>()
                    .and_then(|c| c.nseconds())
                    .map(|n| Event::PlaybackPosition(Duration::from_nanos(n)).into()),
            };

            if let Some(m) = to_send {
                self.sender.send(m);
            }
        }
    }
}
