use std::sync::Arc;
use std::thread;

use serde::Deserialize;

use super::link::{self, Connect, Envelope, Heard};
use super::{Group, StreamEvent, Tell, orders};
use crate::control::contract::Refusal;
use crate::control::{Command, Events};
use crate::log;

const MOST: usize = 1 << 20;

enum Copy {
    Copied(u64, String),
    Refused(Refusal),
    Other,
}

#[derive(Deserialize)]
struct Clipboard {
    event: String,
    pane: u64,
    text: String,
}

pub(super) fn follow(connector: Arc<dyn Connect>, group: Arc<Group>, boot: String, tell: Tell) {
    thread::spawn(move || {
        let Ok(mut line) = connector.connect(&group.killer) else {
            return log::warning!("machines", "the copies there cannot be followed");
        };
        group.add(&line.ends);
        let envelope = Envelope { view: None, boot: Some(boot) };
        if !link::send(&mut line, &envelope, Command::Events(Events { host: true, ..Events::default() })) {
            return log::warning!("machines", "the copies there cannot be followed");
        }
        let ended = loop {
            match link::listen(&mut line.reader) {
                Heard::Answer(text) => heard(&text, &tell),
                Heard::Ended(said) => break said.unwrap_or("the connection ended"),
            }
        };
        log::info!("machines", "the copies there ended", reason = ended);
    });
}

fn heard(text: &str, tell: &Tell) {
    match clipboard(text) {
        Copy::Copied(pane, text) => tell(StreamEvent::Copied { pane, text }),
        Copy::Refused(refusal) => {
            let refused = format!("{:?}, {} bytes", refusal.code, refusal.message.len());
            log::warning!("machines", "the copies there were refused", refused = refused);
        }
        Copy::Other => {}
    }
}

fn clipboard(line: &str) -> Copy {
    let event = match link::replied(line) {
        Some(Err(refusal)) => return Copy::Refused(refusal),
        Some(Ok(value)) => serde_json::from_value::<Clipboard>(value).ok(),
        None => None,
    };
    let Some(event) = event.filter(|event| event.event == "clipboard") else { return Copy::Other };
    let text = orders::selected(&event.text, MOST);
    if text.is_empty() { Copy::Other } else { Copy::Copied(event.pane, text) }
}
