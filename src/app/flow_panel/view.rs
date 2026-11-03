use std::time::{Instant, SystemTime};

use ratatui::style::Color;

use super::{HEAD, NOTHING_SENT};

const LOOP: &str = "loop";
use crate::app::App;
use crate::flows::folder::{self, Listing};
use crate::flows::progress::Edge;
use crate::flows::run::{Kind, Message, Run};
use crate::flows::{ORCHESTRATOR, Role};
use crate::project::{Hierarchy, WorkspaceAt};
use crate::pull_request::PullRequest;
use crate::time;
use crate::ui::{self, flow};

fn age(since: Option<SystemTime>, now: SystemTime) -> String {
    let secs = since.and_then(|at| now.duration_since(at).ok()).map_or(0, |d| d.as_secs());
    time::age(i64::try_from(secs).unwrap_or(i64::MAX))
}

fn head_of(message: &Message) -> Option<String> {
    match (&message.text, message.kind) {
        (Some(text), _) => {
            let cut = (0..=HEAD.min(text.len())).rev().find(|&at| text.is_char_boundary(at)).unwrap_or_default();
            Some(if cut < text.len() { format!("{}…", &text[..cut]) } else { text.clone() })
        }
        (None, Kind::Send) => Some(NOTHING_SENT.to_string()),
        (None, _) => None,
    }
}

fn step_of(run: &Run, edge: Edge) -> flow::Step {
    match edge {
        Edge::Forward(stage) => flow::Step::Forward(stage),
        Edge::Loop(stage) => {
            let back = run.flow.stages.get(stage).and_then(|s| s.back.as_ref());
            let when = back.map(|b| b.when.trim()).filter(|when| !when.is_empty());
            flow::Step::Loop(when.unwrap_or(LOOP).to_string())
        }
        Edge::Outside { .. } => flow::Step::Outside,
    }
}

fn colour_of(run: &Run, role: &str) -> Color {
    let index = run.flow.roles.iter().position(|r| r.name == role).map_or(0, |i| i + 1);
    flow::role_colour(index)
}

impl App {
    pub(super) fn run_diagram(&self, run: &Run, now: Instant, spin: Option<u8>) -> flow::Diagram {
        let (muted, light) = (ui::muted(self.themes.host()), self.themes.light());
        let plan = flow::Diagram::plan(&run.flow, muted, light);
        let pull = run.pull(self.pull_request(run.workspace));
        let end = flow::Ending { reached: run.reached(pull), pull: pull.map(PullRequest::mark) };
        let mut diagram = plan.live(&run.progress, |role| self.role_state(run, role), end, now);
        diagram.spin = spin;
        diagram
    }

    pub(super) fn run_heading(&self, run: &Run, at: WorkspaceAt) -> flow::Heading {
        let workspace = self.projects.workspace(at);
        let read = self.panels.flow.branch.as_ref().filter(|(id, _)| *id == workspace.id).and_then(|(_, b)| b.clone());
        let label = workspace.name.clone().or_else(|| read.clone()).unwrap_or_else(|| "default".into());
        let place = format!("{} › {label}", self.project_label(&self.projects[at.project()]));
        let place = match read.filter(|branch| *branch != label) {
            Some(branch) => format!("{place} · {branch}"),
            None => place,
        };
        let lead = run.pane_of(ORCHESTRATOR).is_some_and(|pane| self.projects.pane_by(pane).is_some());
        flow::Heading { place, run: run.id, messages: run.recorded, active: run.progress.active, lead }
    }

    pub(super) fn run_team(&self, run: &Run, at: WorkspaceAt) -> Vec<flow::Member> {
        let workspace = self.projects.workspace(at);
        let roles = std::iter::once(&run.flow.orchestrator).chain(&run.flow.roles);
        let member = |(index, role): (usize, &Role)| flow::Member {
            role: role.name.clone(),
            agent: role.summary(),
            colour: flow::role_colour(index),
            state: self.role_state(run, &role.name),
            branch: workspace.role_worktrees.iter().find(|tree| tree.role == role.name).map(|tree| tree.branch.clone()),
            pane: run.pane_of(&role.name),
        };
        roles.enumerate().map(member).collect()
    }

    pub(super) fn run_history(run: &Run) -> flow::History {
        let now = SystemTime::now();
        let entry = |message: &Message| flow::Entry {
            seq: message.seq,
            from: message.from.clone(),
            to: message.to.clone(),
            kind: message.kind.word(),
            step: message.edge.map(|edge| step_of(run, edge)),
            colour: colour_of(run, &message.to),
            age: age(Some(message.time), now),
            head: head_of(message),
        };
        flow::History { entries: run.messages.iter().rev().map(entry).collect(), recorded: run.recorded }
    }

    pub(super) fn run_files(&self, run: &Run) -> Option<flow::Files> {
        let listed = self.panels.flow.files.as_ref().filter(|(id, _)| *id == run.id).map(|(_, listing)| listing);
        let Some(Listing { files, more }) = listed else {
            return run.dir.as_ref().map(|_| flow::Files { rows: Vec::new(), more: false });
        };
        let now = SystemTime::now();
        let row = |file: &folder::File| flow::FileRow {
            name: file.name.clone(),
            size: ui::files::size(file.bytes),
            age: age(file.modified, now),
        };
        Some(flow::Files { rows: files.iter().map(row).collect(), more: *more })
    }
}
