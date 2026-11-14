use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::agents::headless::{Action, Ran};
use crate::agents::probe::Running;
use crate::bots::chat::{self, Line};
use crate::bots::{Bot, TURN_TIMEOUT, folder};
use crate::error::Result;

mod manage;
mod turns;

pub(super) struct Turn {
    pub(super) id: u64,
    pub(super) asked: Vec<u64>,
    pub(super) running: Running,
    pub(super) stopping: bool,
    pub(super) dropped: Vec<u64>,
}

pub(super) struct Live {
    pub(super) bot: Bot,
    pub(super) folder: PathBuf,
    pub(super) next_line: u64,
    pub(super) last: Option<Line>,
    pub(super) turn: Option<Turn>,
    pub(super) held: Vec<(u64, String)>,
    pub(super) unread: usize,
    pub(super) waited: HashSet<u64>,
    pub(super) answers: VecDeque<(u64, Line)>,
}

pub(super) struct Bots {
    pub(super) dir: Option<PathBuf>,
    pub(super) live: Vec<Live>,
    pub(super) timeout: Duration,
    pub(super) next_turn: u64,
}

impl Default for Bots {
    fn default() -> Self {
        Self { dir: None, live: Vec::new(), timeout: TURN_TIMEOUT, next_turn: 1 }
    }
}

#[derive(Debug)]
pub(crate) enum BotEvent {
    Step { bot: String, turn: u64, step: Action },
    Ended { bot: String, turn: u64, result: Result<Ran>, took: Duration },
}

impl BotEvent {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Step { .. } => "bot step",
            Self::Ended { .. } => "bot turn",
        }
    }
}

impl Live {
    pub(super) fn new(bot: Bot, folder: PathBuf) -> Self {
        Self {
            bot,
            folder,
            next_line: 1,
            last: None,
            turn: None,
            held: Vec::new(),
            unread: 0,
            waited: HashSet::new(),
            answers: VecDeque::new(),
        }
    }
}

pub(super) enum News {
    Line(Line),
    State(&'static str),
    Change(&'static str),
}

impl super::App {
    pub(super) fn publish_bot(&mut self, bot: &str, news: News) {
        let (mut line, mut state, mut change) = (None, None, None);
        match news {
            News::Line(found) => line = Some(found),
            News::State(text) => state = Some(text.to_string()),
            News::Change(text) => change = Some(text.to_string()),
        }
        let what = crate::control::What::Bot { bot: bot.to_string(), line, state, change };
        let time = crate::time::Stamp(std::time::SystemTime::now()).to_string();
        self.clients.events.publish(&[crate::control::Event { time, what, ids: crate::control::Ids::default() }]);
    }

    pub(super) fn publish_bot_change(&mut self, at: usize, change: &'static str) {
        let id = self.panels.bots.live[at].bot.id.clone();
        self.publish_bot(&id, News::Change(change));
    }

    pub(super) fn publish_bot_removed(&mut self, id: &str) {
        self.publish_bot(id, News::Change("removed"));
    }

    pub(crate) fn set_bots_dir(&mut self, dir: &Path) {
        self.panels.bots.dir = Some(dir.to_path_buf());
        self.panels.bots.live = folder::load_all(dir).into_iter().map(|bot| load_chat(dir, bot)).collect();
    }
}

fn load_chat(dir: &Path, bot: Bot) -> Live {
    let folder = folder::of(dir, &bot.id);
    let path = folder.join(folder::CHAT_FILE);
    let _ = chat::heal(&path);
    let mut lines = chat::read(&path);
    let state = chat::replay(&lines);
    for notice in chat::restart_notices(state, crate::time::now()) {
        if chat::append(&path, &notice).is_ok() {
            lines.push(notice);
        }
    }
    let last = lines.iter().rev().find(|line| matches!(line.kind, chat::Kind::You | chat::Kind::Bot)).cloned();
    Live { next_line: chat::replay(&lines).next_id, last, ..Live::new(bot, folder) }
}
