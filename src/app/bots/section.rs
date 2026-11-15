use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::layout::{Position, Rect};

use crate::agents::Known;
use crate::agents::headless::Access;
use crate::app::App;
use crate::app::interaction::Grab;
use crate::app::layout::List;
use crate::app::menus::MenuAction;
use crate::app::overlay::Overlay;
use crate::ui;

use super::menu::BotItem;

const NO_MODEL: &str = "the agent's own";

impl App {
    pub(in crate::app) fn bots_view(&self) -> Option<ui::BotsView> {
        let live = &self.panels.bots.live;
        if live.is_empty() {
            return None;
        }
        let shown = self.shown_bot();
        let entries = live
            .iter()
            .enumerate()
            .map(|(i, state)| ui::BotEntry {
                name: state.bot.name.clone(),
                detail: self.bot_detail_short(i),
                fallback: self.bot_agent_model(i),
                thinking: state.answering(),
                unread: state.unread,
                active: shown == Some(i),
            })
            .collect();
        Some(ui::BotsView { entries, scroll: self.scroll.of(List::Bots), folded: self.panels.bots.collapsed })
    }

    fn bot_detail_short(&self, at: usize) -> String {
        let idle = !self.panels.bots.live[at].answering();
        let next = self.bot_next_run(&self.panels.bots.live[at].bot.id).filter(|_| idle);
        match next {
            Some(next) => format!("next {}", crate::queue::clock::short(next, crate::time::now(), &self.window_zone())),
            None => self.bot_agent_model(at),
        }
    }

    fn bot_agent_model(&self, at: usize) -> String {
        let bot = &self.panels.bots.live[at].bot;
        match &bot.model {
            Some(model) => format!("{} · {model}", bot.agent),
            None => bot.agent.clone(),
        }
    }

    pub(in crate::app) fn bot_detail(&self, at: usize) -> String {
        let bot = &self.panels.bots.live[at].bot;
        let account = Known::of(&bot.agent).and_then(|kind| self.account_label(kind, bot.account.as_deref()));
        let parts = [Some(bot.agent.clone()), bot.model.clone(), bot.effort.clone(), account];
        parts.into_iter().flatten().collect::<Vec<_>>().join(" · ")
    }

    pub(in crate::app) fn bot_card(&self, at: usize) -> ui::BotCard {
        let live = &self.panels.bots.live[at];
        let bot = &live.bot;
        let title = Known::of(&bot.agent).map_or_else(|| bot.agent.clone(), |kind| kind.title().to_string());
        let folder = crate::text::display_path(&live.folder, self.home.as_deref());
        let mut rows = vec![ui::CardRow { folder: true, ..ui::CardRow::new("folder", folder) }];
        rows.push(ui::CardRow::new("agent", title));
        rows.push(ui::CardRow::new("model", bot.model.clone().unwrap_or_else(|| NO_MODEL.to_string())));
        if let Some(effort) = &bot.effort {
            rows.push(ui::CardRow::new("effort", effort.clone()));
        }
        let account = Known::of(&bot.agent).and_then(|kind| self.account_label(kind, bot.account.as_deref()));
        if let Some(account) = account {
            rows.push(ui::CardRow::new("account", account));
        }
        rows.push(match bot.access {
            Access::Full => ui::CardRow { red: true, ..ui::CardRow::new("access", "full access") },
            Access::Cornercase => ui::CardRow::new("access", "through cornercase"),
        });
        rows.push(ui::CardRow::new("made", crate::queue::clock::day(bot.created, &self.window_zone())));
        rows.extend(self.routine_rows(&bot.id));
        ui::BotCard { name: bot.name.clone(), rows }
    }

    fn routine_rows(&self, bot: &str) -> Vec<ui::CardRow> {
        let (zone, now) = (self.window_zone(), crate::time::now());
        let routines = self.bot_entries(bot).filter(|entry| entry.is_routine());
        routines
            .filter_map(|entry| {
                let repeat = entry.task.repeat?;
                let next = if entry.task.paused {
                    String::from("paused")
                } else {
                    format!("next {}", crate::queue::clock::short(entry.task.at, now, &zone))
                };
                let when = ui::CardRow::new("", format!("{next} · ↻ {repeat}"));
                Some([ui::CardRow::new("routine", entry.task.label.clone()), when])
            })
            .flatten()
            .collect()
    }

    pub(in crate::app) fn bots_layout(&self, areas: &ui::Areas) -> ui::Rows {
        ui::bot_rows(areas.bots_list, areas.pitch, self.panels.bots.live.len(), self.scroll.of(List::Bots))
    }

    pub(in crate::app) fn clamp_bots_scroll(&mut self) {
        self.scroll.set(List::Bots, self.bots_layout(&self.screen.drawn).scrolled(0));
    }

    pub(in crate::app) fn bots_mouse(&mut self, areas: &ui::Areas, pos: Position, kind: MouseEventKind, area: Rect) {
        match kind {
            MouseEventKind::Down(MouseButton::Left) if areas.bots_title.contains(pos) => {
                self.press_bots_title(areas, pos);
            }
            MouseEventKind::Down(MouseButton::Left) => self.press_bot(areas, pos, area),
            MouseEventKind::Down(MouseButton::Right) => {
                if let Some(at) = self.bots_layout(areas).at(pos) {
                    self.open_bot_menu(at, pos);
                }
            }
            _ => {}
        }
    }

    fn press_bots_title(&mut self, areas: &ui::Areas, pos: Position) {
        if ui::bots_new(areas.bots_title).contains(pos) {
            self.open_bot_form(None);
        } else if !areas.compact() && ui::bots_fold(areas.bots_title).contains(pos) {
            self.panels.bots.collapsed = !self.panels.bots.collapsed;
            self.clamp_bots_scroll();
        }
    }

    fn press_bot(&mut self, areas: &ui::Areas, pos: Position, area: Rect) {
        let rows = self.bots_layout(areas);
        let Some(at) = rows.at(pos) else { return };
        let row = rows.item(at);
        if ui::bot_menu(row, areas.pitch).contains(pos) {
            self.open_bot_menu(at, pos);
        } else {
            let key = self.panels.bots.live[at].key;
            self.grab_with(Grab::Bot(key), row, area, false);
        }
    }

    pub(in crate::app) fn open_bot_menu(&mut self, at: usize, pos: Position) {
        let key = self.panels.bots.live[at].key;
        let actions = [BotItem::Rename, BotItem::Edit, BotItem::NewConversation, BotItem::OpenFolder, BotItem::Remove]
            .map(|item| MenuAction::Bot(key, item))
            .to_vec();
        self.overlay = Some(Overlay::Menu { at: pos, actions });
    }
}
