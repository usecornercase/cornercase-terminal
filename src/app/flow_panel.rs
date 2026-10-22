use std::time::{Duration, Instant, SystemTime};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use serde_json::Value;

use super::event::AppEvent;
use super::input::wheel;
use super::layout::Side;
use super::overlay::Overlay;
use super::pull_requests::DONE;
use super::{App, Toast};
use crate::control::{Command, FlowMessage};
use crate::error::Result;
use crate::files::link::Target;
use crate::flows::folder::{self, Listing};
use crate::flows::progress::Edge;
use crate::flows::run::{Kind, Message, Run};
use crate::log::{Job, Level};
use crate::project::search::Goto;
use crate::project::{Hierarchy, Project};
use crate::pull_request::PullRequest;
use crate::time;
use crate::todo::editor::wrap;
use crate::ui::{self, flow};

pub(super) const COPY: &str = "copy";
pub(super) const FRAME_EVERY: Duration = Duration::from_millis(40);
const LIST_EVERY: Duration = Duration::from_secs(2);
const LIST_GIVE_UP: Duration = Duration::from_secs(30);
const TITLE: &str = "Message";
const MAX_TEXT: usize = 32 * 1024;
const MAX_READS: usize = 16;
const NO_RUN: &str = "this workspace runs no flow";
const NOTHING_SENT: &str = "(enter, with nothing typed)";

#[derive(Default)]
pub(super) struct FlowPanel {
    scroll: u16,
    reads: Vec<(u64, Noted)>,
    files: Option<(u64, Listing)>,
    listing: Option<(u64, Instant)>,
    listed: Option<Instant>,
}

pub(super) struct Noted {
    run: u64,
    panes: (u64, u64),
    message: Message,
    answered: bool,
}

fn kept(text: &str) -> String {
    let cut = (0..=MAX_TEXT.min(text.len())).rev().find(|&at| text.is_char_boundary(at)).unwrap_or_default();
    text[..cut].to_string()
}

pub(super) type Sent<'a> = (Edge, Option<&'a str>);

fn heading(message: &Message) -> String {
    let what = match message.kind {
        Kind::Keys => "keys pressed",
        Kind::Read => "read",
        Kind::Wait => "waited for",
        Kind::Send => "sent",
    };
    format!("{} → {} · {what}", message.from, message.to)
}

fn lines(message: &Message, width: usize, muted: Color) -> Vec<Line<'static>> {
    let mut out = Vec::new();
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
    pub(super) fn seat(&self, pane: u64) -> Option<(&Run, &str)> {
        self.runs.seat(pane)
    }

    pub(super) fn run_of(&self, workspace: u64) -> Option<&Run> {
        self.runs.of_workspace(workspace)
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
        self.screen.nav = None;
        self.panels.flow.scroll = 0;
        self.show_side(Some(Side::Flow));
    }

    pub(super) fn toggle_flow(&mut self) {
        if self.screen.side == Some(Side::Flow) {
            self.show_side(None);
        } else {
            self.show_flow();
        }
    }

    pub(super) fn show_flow_of(&mut self, workspace: u64) {
        let Some(at) = self.projects.workspace_index(workspace) else { return };
        self.goto(Goto::Place { project: self.projects[at.project()].id, workspace: Some(workspace), tab: None });
        self.show_flow();
    }

    pub(super) fn flow_by_key(&mut self) {
        if self.has_run() {
            self.toggle_flow();
        } else {
            self.messages.toast = Some(Toast::new(NO_RUN, ui::ToastIcon::Check));
        }
    }

    fn role_state(&self, run: &Run, role: &str) -> flow::State {
        let Some(pane) = run.pane_of(role) else { return flow::State::Agent(None) };
        self.projects.pane_by(pane).map_or(flow::State::Closed, |term| flow::State::Agent(term.agent.status()))
    }

    pub(super) fn flow_view(&self, now: Instant, spin: Option<u8>) -> Option<flow::View> {
        let run = self.shown_run()?;
        let (muted, light) = (ui::muted(self.themes.host()), self.themes.light());
        let plan = flow::Diagram::plan(&run.flow, muted, light);
        let pull = run.pull(self.pull_request(run.workspace));
        let end = flow::Ending { reached: run.reached(pull), pull: pull.map(PullRequest::mark) };
        let mut diagram = plan.live(&run.progress, |role| self.role_state(run, role), end, now);
        diagram.spin = spin;
        let files = self.panels.flow.files.as_ref().filter(|(id, _)| *id == run.id).map(|(_, listing)| {
            let now = SystemTime::now();
            let age = |file: &folder::File| {
                let secs = file.modified.and_then(|at| now.duration_since(at).ok()).map_or(0, |d| d.as_secs());
                time::age(i64::try_from(secs).unwrap_or(i64::MAX))
            };
            let rows = listing.files.iter().map(|file| flow::FileRow { name: file.name.clone(), age: age(file) });
            flow::Files { rows: rows.collect(), more: listing.more }
        });
        let files = files.or_else(|| run.dir.as_ref().map(|_| flow::Files { rows: Vec::new(), more: false }));
        Some(flow::View {
            name: run.flow.name.clone(),
            task: run.task.clone(),
            diagram,
            files,
            scroll: self.panels.flow.scroll,
        })
    }

    pub(super) fn refresh_run_files(&mut self, now: Instant) {
        if self.screen.side != Some(Side::Flow) || !self.panel_shown() {
            return;
        }
        let Some((id, dir)) = self.shown_run().and_then(|run| Some((run.id, run.dir.clone()?))) else { return };
        if let Some((_, since)) = self.panels.flow.listing
            && now.duration_since(since) < LIST_GIVE_UP
        {
            return;
        }
        if self.panels.flow.listed.is_some_and(|at| now.duration_since(at) < LIST_EVERY) {
            return;
        }
        self.panels.flow.listing = Some((id, now));
        self.panels.flow.listed = Some(now);
        let job = Job::new(Level::Debug, "flow", "list files").with("run", id);
        self.spawn_job(
            job,
            move || Ok(folder::list(&dir)?),
            move |result, job| {
                if let Ok(listing) = &result {
                    job.note("files", listing.files.len());
                }
                Some(AppEvent::RunFiles(id, result))
            },
        );
    }

    pub(super) fn run_files_listed(&mut self, run: u64, result: Result<Listing>) {
        if self.panels.flow.listing.is_some_and(|(id, _)| id == run) {
            self.panels.flow.listing = None;
        }
        if let Ok(listing) = result
            && self.runs.has(run)
        {
            self.panels.flow.files = Some((run, listing));
        }
    }

    fn open_run_file(&mut self, index: usize) {
        let path =
            self.panels.flow.files.as_ref().and_then(|(_, listing)| listing.files.get(index)).map(|f| f.path.clone());
        if let Some(path) = path.as_deref().and_then(std::path::Path::to_str) {
            self.open_link(&Target { path: path.to_string(), real: path.into(), lines: None });
        }
    }

    pub(super) fn flow_frame(&self, now: Instant) -> Option<Duration> {
        let shown = self.screen.side == Some(Side::Flow) && self.panel_shown();
        let moving = self.shown_run().is_some_and(|run| run.progress.travelling(now).is_some());
        (shown && moving).then_some(FRAME_EVERY)
    }

    pub(super) fn flow_mouse(&mut self, ev: MouseEvent, pos: Position, panel: Rect) {
        let Some(view) = self.flow_view(Instant::now(), None) else { return };
        if let Some(delta) = wheel(ev.kind) {
            self.panels.flow.scroll = flow::scrolled(panel, &view, delta);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        match flow::hit(panel, &view, pos) {
            Some(flow::Hit::Close) => self.show_side(None),
            Some(flow::Hit::Role(role)) => self.jump_to_role(&role),
            Some(flow::Hit::File(index)) => self.open_run_file(index),
            Some(flow::Hit::Pull) => {
                if let Some(workspace) = self.shown_run().map(|run| run.workspace) {
                    self.open_pull_request(workspace);
                }
            }
            Some(flow::Hit::Edge(edge, to)) => {
                let Some(workspace) = self.shown_run().map(|run| run.workspace) else { return };
                self.overlay = Some(Overlay::FlowMessage { workspace, edge, to, scroll: 0 });
            }
            None => {}
        }
    }

    fn jump_to_role(&mut self, role: &str) {
        let Some(pane) = self.shown_run().and_then(|run| run.pane_of(role)) else { return };
        if self.projects.pane_by(pane).is_none() {
            return;
        }
        if self.screen.drawn.compact() {
            self.show_side(None);
        }
        self.jump_to_pane(pane);
    }

    fn flow_message(&self, workspace: u64, edge: Edge, to: Option<&str>) -> Option<&Message> {
        self.run_of(workspace)?.progress.along(edge, to)
    }

    fn flow_message_lines(&self, workspace: u64, sent: Sent<'_>, area: Rect) -> Option<Vec<Line<'static>>> {
        let width = usize::from(ui::update_notes(area).width.max(1));
        self.flow_message(workspace, sent.0, sent.1).map(|message| lines(message, width, ui::muted(self.themes.host())))
    }

    pub(super) fn flow_message_view(
        &self,
        workspace: u64,
        sent: Sent<'_>,
        scroll: usize,
        area: Rect,
    ) -> Option<ui::Overlay> {
        let message = self.flow_message(workspace, sent.0, sent.1)?;
        Some(ui::Overlay::Update(ui::Update {
            title: TITLE,
            message: heading(message),
            notes: self.flow_message_lines(workspace, sent, area)?,
            scroll,
            note: None,
            submit: COPY,
            extra: None,
            cancel: DONE,
        }))
    }

    pub(super) fn flow_message_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(Overlay::FlowMessage { workspace, edge, to, .. }) = &self.overlay else { return };
        let (workspace, edge, to) = (*workspace, *edge, to.clone());
        if let Some(delta) = wheel(ev.kind) {
            self.scroll_flow_message(delta, area);
            return;
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        let [submit, cancel] = ui::update_buttons(area, COPY, DONE);
        if submit.contains(pos) {
            self.overlay = self.submit_flow_message(workspace, edge, to.as_deref());
        } else if cancel.contains(pos) {
            self.overlay = None;
        }
    }

    pub(super) fn scroll_flow_message(&mut self, delta: isize, area: Rect) {
        let Some(Overlay::FlowMessage { workspace, edge, to, scroll }) = &self.overlay else { return };
        let count = self.flow_message_lines(*workspace, (*edge, to.as_deref()), area).map_or(0, |lines| lines.len());
        let scroll = ui::update_scroll(area, count, scroll.saturating_add_signed(delta));
        if let Some(Overlay::FlowMessage { scroll: kept, .. }) = &mut self.overlay {
            *kept = scroll;
        }
    }

    pub(super) fn submit_flow_message(&mut self, workspace: u64, edge: Edge, to: Option<&str>) -> Option<Overlay> {
        let text = self.flow_message(workspace, edge, to).and_then(|message| message.text.clone());
        if let Some(text) = text {
            self.copy(&text);
        }
        None
    }

    pub(super) fn close_gone_flow_message(&mut self) {
        if let Some(Overlay::FlowMessage { workspace, edge, to, .. }) = &self.overlay
            && self.flow_message(*workspace, *edge, to.as_deref()).is_none()
        {
            self.overlay = None;
        }
    }

    pub(super) fn flow_steps(&self, caller: Option<u64>, command: &Command, now: Instant) -> Vec<Noted> {
        let Some((caller, run, from)) = caller.and_then(|c| self.runs.seat(c).map(|(run, from)| (c, run, from))) else {
            return Vec::new();
        };
        let Some(step) = FlowStep::of(command) else { return Vec::new() };
        let mut panes: Vec<u64> = Vec::new();
        for (pane, tab) in &step.targets {
            if let Ok(to) = self.target(Some(caller), *pane, *tab)
                && to != caller
                && !panes.contains(&to)
            {
                panes.push(to);
            }
        }
        panes
            .into_iter()
            .filter_map(|pane| Some((pane, run.role_of(pane)?)))
            .map(|(pane, to)| Noted {
                run: run.id,
                panes: (caller, pane),
                message: step.message(from, to, now),
                answered: step.answered,
            })
            .collect()
    }

    pub(super) fn flow_requested(&mut self, client: u64, noted: Vec<Noted>, accepted: bool) {
        if !accepted {
            return;
        }
        for noted in noted {
            if noted.answered {
                self.hold_flow_read(client, noted);
            } else {
                self.record_flow(noted);
            }
        }
    }

    fn hold_flow_read(&mut self, client: u64, noted: Noted) {
        if self.panels.flow.reads.len() == MAX_READS {
            self.panels.flow.reads.remove(0);
        }
        self.panels.flow.reads.push((client, noted));
    }

    pub(super) fn flow_answered(&mut self, client: u64, reply: &std::result::Result<Value, String>) {
        let Some(i) = self.panels.flow.reads.iter().position(|(c, _)| *c == client) else { return };
        let (_, mut noted) = self.panels.flow.reads.remove(i);
        let Ok(value) = reply else { return };
        let text = value.get("text").and_then(Value::as_str).unwrap_or_default();
        noted.message.bytes = text.len();
        noted.message.text = Some(kept(text));
        self.record_flow(noted);
    }

    pub(super) fn forget_flow_reads(&mut self, client: Option<u64>) {
        let runs = &self.runs;
        self.panels.flow.reads.retain(|(c, noted)| Some(*c) != client && runs.has(noted.run));
    }

    fn record_flow(&mut self, noted: Noted) {
        let Noted { run, panes: (from, to), message, .. } = noted;
        let same = |r: &Run| r.role_of(from) == Some(&message.from) && r.role_of(to) == Some(&message.to);
        let Some(run) = self.runs.run_mut(run).filter(|r| same(r)) else { return };
        let message = run.record(message).clone();
        let wire = FlowMessage::of(run, &message);
        self.publish_flow_message(&wire);
    }
}

struct FlowStep {
    kind: Kind,
    targets: Vec<(Option<u64>, Option<u64>)>,
    text: Option<String>,
    answered: bool,
}

impl FlowStep {
    fn of(command: &Command) -> Option<Self> {
        let one = |kind, target, text, answered| Self { kind, targets: vec![target], text, answered };
        Some(match command {
            Command::Send(send) => one(Kind::Send, (send.pane, send.tab), send.text.clone(), false),
            Command::Keys(keys) => one(Kind::Keys, (keys.pane, keys.tab), Some(keys.keys.join(" ")), false),
            Command::Read(read) => one(Kind::Read, (read.pane, read.tab), None, false),
            Command::LastMessage(last) => one(Kind::Read, (last.pane, last.tab), None, true),
            Command::Wait(wait) => one(Kind::Wait, (wait.pane, wait.tab), None, false),
            Command::WaitSeveral(wait) => {
                let panes = wait.panes.iter().map(|&pane| (Some(pane), None));
                let targets = panes.chain(wait.tabs.iter().map(|&tab| (None, Some(tab)))).collect();
                Self { kind: Kind::Wait, targets, text: None, answered: false }
            }
            _ => return None,
        })
    }

    fn message(&self, from: &str, to: &str, now: Instant) -> Message {
        Message {
            from: from.to_string(),
            to: to.to_string(),
            kind: self.kind,
            text: self.text.as_deref().map(kept),
            bytes: self.text.as_ref().map_or(0, String::len),
            at: now,
            time: SystemTime::now(),
            seq: 0,
            edge: None,
        }
    }
}
