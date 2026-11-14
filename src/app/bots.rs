use std::path::{Path, PathBuf};

use crate::bots::chat::{self, Line};
use crate::bots::{Bot, folder};

pub(super) struct Live {
    pub(super) bot: Bot,
    pub(super) folder: PathBuf,
    pub(super) next_line: u64,
    pub(super) last: Option<Line>,
}

#[derive(Default)]
pub(super) struct Bots {
    pub(super) dir: Option<PathBuf>,
    pub(super) live: Vec<Live>,
}

impl super::App {
    pub(crate) fn set_bots_dir(&mut self, dir: &Path) {
        self.bots.dir = Some(dir.to_path_buf());
        self.bots.live = folder::load_all(dir).into_iter().map(|bot| load_chat(dir, bot)).collect();
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
    let next_line = chat::replay(&lines).next_id;
    let last = lines.iter().rev().find(|line| matches!(line.kind, chat::Kind::You | chat::Kind::Bot)).cloned();
    Live { bot, folder, next_line, last }
}
