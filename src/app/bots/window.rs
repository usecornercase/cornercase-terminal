use super::Chat;
use crate::app::App;
use crate::app::toast::Toast;
use crate::bots::{chat, folder};
use crate::log;

pub(super) const CHAT_KEPT: usize = 1_000;

impl App {
    pub(in crate::app) fn bot_shown(&self) -> bool {
        self.panels.bots.shown.is_some()
    }

    pub(in crate::app) fn shown_bot(&self) -> Option<usize> {
        let id = &self.panels.bots.shown.as_ref()?.bot;
        self.panels.bots.live.iter().position(|live| &live.bot.id == id)
    }

    pub(in crate::app) fn shown_bot_id(&self) -> Option<String> {
        self.panels.bots.shown.as_ref().map(|shown| shown.bot.clone())
    }

    pub(in crate::app) fn bots_thinking(&self) -> bool {
        self.panels.bots.live.iter().any(|live| live.turn.is_some())
    }

    pub(in crate::app) fn leave_bot(&mut self) {
        if let Some(shown) = self.panels.bots.shown.take() {
            log::info!("ui", "bot hidden", bot = shown.bot);
        }
    }

    pub(in crate::app) fn show_bot(&mut self, at: usize) {
        let live = &self.panels.bots.live[at];
        let (id, path) = (live.bot.id.clone(), live.folder.join(folder::CHAT_FILE));
        self.leave_remote();
        let mut lines = chat::read(&path);
        let skip = lines.len().saturating_sub(CHAT_KEPT);
        lines.drain(..skip);
        log::info!("ui", "bot shown", bot = id, lines = lines.len());
        self.panels.bots.shown = Some(Chat::new(id.clone(), lines));
        let _ = self.bot_seen(&id);
        self.screen.nav = None;
        self.panels.files.set_full_screen(false);
    }

    pub(in crate::app) fn show_bot_by_key(&mut self, key: u64) {
        if let Some(at) = self.panels.bots.by_key(key) {
            self.show_bot(at);
        }
    }

    pub(in crate::app) fn go_to_bot(&mut self, id: &str) {
        match self.bot_at(id) {
            Ok(at) => self.show_bot(at),
            Err(_) => {
                self.messages.toast = Some(Toast::new("that bot is gone", crate::ui::ToastIcon::Check));
            }
        }
        log::info!("activity", "notification clicked", to = "bot");
    }

    pub(in crate::app) fn bot_on_screen(&self, at: usize) -> bool {
        self.shown_bot() == Some(at) && self.has_window() && self.screen.nav.is_none() && !self.file_full_screen()
    }

    pub(in crate::app) fn chat_typing(&self) -> bool {
        self.shown_bot().is_some()
            && self.screen.nav.is_none()
            && !self.file_full_screen()
            && !(self.tour_typing() || self.todo_typing() || self.files_typing() || self.filtering())
    }
}
