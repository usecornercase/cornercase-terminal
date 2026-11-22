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

pub(super) fn access_words(access: Access) -> (&'static str, bool) {
    match access {
        Access::Full => ("full access", true),
        Access::Cornercase => ("through cornercase", false),
    }
}

impl App {
    pub(in crate::app) fn bots_view(&self) -> Option<ui::BotsView> {
        if !self.bots_offered() {
            return None;
        }
        let live = &self.panels.bots.live;
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
                asking: !state.asks.is_empty(),
            })
            .collect();
        Some(ui::BotsView { entries, scroll: self.scroll.of(List::Bots), page: self.bots_page() })
    }

    fn bot_detail_short(&self, at: usize) -> String {
        if !self.panels.bots.live[at].asks.is_empty() {
            return String::from("asks for a secret");
        }
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
        let (access, red) = access_words(bot.access);
        rows.push(ui::CardRow { red, ..ui::CardRow::new("access", access) });
        rows.extend(Self::secrets_row(&live.secrets));
        rows.extend(live.asks.iter().map(|ask| ui::CardRow {
            status: Some(crate::agents::activity::Status::Waiting),
            ..ui::CardRow::new("asks", format!("for a secret: {}", ask.joined()))
        }));
        rows.push(ui::CardRow::new("made", crate::queue::clock::day(bot.created, &self.window_zone())));
        rows.extend(self.started_rows(&bot.id));
        rows.extend(self.routine_rows(&bot.id));
        ui::BotCard { name: bot.name.clone(), rows }
    }

    fn started_rows(&self, bot: &str) -> Vec<ui::CardRow> {
        let mut rows: Vec<ui::CardRow> = Vec::new();
        for place in self.agent_places() {
            let term = self.placed(place);
            let Some(status) = term.agent.status().filter(|_| term.bot.as_deref() == Some(bot)) else { continue };
            let kind = term.agent.agent().unwrap_or_default();
            let value = format!("{kind} · {}", self.pane_place(term.id).unwrap_or_default());
            let label = if rows.is_empty() { "started" } else { "" };
            rows.push(ui::CardRow { status: Some(status), pane: Some(term.id), ..ui::CardRow::new(label, value) });
        }
        rows
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
                let row =
                    |label, value: String| ui::CardRow { routine: Some(entry.id), ..ui::CardRow::new(label, value) };
                Some([row("routine", entry.task.label.clone()), row("", format!("{next} · ↻ {repeat}"))])
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
            MouseEventKind::Down(MouseButton::Left) if self.bots_layout(areas).button().contains(pos) => {
                self.open_bot_form(None);
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
        let items = [
            BotItem::Rename,
            BotItem::Edit,
            BotItem::Routines,
            BotItem::Secrets,
            BotItem::NewConversation,
            BotItem::OpenFolder,
            BotItem::Remove,
        ];
        let actions = items.map(|item| MenuAction::Bot(key, item)).to_vec();
        self.overlay = Some(Overlay::Menu { at: pos, actions });
    }
}
