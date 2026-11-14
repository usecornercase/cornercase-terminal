use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::app::App;
use crate::app::input::WHEEL_ROWS;
use crate::app::opening::Subject;
use crate::app::text_fields::FieldArea;
use crate::app::toast::Toast;
use crate::bots::chat::Kind;
use crate::control::contract::Code;
use crate::editor::Editor;
use crate::ui::{self, ChatHit, RowHit};

const INPUT_ROWS: usize = 6;

impl App {
    fn chat_pane_area(&self, area: Rect) -> Rect {
        self.layout(area).shown(self.screen.nav).pane
    }

    pub(in crate::app) fn chat_key(&mut self, key: KeyEvent, area: Rect) {
        let Some(at) = self.shown_bot() else { return };
        match key.code {
            KeyCode::Enter => self.send_from_chat(),
            KeyCode::Esc if !self.panels.bots.live[at].draft.is_empty() => {
                self.panels.bots.live[at].draft = Editor::default();
            }
            KeyCode::Esc if self.panel_shown() => self.close_panel(),
            KeyCode::Esc => {}
            _ => {
                let width = ui::text_width(self.chat_pane_area(area)).max(1);
                self.panels.bots.live[at].draft.key(key, width, INPUT_ROWS);
            }
        }
    }

    pub(in crate::app) fn chat_paste(&mut self, text: &str, area: Rect) {
        let Some(at) = self.shown_bot() else { return };
        let width = ui::text_width(self.chat_pane_area(area)).max(1);
        let draft = &mut self.panels.bots.live[at].draft;
        draft.insert_lines(text);
        draft.reveal(width, INPUT_ROWS);
    }

    fn chat_toast(&mut self, message: String) {
        self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Bug));
    }

    pub(in crate::app) fn send_from_chat(&mut self) {
        let Some(at) = self.shown_bot() else { return };
        let live = &self.panels.bots.live[at];
        let (id, text) = (live.bot.id.clone(), live.draft.text());
        if text.trim().is_empty() {
            return;
        }
        match self.bot_say(&id, &text, false) {
            Ok(_) => {
                self.panels.bots.live[at].draft = Editor::default();
                if let Some(shown) = self.panels.bots.shown.as_mut() {
                    shown.scroll = 0;
                    shown.news = false;
                }
            }
            Err(refusal) => self.chat_toast(refusal.message),
        }
    }

    fn send_again(&mut self, error: u64) {
        let Some(at) = self.shown_bot() else { return };
        let lines = self.panels.bots.shown.as_ref().map(|shown| shown.lines.as_slice()).unwrap_or_default();
        let before = lines.iter().position(|line| line.id == error).unwrap_or(lines.len());
        let Some(text) = lines[..before].iter().rev().find(|line| line.kind == Kind::You).map(|line| line.text.clone())
        else {
            return;
        };
        let id = self.panels.bots.live[at].bot.id.clone();
        if let Err(refusal) = self.bot_say(&id, &text, false) {
            self.chat_toast(refusal.message);
        }
    }

    pub(in crate::app) fn chat_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        match ev.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                self.scroll_chat(ev.kind == MouseEventKind::ScrollUp);
            }
            MouseEventKind::Down(MouseButton::Left) => self.press_chat(pos, area),
            _ => {}
        }
    }

    fn scroll_chat(&mut self, up: bool) {
        let Some(shown) = self.panels.bots.shown.as_mut() else { return };
        let step = usize::try_from(WHEEL_ROWS).unwrap_or(3);
        shown.scroll = if up { shown.scroll.saturating_add(step) } else { shown.scroll.saturating_sub(step) };
        if shown.scroll == 0 {
            shown.news = false;
        }
    }

    fn press_chat(&mut self, pos: Position, area: Rect) {
        let Some(at) = self.shown_bot() else { return };
        let Some(shown) = self.shown_chat(area) else { return };
        let (pane, pitch) = (self.chat_pane_area(area), self.layout(area).shown(self.screen.nav).pitch);
        let areas = ui::chat_areas(pane, ui::input_rows(&shown.view.input, pane), pitch);
        let Some(hit) = ui::chat_hit(&areas, &shown.view, pos) else { return };
        self.chat_hit(at, hit, (areas.input_text, pos));
    }

    fn chat_hit(&mut self, at: usize, hit: ChatHit, (field, pos): (Rect, Position)) {
        let id = self.panels.bots.live[at].bot.id.clone();
        match hit {
            ChatHit::NewConversation => self.new_conversation(&id),
            ChatHit::Menu => self.open_bot_menu(at, pos),
            ChatHit::Send => self.send_from_chat(),
            ChatHit::Stop => _ = self.bot_stop(&id),
            ChatHit::News => self.scroll_to_end(),
            ChatHit::Row(row) => self.chat_row(at, &row),
            ChatHit::Link(url) => self.copy(&url),
            ChatHit::Input(line, col) => {
                let width = usize::from(field.width.max(1));
                self.panels.bots.live[at].draft.place(width, line, col);
            }
        }
    }

    pub(in crate::app) fn new_conversation(&mut self, id: &str) {
        let Err(refusal) = self.bot_reset(id) else { return };
        let name = self.bot_at(id).map_or_else(|_| id.to_string(), |at| self.panels.bots.live[at].bot.name.clone());
        let busy = refusal.code == Code::Busy;
        self.chat_toast(if busy { format!("{name} is answering: stop it first") } else { refusal.message });
    }

    fn scroll_to_end(&mut self) {
        if let Some(shown) = self.panels.bots.shown.as_mut() {
            shown.scroll = 0;
            shown.news = false;
        }
    }

    fn chat_row(&mut self, at: usize, row: &RowHit) {
        match *row {
            RowHit::Unfold(first) => {
                if let Some(shown) = self.panels.bots.shown.as_mut() {
                    shown.unfolded.insert(first);
                }
            }
            RowHit::Retry(error) => self.send_again(error),
            RowHit::Example(i) => self.panels.bots.live[at].draft = Editor::new(ui::EXAMPLES[i]),
            RowHit::Link => {}
        }
    }
}

impl App {
    pub(in crate::app) fn chat_text(&mut self, area: Rect) -> Option<(FieldArea, &mut Editor)> {
        let at = self.shown_bot()?;
        let areas = self.layout(area).shown(self.screen.nav);
        let draft = self.panels.bots.live[at].draft.clone();
        let geometry = ui::chat_areas(areas.pane, ui::input_rows(&draft, areas.pane), areas.pitch);
        let text = geometry.input_text;
        let editor = &mut self.panels.bots.live[at].draft;
        let top = ui::queue_form::first_line(editor, text);
        Some((FieldArea { text, width: usize::from(text.width.max(1)), top, scrolls: true }, editor))
    }

    pub(in crate::app) fn bot_card_mouse(&mut self, ev: MouseEvent, pos: Position, areas: &ui::Areas) {
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let Some(at) = self.shown_bot() else { return };
        let card = self.bot_card(at);
        if ui::card_folder(areas.workspaces_list, &card).contains(pos) {
            let key = self.panels.bots.live[at].key;
            self.open_menu_or_copy(Subject::Bot(key), pos);
        }
    }
}
