use std::time::{Duration, Instant};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use serde_json::Value;

use super::input::wheel;
use super::layout::Side;
use super::overlay::Overlay;
use super::pull_requests::DONE;
use super::{App, Toast};
use crate::control::Command;
use crate::flows::progress::Edge;
use crate::flows::run::{Kind, Message, Run};
use crate::project::search::Goto;
use crate::project::{Hierarchy, Project};
use crate::todo::editor::wrap;
use crate::ui::{self, flow};

pub(super) const COPY: &str = "copy";
pub(super) const FRAME_EVERY: Duration = Duration::from_millis(40);
const TITLE: &str = "Message";
const MAX_TEXT: usize = 32 * 1024;
const MAX_READS: usize = 16;
const NO_RUN: &str = "this workspace runs no flow";
const NOTHING_SENT: &str = "(enter, with nothing typed)";

#[derive(Default)]
pub(super) struct FlowPanel {
    scroll: u16,
    reads: Vec<(u64, Noted)>,
}

pub(super) struct Noted {
    workspace: u64,
    message: Message,
    answered: bool,
}

fn kept(text: &str) -> String {
    match text.char_indices().nth(MAX_TEXT) {
        Some((cut, _)) => text[..cut].to_string(),
        None => text.to_string(),
    }
}

fn lines(message: &Message, width: usize, muted: Color) -> Vec<Line<'static>> {
    let what = match message.kind {
        Kind::Keys => "keys pressed",
        Kind::Read => "read",
        Kind::Wait => "waited for",
        Kind::Send | Kind::Start => "sent",
    };
    let mut out = vec![Line::from(Span::styled(what, Style::default().fg(muted))), Line::default()];
    let Some(text) = message.text.as_deref() else {
        out.push(Line::from(Span::styled(NOTHING_SENT, Style::default().fg(muted))));
        return out;
    };
    for line in text.lines() {
        let chars: Vec<char> = line.chars().collect();
        out.extend(wrap(line, width).into_iter().map(|range| Line::from(chars[range].iter().collect::<String>())));
    }
    if message.bytes > text.len() {
        let cut = format!("… only the first {} KiB are kept", MAX_TEXT / 1024);
        out.push(Line::from(Span::styled(cut, Style::default().fg(muted))));
    }
    out
}

impl App {
    pub(super) fn run_of(&self, workspace: u64) -> Option<&Run> {
        self.runs.iter().find(|run| run.workspace == workspace)
    }

    fn shown_run(&self) -> Option<&Run> {
        self.run_of(self.project().and_then(Project::workspace)?.id)
    }

    pub(super) fn has_run(&self) -> bool {
        self.shown_run().is_some()
    }

    pub(super) fn show_flow(&mut self) {
        if self.shown_run().is_none() {
            return;
        }
        self.nav = None;
        self.flow.scroll = 0;
        self.show_side(Some(Side::Flow));
    }

    pub(super) fn toggle_flow(&mut self) {
        if self.side == Some(Side::Flow) {
            self.show_side(None);
        } else {
            self.show_flow();
        }
    }

    pub(super) fn show_flow_of(&mut self, workspace: u64) {
        let Some((p, _)) = self.projects.workspace_index(workspace) else { return };
        self.goto(Goto::Place { project: self.projects[p].id, workspace: Some(workspace), tab: None });
        self.show_flow();
    }

    pub(super) fn flow_by_key(&mut self) {
        if self.has_run() {
            self.toggle_flow();
        } else {
            self.toast = Some(Toast::new(NO_RUN, ui::ToastIcon::Check));
        }
    }

    fn role_state(&self, run: &Run, role: &str) -> flow::State {
        let Some(pane) = run.pane_of(role) else { return flow::State::Agent(None) };
        self.projects.pane_by(pane).map_or(flow::State::Closed, |term| flow::State::Agent(term.agent.status()))
    }

    pub(super) fn flow_view(&self, now: Instant, spin: Option<u8>) -> Option<flow::View> {
        let run = self.shown_run()?;
        let (muted, light) = (ui::muted(&self.theme), self.theme.is_light() == Some(true));
        let plan = flow::Diagram::plan(&run.flow, muted, light);
        let mut diagram = plan.live(&run.progress, |role| self.role_state(run, role), now);
        diagram.spin = spin;
        Some(flow::View { name: run.flow.name.clone(), task: run.task.clone(), diagram, scroll: self.flow.scroll })
    }

    pub(super) fn flow_frame(&self, now: Instant) -> Option<Duration> {
        let shown = self.side == Some(Side::Flow) && self.panel_shown();
        let moving = self.shown_run().is_some_and(|run| run.progress.travelling(now).is_some());
        (shown && moving).then_some(FRAME_EVERY)
    }

    pub(super) fn flow_mouse(&mut self, ev: MouseEvent, pos: Position, panel: Rect) {
        let Some(view) = self.flow_view(Instant::now(), None) else { return };
        if let Some(delta) = wheel(ev.kind) {
            self.flow.scroll = flow::scrolled(panel, &view, delta);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        match flow::hit(panel, &view, pos) {
            Some(flow::Hit::Close) => self.show_side(None),
            Some(flow::Hit::Role(role)) => {
                let Some(pane) = self.shown_run().and_then(|run| run.pane_of(&role)) else { return };
                if self.projects.pane_by(pane).is_some() {
                    if self.drawn.compact() {
                        self.show_side(None);
                    }
                    self.jump_to_pane(pane);
                }
            }
            Some(flow::Hit::Edge(edge)) => {
                let Some(workspace) = self.shown_run().map(|run| run.workspace) else { return };
                self.overlay = Some(Overlay::FlowMessage { workspace, edge, scroll: 0 });
            }
            None => {}
        }
    }

    fn flow_message(&self, workspace: u64, edge: Edge) -> Option<&Message> {
        self.run_of(workspace)?.progress.along(edge)
    }

    fn flow_message_lines(&self, workspace: u64, edge: Edge, area: Rect) -> Option<Vec<Line<'static>>> {
        let width = usize::from(ui::update_notes(area).width.max(1));
        self.flow_message(workspace, edge).map(|message| lines(message, width, ui::muted(&self.theme)))
    }

    pub(super) fn flow_message_view(
        &self,
        workspace: u64,
        edge: Edge,
        scroll: usize,
        area: Rect,
    ) -> Option<ui::Overlay> {
        let message = self.flow_message(workspace, edge)?;
        Some(ui::Overlay::Update(ui::Update {
            title: TITLE,
            message: format!("{} → {}", message.from, message.to),
            notes: self.flow_message_lines(workspace, edge, area)?,
            scroll,
            note: None,
            submit: COPY,
            extra: None,
            cancel: DONE,
        }))
    }

    pub(super) fn flow_message_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(Overlay::FlowMessage { workspace, edge, .. }) = self.overlay else { return };
        if let Some(delta) = wheel(ev.kind) {
            self.scroll_flow_message(delta, area);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let [submit, cancel] = ui::update_buttons(area, COPY, DONE);
        if submit.contains(pos) {
            self.overlay = self.submit_flow_message(workspace, edge);
        } else if cancel.contains(pos) {
            self.overlay = None;
        }
    }

    pub(super) fn scroll_flow_message(&mut self, delta: isize, area: Rect) {
        let Some(Overlay::FlowMessage { workspace, edge, scroll }) = self.overlay else { return };
        let count = self.flow_message_lines(workspace, edge, area).map_or(0, |lines| lines.len());
        let scroll = ui::update_scroll(area, count, scroll.saturating_add_signed(delta));
        self.overlay = Some(Overlay::FlowMessage { workspace, edge, scroll });
    }

    pub(super) fn submit_flow_message(&mut self, workspace: u64, edge: Edge) -> Option<Overlay> {
        let text = self.flow_message(workspace, edge).and_then(|message| message.text.clone());
        if let Some(text) = text {
            self.copy(&text);
        }
        None
    }

    pub(super) fn close_gone_flow_message(&mut self) {
        if let Some(Overlay::FlowMessage { workspace, edge, .. }) = self.overlay
            && self.flow_message(workspace, edge).is_none()
        {
            self.overlay = None;
        }
    }

    pub(super) fn flow_steps(&self, caller: Option<u64>, command: &Command, now: Instant) -> Vec<Noted> {
        let Some((run, from)) = caller.and_then(|c| self.runs.iter().find_map(|run| Some((run, run.role_of(c)?))))
        else {
            return Vec::new();
        };
        let (kind, targets, text, answered) = match command {
            Command::Send(send) => (Kind::Send, vec![(send.pane, send.tab)], send.text.clone(), false),
            Command::Keys(keys) => (Kind::Keys, vec![(keys.pane, keys.tab)], Some(keys.keys.join(" ")), false),
            Command::Read(read) => (Kind::Read, vec![(read.pane, read.tab)], None, false),
            Command::LastMessage(last) => (Kind::Read, vec![(last.pane, last.tab)], None, true),
            Command::Wait(wait) => (Kind::Wait, vec![(wait.pane, wait.tab)], None, false),
            Command::WaitSeveral(wait) => {
                let panes = wait.panes.iter().map(|&pane| (Some(pane), None));
                (Kind::Wait, panes.chain(wait.tabs.iter().map(|&tab| (None, Some(tab)))).collect(), None, false)
            }
            _ => return Vec::new(),
        };
        let mut panes: Vec<u64> = Vec::new();
        for (pane, tab) in targets {
            if let Ok(to) = self.target(caller, pane, tab)
                && Some(to) != caller
                && !panes.contains(&to)
            {
                panes.push(to);
            }
        }
        panes
            .into_iter()
            .filter_map(|to| run.role_of(to))
            .map(|to| Noted {
                workspace: run.workspace,
                message: Message {
                    from: from.to_string(),
                    to: to.to_string(),
                    kind,
                    text: text.as_deref().map(kept),
                    bytes: text.as_ref().map_or(0, String::len),
                    at: now,
                },
                answered,
            })
            .collect()
    }

    pub(super) fn flow_requested(&mut self, client: u64, noted: Vec<Noted>, accepted: bool) {
        if !accepted {
            return;
        }
        for noted in noted {
            if noted.answered {
                if self.flow.reads.len() == MAX_READS {
                    self.flow.reads.remove(0);
                }
                self.flow.reads.push((client, noted));
            } else {
                self.record_flow(noted.workspace, noted.message);
            }
        }
    }

    pub(super) fn flow_answered(&mut self, client: u64, reply: &Result<Value, String>) {
        let Some(i) = self.flow.reads.iter().position(|(c, _)| *c == client) else { return };
        let (_, Noted { workspace, mut message, .. }) = self.flow.reads.remove(i);
        let Ok(value) = reply else { return };
        let text = value.get("text").and_then(Value::as_str).unwrap_or_default();
        message.bytes = text.len();
        message.text = Some(kept(text));
        self.record_flow(workspace, message);
    }

    fn record_flow(&mut self, workspace: u64, message: Message) {
        if let Some(run) = self.runs.iter_mut().find(|run| run.workspace == workspace) {
            run.record(message);
        }
    }
}
