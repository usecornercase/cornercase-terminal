use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::app::App;
use crate::app::overlay::Overlay;
use crate::app::toast::Toast;
use crate::editor::Editor;
use crate::log;
use crate::shortcuts::Action;
use crate::ui;

const NO_BOT_SHOWN: &str = "no bot's chat is on screen";

impl App {
    pub(in crate::app) fn bots_offered(&self) -> bool {
        self.panels.bots.dir.is_some() || !self.panels.bots.live.is_empty()
    }

    pub(in crate::app) fn bots_page(&self) -> bool {
        self.panels.bots.page
    }

    fn last_bot(&self) -> Option<usize> {
        let last = self.panels.bots.last.as_deref().and_then(|id| self.panels.bots.by_id(id));
        last.or_else(|| (!self.panels.bots.live.is_empty()).then_some(0))
    }

    pub(in crate::app) fn open_bots(&mut self, compact: bool) {
        match self.last_bot().filter(|_| !compact) {
            Some(at) => self.show_bot(at),
            None => self.open_lobby(),
        }
        if compact {
            self.screen.nav = (!self.panels.bots.live.is_empty()).then_some(ui::Nav::Bots);
        }
    }

    pub(in crate::app) fn bot_shortcut(&mut self, action: Action, area: Rect) -> Option<Action> {
        match action {
            Action::BotsPage => self.open_bots(self.layout(area).shown(self.screen.nav).compact()),
            Action::NextBot => self.step_bot(1, area),
            Action::PreviousBot => self.step_bot(-1, area),
            Action::BotMemory => self.memory_by_key(),
            other => return Some(other),
        }
        None
    }

    fn step_bot(&mut self, delta: isize, area: Rect) {
        let count = self.panels.bots.live.len();
        let next = match self.shown_bot() {
            Some(at) => crate::app::shortcuts::cycle(at, count, delta),
            None => self.last_bot(),
        };
        match next {
            Some(at) => self.show_bot(at),
            None => self.open_bots(self.layout(area).shown(self.screen.nav).compact()),
        }
    }

    fn memory_by_key(&mut self) {
        match self.shown_bot() {
            Some(at) => self.open_bot_memory(self.panels.bots.live[at].key),
            None => self.messages.toast = Some(Toast::new(NO_BOT_SHOWN, ui::ToastIcon::Bug)),
        }
    }

    fn open_lobby(&mut self) {
        self.leave_remote();
        self.panels.bots.page = true;
        self.panels.files.set_full_screen(false);
        log::info!("ui", "bots page shown");
    }

    pub(in crate::app) fn press_back(&mut self, compact: bool) {
        if !compact || self.screen.nav == Some(ui::Nav::Bots) {
            self.leave_bot();
        }
        if compact {
            self.screen.nav = Some(ui::Nav::Projects);
        }
    }

    pub(in crate::app) fn lobby(&self) -> Option<ui::Lobby> {
        let empty = self.panels.bots.live.is_empty();
        (self.bots_page() && !self.bot_shown()).then_some(if empty { ui::Lobby::Empty } else { ui::Lobby::Pick })
    }

    pub(in crate::app) fn lobby_mouse(&mut self, ev: MouseEvent, pos: Position, pane: Rect) {
        let Some(lobby) = self.lobby().filter(|_| ev.kind == MouseEventKind::Down(MouseButton::Left)) else { return };
        match ui::lobby_hit(lobby, pane, pos) {
            Some(ui::LobbyHit::New) => self.open_bot_form(None),
            Some(ui::LobbyHit::Idea(at)) => self.open_bot_idea(at),
            None => {}
        }
    }

    fn open_bot_idea(&mut self, at: usize) {
        let Some((name, about)) = ui::IDEAS.get(at) else { return };
        self.open_bot_form(None);
        if let Some(Overlay::BotForm(form)) = &mut self.overlay {
            form.name = Editor::new(name);
            form.about = Editor::new(about);
        }
    }
}
