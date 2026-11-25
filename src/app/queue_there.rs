use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::App;
use super::event::AppEvent;
use super::machine_links::{Held, RemoteItem};
use super::notifications::{Message, MessageKind};
use super::queue_results::{QueueResultEvent, pull_of};
use crate::agents::activity::Status;
use crate::control::PaneInfo;
use crate::log::{Job, Level};
use crate::machines::State as Link;
use crate::pull_request;
use crate::queue::outcome::{Pull, Taken, turn_ended};
use crate::queue::{Entry, State, There};
use crate::ui;

const PULL_EVERY: Duration = Duration::from_mins(2);
const PULL_FOR: i64 = 24 * 3_600;

#[derive(Default)]
pub(super) struct Following {
    status: HashMap<u64, Option<Status>>,
    asked: HashMap<u64, Instant>,
    looking: HashSet<u64>,
}

impl Following {
    pub(super) fn follows(entry: &Entry) -> bool {
        let pulled = entry.result.as_ref().is_some_and(|result| result.pull_request.is_some());
        let recent = entry.started.is_some_and(|at| crate::time::now().saturating_sub(at) < PULL_FOR);
        entry.there.is_some() && matches!(entry.state, State::Started { .. }) && !pulled && recent
    }
}

fn status_of(name: &str) -> Option<Status> {
    [Status::Idle, Status::Working, Status::Done, Status::Pending, Status::Failed, Status::Waiting]
        .into_iter()
        .find(|status| status.name() == name)
}

fn pane_there(held: &Held, pane: u64) -> Option<&PaneInfo> {
    let seen = held.seen.as_deref()?;
    let workspaces = seen.projects.iter().flat_map(|project| &project.workspaces);
    let tabs = workspaces.flat_map(|workspace| &workspace.tabs);
    tabs.flat_map(|tab| &tab.panes).find(|info| info.id == pane)
}

impl App {
    pub(super) fn queue_started_there(&mut self, id: u64, machine: &str, value: &Value) {
        let number = |key: &str| value.get(key).and_then(Value::as_u64);
        let first = |key: &str| value.pointer(&format!("/members/0/{key}")).and_then(Value::as_u64);
        let pane = number("pane").or_else(|| first("pane")).unwrap_or_default();
        let tab = number("tab").or_else(|| first("tab"));
        let boot = self.held_named(machine).and_then(|held| held.seen.as_ref()).map(|seen| seen.boot.clone());
        let there = There { machine: machine.to_string(), workspace: number("workspace"), tab };
        if let Some(branch) = value.get("branch").and_then(Value::as_str) {
            self.panels.queued.set_branch(id, branch);
        }
        self.panels.queued.started_there(id, (pane, boot.unwrap_or_else(|| machine.to_string())), there);
        if value.get("unsent").and_then(Value::as_bool) == Some(true) {
            self.panels.queued.set_unsent(id, true);
            return self.queue_unsent_notice(id, None, Vec::new());
        }
        let text = format!("started {} on {machine}", self.queued_name(id));
        let said = Message {
            kind: MessageKind::QueueStarted,
            text,
            workspace: None,
            panes: Vec::new(),
            run: None,
            role: None,
        };
        self.queue_notice(said, ui::ToastIcon::Check);
    }

    fn held_named(&self, name: &str) -> Option<&Held> {
        self.machines.held().iter().find(|held| held.name.eq_ignore_ascii_case(name))
    }

    pub(super) fn there_status(&self, entry: &Entry) -> Option<String> {
        let there = entry.there.as_ref().filter(|_| matches!(entry.state, State::Started { .. }))?;
        let State::Started { pane, .. } = entry.state else { return None };
        let name = &there.machine;
        let Some(held) = self.held_named(name) else {
            return Some(format!("started on {name} · {name} is no longer listed"));
        };
        if held.status.state != Link::Linked {
            return Some(format!("started on {name} · paused while {name} is unreachable"));
        }
        Some(match pane_there(held, pane) {
            Some(info) => match info.status.as_deref() {
                Some(status) => format!("started on {name} · {status}"),
                None => format!("started on {name}"),
            },
            None => format!("started on {name} · tab closed"),
        })
    }

    pub(super) fn open_there(&mut self, entry: &Entry) -> bool {
        let Some(there) = &entry.there else { return false };
        let Some(tab) = there.tab else { return false };
        let shown = self.select_remote(&there.machine, RemoteItem::Tab(tab));
        if shown {
            self.overlay = None;
        }
        shown
    }

    pub(in crate::app) fn follow_there(&mut self, now: Instant) {
        let followed: Vec<Entry> = {
            let queued = &self.panels.queued;
            queued.queue().entries.iter().filter(|entry| Following::follows(entry)).cloned().collect()
        };
        let ids: HashSet<u64> = followed.iter().map(|entry| entry.id).collect();
        let following = &mut self.panels.queued.there;
        following.status.retain(|id, _| ids.contains(id));
        following.asked.retain(|id, _| ids.contains(id));
        for entry in followed {
            self.look_there(&entry);
            self.ask_pull_there(&entry, now);
        }
    }

    fn look_there(&mut self, entry: &Entry) {
        let (Some(there), State::Started { pane, .. }) = (&entry.there, &entry.state) else { return };
        let Some(held) = self.held_named(&there.machine).filter(|held| held.status.state == Link::Linked) else {
            return;
        };
        let status = pane_there(held, *pane).and_then(|info| info.status.as_deref()).and_then(status_of);
        let before = self.panels.queued.there.status.insert(entry.id, status);
        if before.is_some_and(|before| turn_ended(before, status)) {
            self.capture_there(entry.id, None);
        }
    }

    fn ask_pull_there(&mut self, entry: &Entry, now: Instant) {
        let Some(branch) = entry.task.place.branch.clone() else { return };
        let following = &self.panels.queued.there;
        let due = following.asked.get(&entry.id).is_none_or(|at| now.saturating_duration_since(*at) >= PULL_EVERY);
        if !due || following.looking.contains(&entry.id) {
            return;
        }
        let project = self.projects.iter().find(|project| project.path == entry.task.place.project);
        let Some(dir) = project.map(|project| project.path.clone()) else { return };
        self.panels.queued.there.asked.insert(entry.id, now);
        self.panels.queued.there.looking.insert(entry.id);
        self.spawn_pull_there(entry.id, PathBuf::from(&self.config.gh), dir, branch);
    }

    fn spawn_pull_there(&self, entry: u64, gh: PathBuf, dir: PathBuf, branch: String) {
        let job = Job::new(Level::Debug, "queue", "pull request there").with("entry", entry);
        self.spawn_job(
            job,
            move || pull_request::find_branch(&gh, &dir, &branch),
            move |found, _| {
                let found = found.map(|pull| pull.as_ref().map(pull_of)).map_err(|error| error.to_string());
                Some(AppEvent::QueueResult(Box::new(QueueResultEvent::Pulled { entry, found })))
            },
        );
    }

    pub(super) fn pulled_there(&mut self, entry: u64, found: Result<Option<Pull>, String>) {
        self.panels.queued.there.looking.remove(&entry);
        let Ok(Some(pull)) = found else { return };
        self.capture_there(entry, Some(pull.clone()));
        self.pull_opened(entry, &pull);
    }

    fn capture_there(&mut self, id: u64, pull: Option<Pull>) {
        let Some(entry) = self.panels.queued.queue().entry(id) else { return };
        let known = entry.result.as_ref().and_then(|result| result.pull_request.clone());
        let taken = Taken {
            agent: entry.task.flow.as_ref().map_or_else(|| Some(entry.task.agent.clone()), |_| None),
            record: None,
            branch: entry.task.place.branch.clone(),
            pull_request: pull.or(known),
        };
        let generation = self.panels.queued.results.generation();
        self.queue_captured(id, generation, taken, Ok(None));
    }

    pub(super) fn pull_opened(&mut self, id: u64, pull: &Pull) {
        let Some(entry) = self.panels.queued.queue().entry(id).filter(|entry| entry.is_free()) else { return };
        let workspace = entry.task.place.workspace_id.filter(|_| entry.there.is_none());
        let text = format!("{}'s pull request opened: #{} {}", self.queued_name(id), pull.number, pull.title);
        let said =
            Message { kind: MessageKind::QueuePullRequest, text, workspace, panes: Vec::new(), run: None, role: None };
        self.queue_notice(said, ui::ToastIcon::Check);
    }
}
