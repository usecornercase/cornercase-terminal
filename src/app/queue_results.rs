use std::collections::{HashMap, HashSet};

use ratatui::layout::Rect;

use super::App;
use super::event::AppEvent;
use crate::agents::activity::Status;
use crate::agents::context::{Pointer, Said};
use crate::control;
use crate::error::Result;
use crate::git;
use crate::log::{self, Job, Level};
use crate::project::Hierarchy;
use crate::pull_request::PullRequest;
use crate::queue::conversation::Talk;
use crate::queue::outcome::{self, Pull, Taken};
use crate::queue::{self, State};

#[derive(Default)]
pub(super) struct Watching {
    seen: HashMap<u64, Seen>,
    last: HashSet<u64>,
    generations: u64,
    applied: HashMap<u64, u64>,
}

impl Watching {
    pub(super) fn settled(&mut self, entry: u64) {
        self.last.remove(&entry);
    }

    fn retain(&mut self, watched: &HashSet<u64>) {
        self.seen.retain(|entry, _| watched.contains(entry));
        self.last.retain(|entry| watched.contains(entry));
    }

    fn generation(&mut self) -> u64 {
        self.generations += 1;
        self.generations
    }
}

#[derive(Clone)]
struct Seen {
    status: Option<Status>,
    agent: Option<String>,
    record: Option<Pointer>,
    workspace: Option<u64>,
    branch: Option<String>,
    pull: Option<Pull>,
}

struct Look {
    status: Option<Status>,
    agent: Option<String>,
    record: Option<Pointer>,
    workspace: Option<u64>,
    pull: Option<Pull>,
}

impl Seen {
    fn first(look: Look, branch: Option<String>) -> Self {
        let Look { status, agent, record, workspace, pull } = look;
        Self { status, agent, record, workspace, branch, pull }
    }

    fn merge(&mut self, look: Look) -> bool {
        let ended = outcome::turn_ended(self.status, look.status);
        self.status = look.status;
        self.agent = look.agent.or(self.agent.take());
        self.record = look.record.or(self.record.take());
        self.workspace = look.workspace.or(self.workspace);
        self.pull = look.pull.or(self.pull.take());
        ended
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Reader {
    Request(u64),
    Window,
}

#[derive(Debug)]
pub(crate) enum QueueRead {
    Captured { entry: u64, generation: u64, taken: Taken, read: Result<Option<Said>> },
    Conversation { entry: u64, to: Reader, result: Result<Talk> },
}

fn pull_of(pull: &PullRequest) -> Pull {
    Pull {
        number: pull.number,
        title: pull.title.clone(),
        url: pull.url.clone(),
        state: pull.state.name().to_string(),
        checks: pull.ci().map(|ci| ci.name().to_string()),
    }
}

impl App {
    fn queue_watched(&self) -> Vec<(u64, u64)> {
        let token = control::server_token();
        let ours = |state: &State| match state {
            State::Started { pane, server } | State::Kept { pane, server, .. } if server == token => Some(*pane),
            _ => None,
        };
        self.panels.queued.queue().entries.iter().filter_map(|e| ours(&e.state).map(|pane| (e.id, pane))).collect()
    }

    fn queue_look(&self, pane: u64) -> Option<Look> {
        let term = self.projects.pane_by(pane)?;
        let workspace = self.projects.tab_of_pane(pane).map(|at| self.projects.workspace(at.workspace()).id);
        Some(Look {
            status: term.agent.status(),
            agent: term.agent.agent().map(str::to_string),
            record: term.context.kind().and_then(|kind| term.context.record_for(kind)).as_ref().map(Pointer::of),
            workspace,
            pull: workspace.and_then(|id| self.pull_request(id)).map(pull_of),
        })
    }

    fn workspace_branch(&self, workspace: Option<u64>) -> Option<String> {
        let found = self.projects.workspace_by(workspace?)?;
        git::branch(&found.path)
    }

    pub(super) fn watch_queue_results(&mut self) {
        let watched = self.queue_watched();
        let ids: HashSet<u64> = watched.iter().map(|(entry, _)| *entry).collect();
        self.panels.queued.results.retain(&ids);
        for (entry, pane) in watched {
            if self.panels.queued.results.last.contains(&entry) {
                continue;
            }
            match self.queue_look(pane) {
                Some(look) => self.queue_looked(entry, look),
                None => self.queue_pane_closed(entry),
            }
        }
    }

    fn queue_looked(&mut self, entry: u64, look: Look) {
        if self.panels.queued.results.seen.contains_key(&entry) {
            let seen = self.panels.queued.results.seen.get_mut(&entry);
            if seen.is_some_and(|seen| seen.merge(look)) {
                self.capture_seen(entry);
            }
            return;
        }
        let branch = self.workspace_branch(look.workspace);
        self.panels.queued.results.seen.insert(entry, Seen::first(look, branch));
    }

    fn queue_pane_closed(&mut self, entry: u64) {
        if let Some(seen) = self.panels.queued.results.seen.remove(&entry) {
            self.capture(entry, &seen);
        }
    }

    fn capture_seen(&mut self, entry: u64) {
        if let Some(seen) = self.panels.queued.results.seen.get(&entry).cloned() {
            self.capture(entry, &seen);
        }
    }

    pub(super) fn capture_before_removal(&mut self, run: u64) {
        let pane = self.queue_watched().into_iter().find(|(entry, _)| *entry == run).map(|(_, pane)| pane);
        if let Some(look) = pane.and_then(|pane| self.queue_look(pane)) {
            self.queue_looked(run, look);
        }
        self.queue_pane_closed(run);
        self.panels.queued.results.last.insert(run);
    }

    fn taken(&self, entry: u64, seen: &Seen) -> Taken {
        let placed = self.panels.queued.queue().entry(entry).and_then(|entry| entry.task.place.branch.clone());
        let live = seen.workspace.and_then(|id| self.pull_request(id)).map(pull_of);
        Taken {
            agent: seen.agent.clone(),
            record: seen.record.clone(),
            branch: self.workspace_branch(seen.workspace).or_else(|| seen.branch.clone()).or(placed),
            pull_request: live.or_else(|| seen.pull.clone()),
        }
    }

    fn capture(&mut self, entry: u64, seen: &Seen) {
        let taken = self.taken(entry, seen);
        let generation = self.panels.queued.results.generation();
        let Some(record) = seen.record.as_ref().map(Pointer::record) else {
            self.queue_captured(entry, generation, taken, Ok(None));
            return;
        };
        let agent = seen.agent.clone().unwrap_or_default();
        let job = Job::new(Level::Info, "queue", "capture result").with("entry", entry).with("agent", agent);
        self.spawn_job(
            job,
            move || record.last_message(),
            move |read, _| Some(AppEvent::QueueRead(Box::new(QueueRead::Captured { entry, generation, taken, read }))),
        );
    }

    pub(super) fn queue_captured(&mut self, entry: u64, generation: u64, taken: Taken, read: Result<Option<Said>>) {
        let applied = &mut self.panels.queued.results.applied;
        if applied.get(&entry).is_some_and(|done| *done >= generation) {
            return;
        }
        let Ok(said) = read else { return };
        applied.insert(entry, generation);
        let Some(kept) = self.panels.queued.keep_result(entry, taken, said, crate::time::now()) else { return };
        log::info!(
            "queue",
            "result kept",
            entry = entry,
            agent = kept.agent.clone().unwrap_or_default(),
            bytes = kept.text.len(),
            cut = kept.cut
        );
    }

    pub(in crate::app) fn readable_record(&self, id: u64) -> Option<Pointer> {
        let entry = self.panels.queued.queue().entry(id)?;
        entry.result.as_ref()?.record.clone().filter(Pointer::readable)
    }

    pub(in crate::app) fn read_conversation(&self, entry: u64, pointer: Pointer, to: Reader) {
        let record = pointer.record();
        let job = Job::new(Level::Debug, "queue", "read conversation").with("entry", entry);
        self.spawn_job(
            job,
            move || queue::conversation::read(&record),
            move |result, job| {
                job.note("messages", result.as_ref().map_or(0, |talk| talk.messages.len()));
                Some(AppEvent::QueueRead(Box::new(QueueRead::Conversation { entry, to, result })))
            },
        );
    }

    pub(super) fn queue_read(&mut self, event: QueueRead, area: Rect) {
        match event {
            QueueRead::Captured { entry, generation, taken, read } => {
                self.queue_captured(entry, generation, taken, read)
            }
            QueueRead::Conversation { entry, to: Reader::Request(key), result } => {
                self.queue_conversation_answered(key, entry, result);
            }
            QueueRead::Conversation { entry, to: Reader::Window, result } => {
                self.queue_conversation_shown(entry, result, area);
            }
        }
    }
}
