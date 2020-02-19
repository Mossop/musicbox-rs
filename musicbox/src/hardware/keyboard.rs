use crossterm::event::{Event, EventStream, KeyEvent};
use futures::future::ready;
use futures::stream::{Stream, StreamExt};

use serde::Deserialize;

use crate::events::{Command, Message};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyConfig {
    pub key: KeyEvent,
    pub command: Command,
}

pub struct Keyboard;

impl Keyboard {
    pub fn init(keys: Vec<KeyConfig>) -> impl Stream<Item = Message<Command>> {
        EventStream::new().filter_map(move |event| {
            ready(match event {
                Ok(Event::Key(k)) => Keyboard::generate_command(k, &keys),
                _ => None,
            })
        })
    }

    fn generate_command(key: KeyEvent, keys: &[KeyConfig]) -> Option<Message<Command>> {
        keys.iter()
            .find(|config| config.key == key)
            .map(|config| config.command.to_owned().into())
    }
}
