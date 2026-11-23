use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::app::App;
use crate::app::input::wheel;
use crate::app::overlay::Overlay;
use crate::app::text_fields::FieldArea;
use crate::app::toast::Toast;
use crate::bots::folder;
use crate::bots::memory::{Dialog, Focus, Step};
use crate::control::bots::EditBot;
use crate::control::contract::{Code, Refusal};
use crate::editor::Editor;
use crate::ui::{self, bot_memory};

enum Saved {
    Done,
    Changed(String),
    Refused(String),
}

fn room(area: Rect) -> (usize, usize) {
    let text = bot_memory::text(area);
    (usize::from(text.width.max(1)), usize::from(text.height.max(1)))
}

fn intro(name: &str) -> String {
    format!(
        "What {name} keeps from one conversation to the next, in memory.md. A new conversation reads its first \
         8 KB, and the bot keeps it up to date itself."
    )
}

impl App {
    pub(in crate::app) fn bot_memory(&self, key: &str) -> Result<(String, String), Refusal> {
        let at = self.bot_at(key)?;
        let live = &self.panels.bots.live[at];
        let memory = folder::memory(&live.folder)
            .map_err(|error| Refusal::new(Code::Failed, format!("cannot read {}'s memory.md: {error}", live.bot.id)))?;
        Ok((live.bot.id.clone(), memory))
    }

    pub(in crate::app) fn open_bot_memory(&mut self, key: u64) {
        let Some(at) = self.panels.bots.by_key(key) else { return };
        let id = self.panels.bots.live[at].bot.id.clone();
        match self.bot_memory(&id) {
            Ok((_, read)) => {
                let name = self.panels.bots.live[at].bot.name.clone();
                self.screen.nav = None;
                self.overlay = Some(Overlay::BotMemory(Box::new(Dialog::new(key, &name, read))));
            }
            Err(refusal) => self.messages.toast = Some(Toast::new(refusal.message, ui::ToastIcon::Bug)),
        }
    }

    fn memory_dialog(&mut self) -> Option<&mut Dialog> {
        match &mut self.overlay {
            Some(Overlay::BotMemory(dialog)) => Some(dialog),
            _ => None,
        }
    }

    pub(in crate::app) fn bot_memory_key(&mut self, key: KeyEvent, area: Rect) {
        let (width, rows) = room(area);
        let Some(dialog) = self.memory_dialog() else { return };
        match dialog.key(key, width, rows) {
            Step::Save => self.save_bot_memory(),
            Step::Cancel => self.overlay = None,
            Step::None => {}
        }
    }

    pub(in crate::app) fn bot_memory_paste(&mut self, text: &str, area: Rect) {
        let (width, rows) = room(area);
        if let Some(dialog) = self.memory_dialog() {
            dialog.paste(text, width, rows);
        }
    }

    pub(in crate::app) fn bot_memory_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let (width, rows) = room(area);
        let Some(dialog) = self.memory_dialog() else { return };
        if let Some(delta) = wheel(ev.kind) {
            dialog.text.scroll_by(delta, width, rows);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        match bot_memory::hit(area, pos) {
            Some(bot_memory::Hit::Cancel) => self.overlay = None,
            Some(bot_memory::Hit::Save) => self.save_bot_memory(),
            Some(bot_memory::Hit::Text(line, col)) => dialog.place(width, (line, col)),
            None => {}
        }
    }

    fn save_bot_memory(&mut self) {
        let Some(dialog) = self.memory_dialog() else { return };
        let (key, text, read, changed) = (dialog.bot, dialog.text.text(), dialog.read.clone(), dialog.changed());
        let Some(at) = self.panels.bots.by_key(key) else {
            self.overlay = None;
            return;
        };
        let id = self.panels.bots.live[at].bot.id.clone();
        let saved = match self.bot_memory(&id) {
            Ok(_) if !changed => Saved::Done,
            Ok((_, now)) if now != read => Saved::Changed(now),
            Ok(_) => match self.bot_edit(&EditBot { bot: id, memory: Some(text), ..EditBot::default() }) {
                Ok(_) => Saved::Done,
                Err(refusal) => Saved::Refused(refusal.message),
            },
            Err(refusal) => Saved::Refused(refusal.message),
        };
        if matches!(saved, Saved::Done) {
            self.overlay = None;
            return;
        }
        let Some(dialog) = self.memory_dialog() else { return };
        match saved {
            Saved::Changed(now) => dialog.changed_meanwhile(now),
            Saved::Refused(why) => dialog.error = Some(why),
            Saved::Done => {}
        }
    }

    pub(in crate::app) fn bot_memory_view(dialog: &Dialog) -> ui::Overlay {
        ui::Overlay::BotMemory(Box::new(bot_memory::BotMemory {
            title: dialog.title(),
            intro: intro(&dialog.name),
            text: dialog.text.clone(),
            text_focused: dialog.focus == Focus::Text,
            note: dialog.error.clone().map(ui::Note::Error),
            hint: dialog.hint().to_string(),
        }))
    }

    pub(in crate::app) fn bot_memory_text(&mut self, area: Rect) -> Option<(FieldArea, &mut Editor)> {
        let text = bot_memory::text(area);
        let dialog = self.memory_dialog()?;
        let top = dialog.text.scroll();
        Some((FieldArea { text, width: usize::from(text.width.max(1)), top, scrolls: true }, &mut dialog.text))
    }
}
