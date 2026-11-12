use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;

use super::App;
use super::flow_panel::COPY;
use super::input::wheel;
use super::overlay::Overlay;
use super::pull_requests::DONE;
use super::queue_results::Reader;
use crate::error::Result;
use crate::queue::conversation::Talk;
use crate::ui;

mod lines;

const CONVERSATION: &str = "conversation";
const RESULT: &str = "result";
const RESULT_TITLE: &str = "Result";
const TALK_TITLE: &str = "Conversation";
const READING: &str = "reading the conversation…";

#[derive(Debug)]
pub(super) struct Detail {
    entry: u64,
    scroll: usize,
    showing: Showing,
}

#[derive(Debug)]
enum Showing {
    Result,
    Reading,
    Talk { talk: Talk, wrapped: (usize, Vec<Line<'static>>) },
    Failed(String),
}

impl App {
    pub(super) fn open_queue_detail(&mut self, id: u64) {
        self.stash_browser();
        self.overlay = Some(Overlay::QueueEntry(Box::new(Detail { entry: id, scroll: 0, showing: Showing::Result })));
    }

    fn queue_detail_lines(&self, detail: &Detail, area: Rect) -> Option<Vec<Line<'static>>> {
        let zone = self.panels.queued.display_zone();
        let at = (zone, usize::from(ui::update_notes(area).width));
        Some(match &detail.showing {
            Showing::Result => {
                let shown = self.queue_shown(detail.entry).ok()?;
                let entry = self.panels.queued.queue().entry(detail.entry)?;
                lines::result(&shown, entry, at, self.muted_color())
            }
            Showing::Talk { wrapped, .. } if wrapped.0 == at.1 => wrapped.1.clone(),
            Showing::Talk { talk, .. } => lines::talk(talk, at, self.muted_color()),
            Showing::Reading | Showing::Failed(_) => Vec::new(),
        })
    }

    fn queue_detail_extra(&self, detail: &Detail) -> Option<&'static str> {
        match &detail.showing {
            Showing::Result => {
                let shown = self.queue_shown(detail.entry).ok()?;
                shown.result.is_some_and(|result| result.conversation).then_some(CONVERSATION)
            }
            _ => Some(RESULT),
        }
    }

    pub(super) fn queue_detail_view(&self, detail: &Detail, area: Rect) -> Option<ui::Overlay> {
        let entry = self.panels.queued.queue().entry(detail.entry)?;
        let meta = self.queued_row(entry, self.panels.queued.display_zone()).meta;
        let (title, note) = match &detail.showing {
            Showing::Result => (RESULT_TITLE, None),
            Showing::Reading => (TALK_TITLE, Some(ui::Note::Busy(READING))),
            Showing::Talk { .. } => (TALK_TITLE, None),
            Showing::Failed(error) => {
                (TALK_TITLE, Some(ui::Note::Error(format!("cannot read the conversation: {error}"))))
            }
        };
        Some(ui::Overlay::Update(ui::Update {
            title,
            message: format!("{}\n{meta}", entry.task.label),
            notes: self.queue_detail_lines(detail, area)?,
            scroll: detail.scroll,
            note,
            submit: COPY,
            extra: self.queue_detail_extra(detail),
            cancel: DONE,
        }))
    }

    pub(super) fn queue_detail_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        if let Some(delta) = wheel(ev.kind) {
            self.scroll_queue_detail(delta, area);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let Some(Overlay::QueueEntry(detail)) = &self.overlay else { return };
        let [submit, cancel] = ui::update_buttons(area, COPY, DONE);
        let extra = self.queue_detail_extra(detail).map(|label| (label, ui::update_extra(area, COPY, DONE, label)));
        if submit.contains(pos) {
            self.copy_queue_detail();
        } else if let Some((label, _)) = extra.filter(|(_, rect)| rect.contains(pos)) {
            self.queue_detail_other(label);
        } else if cancel.contains(pos) {
            self.close_queue_detail();
        }
    }

    fn queue_detail_other(&mut self, label: &str) {
        if label == CONVERSATION {
            self.start_conversation();
        } else if let Some(Overlay::QueueEntry(detail)) = &mut self.overlay {
            detail.showing = Showing::Result;
            detail.scroll = 0;
        }
    }

    pub(super) fn scroll_queue_detail(&mut self, delta: isize, area: Rect) {
        let Some(Overlay::QueueEntry(detail)) = &self.overlay else { return };
        let count = self.queue_detail_lines(detail, area).map_or(0, |lines| lines.len());
        let scroll = ui::update_scroll(area, count, detail.scroll.saturating_add_signed(delta));
        if let Some(Overlay::QueueEntry(detail)) = &mut self.overlay {
            detail.scroll = scroll;
        }
    }

    fn start_conversation(&mut self) {
        let Some(Overlay::QueueEntry(detail)) = &self.overlay else { return };
        let entry = detail.entry;
        let pointer = self.readable_record(entry);
        let reading = pointer.is_some();
        if let Some(Overlay::QueueEntry(detail)) = &mut self.overlay {
            detail.scroll = 0;
            detail.showing =
                if reading { Showing::Reading } else { Showing::Failed("the agent no longer keeps its record".into()) };
        }
        if let Some(pointer) = pointer {
            self.read_conversation(entry, pointer, Reader::Window);
        }
    }

    pub(super) fn queue_conversation_shown(&mut self, entry: u64, result: Result<Talk>, area: Rect) {
        let Some(Overlay::QueueEntry(detail)) = &self.overlay else { return };
        if detail.entry != entry || !matches!(detail.showing, Showing::Reading) {
            return;
        }
        let showing = match result {
            Ok(talk) => {
                let at = (self.panels.queued.display_zone(), usize::from(ui::update_notes(area).width));
                let wrapped = (at.1, lines::talk(&talk, at, self.muted_color()));
                Showing::Talk { talk, wrapped }
            }
            Err(error) => Showing::Failed(error.to_string()),
        };
        if let Some(Overlay::QueueEntry(detail)) = &mut self.overlay {
            detail.showing = showing;
            detail.scroll = 0;
        }
    }

    fn queue_detail_text(&self, detail: &Detail) -> Option<String> {
        match &detail.showing {
            Showing::Result => self.queue_shown(detail.entry).ok()?.result.map(|result| result.text),
            Showing::Talk { talk, .. } => Some(lines::plain(talk, self.panels.queued.display_zone())),
            Showing::Reading | Showing::Failed(_) => None,
        }
    }

    fn copy_queue_detail(&mut self) {
        let Some(Overlay::QueueEntry(detail)) = &self.overlay else { return };
        if let Some(text) = self.queue_detail_text(detail).filter(|text| !text.is_empty()) {
            self.copy(&text);
        }
    }

    pub(super) fn submit_queue_detail(&mut self, detail: Box<Detail>) -> Option<Overlay> {
        self.overlay = Some(Overlay::QueueEntry(detail));
        self.copy_queue_detail();
        self.overlay.take()
    }

    pub(super) fn close_queue_detail(&mut self) {
        self.overlay = self.panels.queued.take_browser().map(Overlay::Issues);
        self.sync_queued();
    }

    pub(super) fn close_gone_queue_detail(&mut self) {
        if let Some(Overlay::QueueEntry(detail)) = &self.overlay
            && self.panels.queued.queue().entry(detail.entry).is_none()
        {
            self.close_queue_detail();
        }
    }
}
