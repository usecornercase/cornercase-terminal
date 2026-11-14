use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::agents::headless::{Action, Ran};
use crate::agents::probe::Running;
use crate::bots::chat::{self, Line};
use crate::bots::{Bot, TURN_TIMEOUT, folder};
use crate::editor::Editor;
use crate::error::Result;

mod form;
mod manage;
mod menu;
mod section;
mod shown;
mod talk;
mod turns;
mod window;

pub(super) use menu::BotItem;
pub(super) use shown::Chat;

pub(super) struct Turn {
    pub(super) id: u64,
    pub(super) asked: Vec<u64>,
    pub(super) running: Running,
    pub(super) stopping: bool,
    pub(super) dropped: Vec<u64>,
    pub(super) started: Instant,
    pub(super) steps: usize,
}

pub(super) struct Live {
    pub(super) key: u64,
    pub(super) draft: Editor,
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
    pub(super) next_key: u64,
    pub(super) shown: Option<Chat>,
    pub(super) collapsed: bool,
}

impl Default for Bots {
    fn default() -> Self {
        Self {
            dir: None,
            live: Vec::new(),
            timeout: TURN_TIMEOUT,
            next_turn: 1,
            next_key: 1,
            shown: None,
            collapsed: false,
        }
    }
}

impl Bots {
    pub(super) fn take_key(&mut self) -> u64 {
        let key = self.next_key;
        self.next_key += 1;
        key
    }

    pub(super) fn by_key(&self, key: u64) -> Option<usize> {
        self.live.iter().position(|live| live.key == key)
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
    pub(super) fn new(bot: Bot, folder: PathBuf, key: u64) -> Self {
        Self {
            key,
            draft: Editor::default(),
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
        let bots = folder::load_all(dir);
        let live = bots.into_iter().map(|bot| load_chat(dir, bot, self.panels.bots.take_key())).collect();
        self.panels.bots.live = live;
        self.panels.bots.shown = None;
    }
}

fn load_chat(dir: &Path, bot: Bot, key: u64) -> Live {
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
    Live { next_line: chat::replay(&lines).next_id, last, ..Live::new(bot, folder, key) }
}
