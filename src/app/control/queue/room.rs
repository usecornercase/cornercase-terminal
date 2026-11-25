use std::time::Instant;

use super::queued_start;
use crate::agents;
use crate::app::App;
use crate::app::machine_memory::REFRESH;
use crate::app::notifications::{Message, MessageKind};
use crate::control::{FlowRun, Start};
use crate::log;
use crate::machines::HERE;
use crate::machines::memory::{self, Candidate, Left, Picked, is_auto};
use crate::machines::send::Send;
use crate::project::Hierarchy;
use crate::queue::outcome::closing;
use crate::queue::{self, Entry, Waiting};
use crate::ui;

enum Turn {
    Started,
    Waits(String, u64),
}

fn ordinal(position: usize) -> String {
    let suffix = match (position % 10, position % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{position}{suffix}")
}

fn sent(entry: &Entry, project: u64) -> Send {
    let task = &entry.task;
    let prompt = closing(&task.prompt);
    match &task.flow {
        Some(flow) => Send::FlowRun(FlowRun {
            name: flow.clone(),
            prompt,
            worktree: task.place.branch.clone(),
            project: Some(project),
            accounts: task.accounts.clone(),
            fresh: task.fresh,
            ..FlowRun::default()
        }),
        None => Send::Start(Start {
            agent: Some(task.agent.clone()),
            project: Some(project),
            worktree: task.place.branch.clone(),
            name: Some(task.label.clone()),
            prompt: Some(prompt),
            accounts: task.accounts.clone(),
            fresh: task.fresh,
            ..Start::default()
        }),
    }
}

impl App {
    pub(in crate::app) fn drive_free(&mut self, now: Instant) {
        let waiting = self.panels.queued.queue().waiting();
        self.panels.queued.waits.retain(|id, _| waiting.contains(id));
        if waiting.is_empty() {
            return;
        }
        self.refresh_memory(REFRESH);
        if self.memory_asking() {
            return;
        }
        let mut head = None;
        for (at, id) in waiting.into_iter().enumerate() {
            if let Some(head) = head {
                self.note_wait(id, at + 1, format!("behind queued agent {head}"), None);
                continue;
            }
            match self.free_turn(id, now) {
                Ok(Turn::Started) => {}
                Ok(Turn::Waits(why, needs)) => {
                    self.note_wait(id, at + 1, why, Some(needs));
                    head = Some(id);
                }
                Err(reason) => self.fail_queued(id, &reason),
            }
        }
    }

    pub(in crate::app) fn run_free_now(&mut self, id: u64, now: Instant) -> Result<(), String> {
        let entry = self.panels.queued.queue().entry(id).cloned().ok_or_else(|| queue::missing(id))?;
        let (p, _) = self.queue_place(&entry.task.place)?;
        let project = self.projects[p].id;
        match memory::pick(0, &self.free_candidates(&entry, project, now)) {
            Picked::Fits(best) => self.start_waiting(&entry, best.name.as_deref(), project, now),
            Picked::Nowhere { .. } => self.start_waiting(&entry, None, project, now),
        }
    }

    fn free_turn(&mut self, id: u64, now: Instant) -> Result<Turn, String> {
        let entry = self.panels.queued.queue().entry(id).cloned().ok_or_else(|| queue::missing(id))?;
        let (p, _) = self.queue_place(&entry.task.place)?;
        let project = self.projects[p].id;
        let needs = self.entry_memory(&entry, p)?;
        match memory::pick(needs, &self.free_candidates(&entry, project, now)) {
            Picked::Fits(best) => {
                self.start_waiting(&entry, best.name.as_deref(), project, now)?;
                self.reserve_memory(best.name.as_deref(), needs, now);
                Ok(Turn::Started)
            }
            nowhere @ Picked::Nowhere { .. } => Ok(Turn::Waits(nowhere.why(&self.project_name(project)), needs)),
        }
    }

    fn entry_memory(&self, entry: &Entry, p: usize) -> Result<u64, String> {
        let Some(name) = &entry.task.flow else { return Ok(self.config.default_memory()) };
        let (flow, _) = super::super::flow::pick(&self.flow_listing(Some(&self.projects[p].path)), name)?;
        Ok(flow.memory_needed().unwrap_or_else(|| self.config.default_memory()))
    }

    fn free_candidates(&self, entry: &Entry, project: u64, now: Instant) -> Vec<Candidate> {
        let candidates = self.memory_candidates(project, now);
        let Some(name) = entry.task.machine.as_deref().filter(|machine| !is_auto(machine)) else { return candidates };
        let named =
            |candidate: &Candidate| candidate.name.as_deref().is_some_and(|there| there.eq_ignore_ascii_case(name));
        let mut only: Vec<Candidate> = candidates.into_iter().filter(named).collect();
        if only.is_empty() {
            only.push(Candidate {
                name: Some(name.to_string()),
                left: Some(Left::Unreachable),
                ..Candidate::here(None, 0)
            });
        }
        only
    }

    fn start_waiting(&mut self, entry: &Entry, there: Option<&str>, project: u64, now: Instant) -> Result<(), String> {
        log::info!("queue", "starts after waiting for room", entry = entry.id, machine = there.unwrap_or(HERE));
        let Some(name) = there else { return self.start_free_here(entry, now) };
        let link = self.machine_link(name)?;
        let there = self.project_there(link, project).map(|there| there.id);
        let there = there.ok_or_else(|| format!("the project is no longer on {name}"))?;
        let client = self.mark_starting(entry.id, now)?;
        let answer = self.relay_request(client, name, sent(entry, there));
        self.routine_started(client, entry.id, answer)
    }

    fn start_free_here(&mut self, entry: &Entry, now: Instant) -> Result<(), String> {
        let (p, at) = self.queue_place(&entry.task.place)?;
        if let Some(name) = &entry.task.flow {
            let branch = entry.task.place.branch.clone().unwrap_or_default();
            return self.run_queued_flow(entry, p, name, (branch, entry.task.fresh));
        }
        if !agents::installed(&entry.task.program) {
            return Err(format!("{} is not installed", entry.task.agent));
        }
        let workspace = at.map(|at| self.projects.workspace(at).id);
        let start = queued_start(entry, (self.projects[p].id, workspace), entry.task.place.branch.clone());
        self.launch_start(entry.id, now, start)
    }

    fn note_wait(&mut self, id: u64, position: usize, why: String, needs: Option<u64>) {
        let since = crate::time::now();
        let known = self.panels.queued.waits.get(&id).map(|waiting| waiting.since);
        let waiting = Waiting { position, why, since: known.unwrap_or(since), needs };
        let first = known.is_none();
        self.panels.queued.waits.insert(id, waiting.clone());
        if first {
            log::info!("queue", "waits for room", entry = id, position = position, needs = needs.unwrap_or_default());
            let text =
                format!("{} waits for room: {} in line, {}", self.queued_name(id), ordinal(position), waiting.why);
            let said = Message {
                kind: MessageKind::QueueWaiting,
                text,
                workspace: None,
                panes: Vec::new(),
                run: None,
                role: None,
            };
            self.queue_notice(said, ui::ToastIcon::Check);
        }
    }
}
