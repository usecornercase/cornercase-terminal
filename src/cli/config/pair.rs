use std::io::Write as _;
use std::time::Duration;

use serde_json::Value;

use crate::cli::{print_json, print_out, quietly};
use crate::client::{self, answer, ask};
use crate::control::pair::{ConfigPair, PairEnded, PairKey, PairLine, PairShown, Route, Routes};
use crate::control::{AppRoutes, Command};
use crate::error::{Error, Result};
use crate::os::console;

const SERVER_LEFT: &str = "the server stopped before a phone paired";
const NOT_PAIRED: &str = "no phone was paired";

pub(super) fn run(key: String, routes: bool, json: bool) -> Result<()> {
    if routes && key != "app" {
        return Err(Error::WrongUsage("--routes goes with `config pair app`".into()));
    }
    if routes {
        return listed(json);
    }
    let mut reading = Reading { key: PairKey::of(&key), json, ended: None, reader_left: false };
    client::follow_all("config pair", Command::ConfigPair(ConfigPair { key }), |value| reading.line(&value))?;
    reading.finished(reader_gone)
}

fn listed(json: bool) -> Result<()> {
    let value = ask("app-routes", Command::AppRoutes(AppRoutes {}))?;
    let routes: Routes = answer(value.clone())?;
    if json { print_json(&value) } else { print_out(&routes_text(&routes)) }
}

fn routes_text(routes: &Routes) -> String {
    let route = |Route { host, port }: &Route| {
        if host.contains(':') { format!("[{host}]:{port}\n") } else { format!("{host}:{port}\n") }
    };
    routes.routes.iter().map(route).collect()
}

struct Reading {
    key: Option<PairKey>,
    json: bool,
    ended: Option<PairEnded>,
    reader_left: bool,
}

impl Reading {
    fn line(&mut self, value: &Value) -> Result<bool> {
        let line: PairLine = answer(value.clone())?;
        let text = if self.json { format!("{value}\n") } else { self.text(&line) };
        let mut out = std::io::stdout().lock();
        self.reader_left = quietly(out.write_all(text.as_bytes()).and_then(|()| out.flush()))?;
        if let PairLine::Ended(ended) = line {
            self.ended = Some(ended);
        }
        Ok(self.reader_left || self.ended.is_some())
    }

    fn text(&self, line: &PairLine) -> String {
        match line {
            PairLine::Shown(shown) => shown_text(shown),
            PairLine::Ended(PairEnded { paired: true, chat, .. }) => connected(self.key, chat.as_deref()),
            PairLine::Ended(_) => String::new(),
        }
    }

    fn finished(self, reader_gone: impl FnOnce() -> bool) -> Result<()> {
        match self.ended {
            Some(PairEnded { paired: true, .. }) => Ok(()),
            Some(PairEnded { error, .. }) => Err(Error::Control(error.unwrap_or_else(|| NOT_PAIRED.into()))),
            None if self.reader_left || reader_gone() => Ok(()),
            None => Err(Error::Control(SERVER_LEFT.into())),
        }
    }
}

fn shown_text(shown: &PairShown) -> String {
    let lines = shown.url.iter().chain(&shown.lines);
    lines.map(|line| format!("{line}\n")).chain([String::from("\n")]).collect()
}

fn connected(key: Option<PairKey>, chat: Option<&str>) -> String {
    key.map_or_else(|| String::from("paired\n"), |key| format!("{}\n", key.connected(chat)))
}

fn reader_gone() -> bool {
    client::reader_gone_within(&mut client::watched(console::stdout()), Duration::ZERO)
}

#[cfg(test)]
mod tests;
