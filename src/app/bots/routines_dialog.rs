use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use crate::app::App;
use crate::app::input::{ListPointer, list_pointer};
use crate::app::overlay::Overlay;
use crate::bots::chat::STOPPED;
use crate::bots::routines::{Asked, Button, Dialog, buttons, left_a_block};
use crate::queue::{self, Entry, State};
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Listing {
    Routines,
    Secrets,
}

const HINT_OPEN: &str = "enter shows its last run in the chat";

const HINT_NONE: &str = "each run is a new conversation of {bot}, which answers in its chat";

impl App {
    pub(in crate::app) fn open_bot_routines(&mut self, key: u64, selected: Option<u64>) {
        let first = || {
            let at = self.panels.bots.by_key(key)?;
            self.routine_ids(&self.panels.bots.live[at].bot.id).first().copied()
        };
        let selected = selected.or_else(first);
        self.overlay = Some(Overlay::BotRoutines(Box::new(Dialog::new(key, selected))));
    }

    pub(super) fn routine_ids(&self, bot: &str) -> Vec<u64> {
        self.bot_entries(bot).filter(|entry| !entry.is_run()).map(|entry| entry.id).collect()
    }

    pub(super) fn dialog_bot(&self, dialog: &Dialog) -> Option<(usize, String)> {
        let at = self.panels.bots.by_key(dialog.bot)?;
        Some((at, self.panels.bots.live[at].bot.id.clone()))
    }

    pub(in crate::app) fn bot_routines_view(&self, dialog: &Dialog, area: Rect) -> Option<ui::Overlay> {
        self.routines_list_view(dialog, area).map(|view| ui::Overlay::BotRoutines(Box::new(view)))
    }

    fn routines_list_view(&self, dialog: &Dialog, area: Rect) -> Option<ui::BotRoutines> {
        let (at, id) = self.dialog_bot(dialog)?;
        let name = self.panels.bots.live[at].bot.name.clone();
        let entries: Vec<&Entry> = self.bot_entries(&id).filter(|entry| !entry.is_run()).collect();
        let ids: Vec<u64> = entries.iter().map(|entry| entry.id).collect();
        let compact = area.width < ui::COMPACT_WIDTH;
        let mut fitted = dialog.clone();
        fitted.fit(&ids, ui::routines_room(area, compact).0);
        let (selected, scroll) = (fitted.index(&ids), fitted.scroll);
        let rows: Vec<ui::RoutineRow> = entries.iter().map(|entry| self.routine_row(entry)).collect();
        let hint =
            selected.map_or_else(|| HINT_NONE.replace("{bot}", &name), |at| self.routine_hint(entries[at], &name));
        let example = ui::EXAMPLES[3];
        let empty = format!(
            "{name} has no routines yet. + routine schedules one, or ask {name} in its chat, such as \"{example}\""
        );
        Some(ui::BotRoutines {
            title: format!("Routines › {name}"),
            rows,
            selected,
            scroll,
            hint,
            empty,
            compact,
            new: "+ routine",
        })
    }

    fn routine_row(&self, entry: &Entry) -> ui::RoutineRow {
        let zone = self.panels.queued.display_zone();
        let now = crate::time::now();
        let when = match entry.task.repeat {
            Some(repeat) if entry.task.paused => format!("↻ {repeat} · paused"),
            Some(repeat) => format!("↻ {repeat} · next {}", queue::clock::short(entry.task.at, now, zone)),
            None if entry.state == State::Pending => {
                format!("once · {}", queue::clock::short(entry.task.at, now, zone))
            }
            None => "once".into(),
        };
        let run = self.panels.queued.queue().last_run(entry);
        let (state, colour, at) = run.map_or((None, None, String::new()), |run| run_text(run, zone, now));
        ui::RoutineRow {
            name: entry.task.label.clone(),
            when,
            state,
            colour,
            at,
            paused: entry.task.paused,
            buttons: buttons(entry),
            detail: None,
        }
    }

    fn routine_hint(&self, entry: &Entry, name: &str) -> String {
        let zone = self.panels.queued.display_zone();
        let Some(run) = self.panels.queued.queue().last_run(entry) else { return HINT_NONE.replace("{bot}", name) };
        match &run.state {
            State::Missed => {
                let at = queue::clock::when(run.task.at, crate::time::now(), zone);
                format!("missed its run at {at} · run now runs it once")
            }
            State::Failed { reason } if reason != STOPPED => {
                format!("last run failed: {reason} · enter shows it in the chat")
            }
            state if left_a_block(state) => HINT_OPEN.into(),
            _ => HINT_NONE.replace("{bot}", name),
        }
    }

    fn open_list(&self) -> Option<(Listing, &Dialog)> {
        match &self.overlay {
            Some(Overlay::BotRoutines(dialog)) => Some((Listing::Routines, dialog)),
            Some(Overlay::BotSecrets(secrets)) => secrets.list().map(|list| (Listing::Secrets, &list.dialog)),
            _ => None,
        }
    }

    fn open_list_mut(&mut self) -> Option<&mut Dialog> {
        match &mut self.overlay {
            Some(Overlay::BotRoutines(dialog)) => Some(dialog),
            Some(Overlay::BotSecrets(secrets)) => secrets.list_mut().map(|list| &mut list.dialog),
            _ => None,
        }
    }

    fn list_ids(&self, kind: Listing, dialog: &Dialog) -> Vec<u64> {
        let Some((at, id)) = self.dialog_bot(dialog) else { return Vec::new() };
        match kind {
            Listing::Routines => self.routine_ids(&id),
            Listing::Secrets => {
                let kept = &self.panels.bots.live[at].secrets;
                (0..if kept.is_unreadable() { 1 } else { kept.count() } as u64).collect()
            }
        }
    }

    fn fit_list(&mut self, area: Rect) {
        let rows = ui::routines_room(area, area.width < ui::COMPACT_WIDTH).0;
        let Some((kind, dialog)) = self.open_list() else { return };
        let ids = self.list_ids(kind, dialog);
        if let Some(dialog) = self.open_list_mut() {
            dialog.fit(&ids, rows);
        }
    }

    pub(in crate::app) fn bot_list_key(&mut self, key: KeyEvent, area: Rect) {
        self.fit_list(area);
        let (rows, _) = ui::routines_room(area, area.width < ui::COMPACT_WIDTH);
        let Some((kind, dialog)) = self.open_list() else { return };
        let ids = self.list_ids(kind, dialog);
        let Some(dialog) = self.open_list_mut() else { return };
        let asked = dialog.key(key, &ids, rows);
        self.bot_list_do(kind, asked);
    }

    pub(in crate::app) fn bot_list_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(pointer) = list_pointer(ev.kind) else { return };
        self.fit_list(area);
        let Some((kind, dialog)) = self.open_list() else { return };
        let ids = self.list_ids(kind, dialog);
        let Some(view) = self.list_view(kind, dialog, area) else { return };
        let Some(dialog) = self.open_list_mut() else { return };
        let asked = match pointer {
            ListPointer::Wheel(delta) => {
                let (rows, _) = ui::routines_room(area, view.compact);
                dialog.scroll_by(delta, ids.len(), rows);
                Asked::None
            }
            ListPointer::Press => match ui::routines_hit(area, &view, pos) {
                Some(hit) => dialog.click(hit, &ids),
                None => Asked::None,
            },
        };
        self.bot_list_do(kind, asked);
    }

    fn list_view(&self, kind: Listing, dialog: &Dialog, area: Rect) -> Option<ui::BotRoutines> {
        match kind {
            Listing::Routines => self.routines_list_view(dialog, area),
            Listing::Secrets => self.secrets_list_view(dialog, area),
        }
    }

    fn bot_list_do(&mut self, kind: Listing, asked: Asked) {
        match kind {
            Listing::Routines => self.bot_routines_do(asked),
            Listing::Secrets => self.bot_secrets_do(asked),
        }
    }

    fn bot_routines_do(&mut self, asked: Asked) {
        let Some(Overlay::BotRoutines(dialog)) = &self.overlay else { return };
        let Some((at, id)) = self.dialog_bot(dialog) else { return };
        match asked {
            Asked::None => {}
            Asked::Close => self.overlay = None,
            Asked::Open(entry) => self.open_last_run(at, entry),
            Asked::Press(entry, button) => self.press_routine(entry, button),
            Asked::New => self.new_bot_routine_form(&id),
        }
    }

    fn open_last_run(&mut self, at: usize, id: u64) {
        let run = self
            .panels
            .queued
            .queue()
            .entry(id)
            .and_then(|entry| self.panels.queued.queue().last_run(entry))
            .filter(|run| left_a_block(&run.state))
            .map(|run| run.id);
        if let Some(run) = run {
            self.overlay = None;
            self.show_bot_at_run(at, Some(run));
        }
    }

    pub(super) fn press_routine(&mut self, id: u64, button: Button) {
        match button {
            Button::RunNow => self.run_queued_now(id),
            Button::Pause => self.pause_queued(id, true),
            Button::Resume => self.pause_queued(id, false),
            Button::Edit => self.new_queue_form(Some(id)),
            Button::Remove => self.remove_queued(id),
            Button::Replace | Button::RemoveAll => {}
        }
    }
}

fn run_text(run: &Entry, zone: &str, now: i64) -> (Option<String>, Option<Color>, String) {
    let at = queue::clock::when(run.started.unwrap_or(run.task.at), now, zone);
    match &run.state {
        State::Starting => (Some("answering…".into()), None, String::new()),
        State::Answered { .. } => (Some("answered".into()), Some(Color::Green), at),
        State::Failed { reason } if reason == STOPPED => (Some("stopped".into()), None, at),
        State::Failed { .. } => (Some("failed".into()), Some(Color::Red), at),
        State::Missed => (Some("missed".into()), Some(Color::Yellow), at),
        State::Pending => (None, None, String::new()),
        State::Started { .. } | State::Kept { .. } | State::Removed => (Some("started".into()), None, at),
    }
}
