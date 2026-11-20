use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use super::App;
use super::input::{ListPointer, list_pointer};
use super::overlay::Overlay;
use crate::queue::State;
use crate::queue::dialog::{Asked, Button, Dialog, Kind, Row, Shown};
use crate::text::plural;
use crate::ui;

const EMPTY: &str = "nothing queued: new schedules a prompt, and schedule on an issue queues it";
const NOTHING_MATCHES: &str = "nothing matches";
const FOLD_HINT: &str = " · ← → fold its runs";

impl App {
    pub(super) fn open_queue(&mut self, selected: Option<u64>) {
        self.screen.nav = None;
        self.overlay = Some(Overlay::QueueList(Box::new(Dialog::new(selected))));
        self.sync_queued();
    }

    pub(super) fn queue_list_view(dialog: &Dialog, area: Rect) -> ui::Overlay {
        ui::Overlay::QueueList(Box::new(queue_list_shown(dialog, area)))
    }

    pub(super) fn queue_list_key(&mut self, key: KeyEvent, area: Rect) {
        self.sync_queued();
        let (rows, _) = ui::queue_list_room(area);
        let Some(Overlay::QueueList(dialog)) = &mut self.overlay else { return };
        dialog.fit(rows);
        let asked = dialog.key(key, rows);
        self.queue_list_do(asked, area);
    }

    pub(super) fn queue_list_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(pointer) = list_pointer(ev.kind) else { return };
        self.sync_queued();
        let (rows, width) = ui::queue_list_room(area);
        let Some(Overlay::QueueList(dialog)) = &mut self.overlay else { return };
        let shown = queue_list_shown(dialog, area);
        dialog.fit(rows);
        let asked = match pointer {
            ListPointer::Wheel(delta) => {
                dialog.scroll_by(delta, rows);
                Asked::None
            }
            ListPointer::Press => {
                ui::queue_list_hit(area, &shown, pos).map_or(Asked::None, |hit| dialog.click(hit, width))
            }
        };
        self.queue_list_do(asked, area);
    }

    fn queue_list_do(&mut self, asked: Asked, area: Rect) {
        match asked {
            Asked::None => {}
            Asked::Close => self.overlay = None,
            Asked::New => self.new_queue_form(None),
            Asked::Open(id) => self.open_queued(id, area),
            Asked::Run(id) => self.run_queued_now(id),
            Asked::Pause(id) => self.pause_queued(id, true),
            Asked::Resume(id) => self.pause_queued(id, false),
            Asked::Remove(id) => self.remove_queued(id),
            Asked::Clear => self.clear_queued(),
            Asked::Fold(id) => {
                self.panels.queued.toggle_fold(id);
                self.sync_queued();
            }
        }
    }
}

fn queue_list_shown(dialog: &Dialog, area: Rect) -> ui::QueueList {
    let (rows, width) = ui::queue_list_room(area);
    let mut fitted = dialog.clone();
    fitted.fit(rows);
    let shown = fitted.shown();
    let selected = fitted.index();
    let folds = selected.and_then(|at| shown.get(at)).is_some_and(|shown| shown.fold.is_some());
    let hint = queue_hint(fitted.selected());
    ui::QueueList {
        filter: fitted.query().to_string(),
        rows: shown.iter().map(queue_row).collect(),
        selected,
        scroll: fitted.scroll(),
        empty: if fitted.is_empty() { EMPTY } else { NOTHING_MATCHES }.into(),
        tail: bot_routines_tail(fitted.bot_routines),
        hint: if folds { format!("{hint}{FOLD_HINT}") } else { hint.into() },
        buttons: fitted.buttons(width).into_iter().map(Button::label).collect(),
    }
}

fn queue_row(shown: &Shown) -> ui::QueueRow {
    let row = shown.row;
    ui::QueueRow {
        fold: shown.fold,
        run: shown.run,
        when: row.when.clone(),
        title: if shown.run { row.status.clone() } else { row.label.clone() },
        meta: if shown.run { row.origin.clone() } else { row.meta() },
        hidden: (shown.hidden > 0).then(|| format!(" · {}", plural(shown.hidden, "run", "runs"))),
        colour: queued_colour(row),
        dim: row.kind == (Kind::Routine { paused: true }),
    }
}

pub(super) fn queued_colour(row: &Row) -> Option<Color> {
    if row.unsent && matches!(row.state, State::Started { .. }) {
        return Some(Color::Yellow);
    }
    match (row.kind, &row.state) {
        (Kind::Routine { paused: true }, _) | (_, State::Removed) => Some(Color::DarkGray),
        (_, State::Failed { .. }) => Some(Color::Red),
        (_, State::Missed | State::Kept { .. }) => Some(Color::Yellow),
        (_, State::Started { .. } | State::Answered { .. }) => Some(Color::Green),
        (_, State::Pending | State::Starting) => None,
    }
}

fn bot_routines_tail(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 bot routine: in its bot's chat".into()),
        n => Some(format!("{n} bot routines: in each bot's chat")),
    }
}

fn queue_hint(selected: Option<&Row>) -> &'static str {
    let Some(row) = selected else { return "every project's scheduled agents" };
    let opens = matches!(row.state, State::Started { .. } | State::Kept { .. }) && row.live;
    match (row.kind, &row.state) {
        (Kind::Routine { .. }, _) => "enter edits it · start now runs it once more",
        (Kind::Run, _) if opens => "enter opens its pane · a run of a routine",
        (Kind::Run, State::Pending | State::Starting) => "a run of a routine · every project's scheduled agents",
        (Kind::Run, _) => "enter shows its result · a run of a routine",
        (_, State::Started { .. }) if opens => "enter opens its pane · every project's scheduled agents",
        (_, State::Started { .. }) => "enter shows its result · every project's scheduled agents",
        (_, State::Starting) => "starting… · every project's scheduled agents",
        _ => "enter edits it · start now runs it at once",
    }
}
