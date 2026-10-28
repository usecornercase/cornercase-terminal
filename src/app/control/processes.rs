use std::time::Duration;

use super::{Pending, Stage, json};
use crate::app::App;
use crate::app::control::Handled;
use crate::app::control::none;
use crate::app::event::AppEvent;
use crate::app::processes_dialog::ProcessesEvent;
use crate::control::contract::{Code, Refusal};
use crate::control::processes::{OwnProcesses, PaneProcesses, ProcessInfo, ProcessList, ProcessStopped, ProcessTotal};
use crate::control::{Done, Processes, StopProcess};
use crate::error;
use crate::log::{Job, Level};
use crate::process::owners::{Owner, owner, trees};
use crate::processes::stop::{self, Group};
use crate::processes::{self, Pane, PaneTree, Places, Proc, Sample, sum_cpu, sum_memory};
use crate::project::{Hierarchy, Workspace};
use crate::{process, term::Term};

const DEFAULT_INTERVAL: u64 = 1_000;

const MIN_INTERVAL: u64 = 100;

const MAX_INTERVAL: u64 = 10_000;

impl App {
    pub(in crate::app) fn process_places(&self) -> Places {
        let server = i32::try_from(std::process::id()).unwrap_or_default();
        let panes = self.projects.iter().flat_map(|project| &project.workspaces).flat_map(Workspace::terms);
        Places { panes: panes.filter_map(|term| self.process_pane(term)).collect(), server, own: true }
    }

    fn process_pane(&self, term: &Term) -> Option<Pane> {
        let pids = self.pane_pids(term)?;
        Some(Pane { id: term.id, shell: pids.shell, foreground: pids.foreground, agent: pids.agent })
    }

    pub(super) fn processes_request(&mut self, client: u64, request: &Processes) -> Handled {
        let mut places = self.process_places();
        self.narrowed(&mut places, request)?;
        let interval = request.interval.unwrap_or(DEFAULT_INTERVAL).clamp(MIN_INTERVAL, MAX_INTERVAL);
        let key = self.clients.requests.key();
        let job = Job::new(Level::Debug, "control", "processes");
        self.spawn_job(
            job,
            move || Ok(processes::sample(&places, None, Duration::from_millis(interval))),
            move |result, job| {
                job.note("panes", result.as_ref().map_or(0, |sample| sample.panes.len()));
                Some(AppEvent::Processes(ProcessesEvent::Answered { request: key, interval, result }))
            },
        );
        self.pend_processes(client, key);
        Ok(None)
    }

    fn narrowed(&self, places: &mut Places, request: &Processes) -> Result<(), Refusal> {
        if let Some(pane) = request.pane {
            self.projects.tab_of_pane(pane).ok_or_else(|| none("pane", pane))?;
            places.panes.retain(|candidate| candidate.id == pane);
            places.own = false;
        }
        if let Some(id) = request.workspace {
            let at = self.usable_workspace(id)?;
            let ids: Vec<u64> = self.projects.workspace(at).terms().map(|term| term.id).collect();
            places.panes.retain(|candidate| ids.contains(&candidate.id));
            places.own = false;
        }
        Ok(())
    }

    pub(super) fn stop_process_request(&mut self, client: u64, request: &StopProcess) {
        let places = self.process_places();
        let key = self.clients.requests.key();
        let request = request.clone();
        let job = Job::new(Level::Info, "control", "stop process").with("pid", request.pid);
        self.spawn_job(
            job,
            move || Ok(stop_process(&places, &request)),
            move |result, _| {
                let result = result.unwrap_or_else(|error| Err(Refusal::new(Code::Failed, error.to_string())));
                Some(AppEvent::Processes(ProcessesEvent::StopAnswered { request: key, result }))
            },
        );
        self.pend_processes(client, key);
    }

    fn pend_processes(&mut self, client: u64, key: u64) {
        let stage = Stage::Processes;
        let pending = Pending { client: Some(client), key, timeout: None, done: Done::default(), stage };
        self.clients.requests.pending.push(pending);
    }

    pub(in crate::app) fn process_stop_answered(&mut self, key: u64, result: Result<ProcessStopped, Refusal>) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        self.refuse_or_answer(pending.client, result.map(|stopped| json(&stopped)));
    }

    pub(in crate::app) fn processes_found(&mut self, key: u64, interval: u64, result: error::Result<Sample>) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        let reply = result
            .map(|sample| json(&self.process_list(&sample, interval)))
            .map_err(|e| format!("cannot read the processes: {e}"));
        self.answer(pending.client, reply);
    }

    fn process_list(&self, sample: &Sample, interval: u64) -> ProcessList {
        let panes: Vec<PaneProcesses> = sample.panes.iter().map(|tree| self.pane_processes(tree)).collect();
        let own = (!sample.cornercase.is_empty()).then(|| OwnProcesses {
            pid: sample.cornercase.first().map_or(0, |proc| proc.pid),
            cpu: round(sum_cpu(&sample.cornercase)),
            memory: sum_memory(&sample.cornercase),
            processes: sample.cornercase.iter().map(info).collect(),
        });
        let total = ProcessTotal {
            cpu: round(panes.iter().map(|pane| pane.cpu).sum::<f64>() + own.as_ref().map_or(0.0, |own| own.cpu)),
            memory: panes.iter().map(|pane| pane.memory).sum::<u64>() + own.as_ref().map_or(0, |own| own.memory),
            panes: panes.len(),
            processes: panes.iter().map(|pane| pane.processes.len()).sum::<usize>()
                + own.as_ref().map_or(0, |own| own.processes.len()),
        };
        ProcessList { interval, total, panes, cornercase: own }
    }

    fn pane_processes(&self, tree: &PaneTree) -> PaneProcesses {
        let at = self.projects.tab_of_pane(tree.pane.id);
        let term = self.projects.pane_by(tree.pane.id);
        PaneProcesses {
            pane: tree.pane.id,
            tab: at.map(|at| self.projects.tab(at).id),
            workspace: at.map(|at| self.projects.workspace(at.workspace()).id),
            project: at.map(|at| self.projects[at.project()].id),
            program: term.and_then(|term| term.program(&self.config)),
            agent: tree.pane.agent.as_ref().map(|(name, _)| name.clone()),
            foreground: tree.pane.foreground,
            pid: tree.pane.shell,
            cpu: round(sum_cpu(&tree.procs)),
            memory: sum_memory(&tree.procs),
            processes: tree.procs.iter().map(info).collect(),
        }
    }
}

fn round(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn info(proc: &Proc) -> ProcessInfo {
    ProcessInfo {
        pid: proc.pid,
        parent: proc.parent,
        name: proc.name.clone(),
        args: proc.args.clone(),
        started: proc.started,
        cpu: proc.cpu.map(round),
        memory: proc.memory,
    }
}

fn conflict(message: String) -> Refusal {
    Refusal::new(Code::Conflict, message)
}

fn pane_of(places: &Places, pid: i32) -> Option<u64> {
    let shells: Vec<(i32, u64)> = places.panes.iter().map(|pane| (pane.shell, pane.id)).collect();
    match owner(pid, &trees(&shells), places.server) {
        Owner::Pane(pane) => Some(*pane),
        _ => None,
    }
}

fn refusal(places: &Places, pid: i32) -> Option<Refusal> {
    if pid <= 1 || !process::running(pid, None) {
        return Some(Refusal::new(Code::NotFound, format!("there is no running process with pid {pid}")));
    }
    if let Some(pane) = places.panes.iter().find(|pane| pane.shell == pid) {
        let message = format!(
            "pid {pid} is the shell of pane {0}: close the pane instead (cornercase close --pane {0})",
            pane.id
        );
        return Some(conflict(message));
    }
    if pane_of(places, pid).is_some() {
        return None;
    }
    if pid == places.server || stop::inside(pid, places.server) {
        return Some(conflict(format!("pid {pid} is cornercase itself")));
    }
    Some(conflict(format!("pid {pid} is not a program of a cornercase pane, so it is not stopped")))
}

fn stop_process(places: &Places, request: &StopProcess) -> Result<ProcessStopped, Refusal> {
    let pid = request.pid;
    if let Some(refusal) = refusal(places, pid) {
        return Err(refusal);
    }
    if request.started.is_some_and(|started| process::start_stamp(pid) != Some(started)) {
        return Err(conflict(format!("pid {pid} is no longer the program that was listed")));
    }
    let pane = pane_of(places, pid);
    let (group, outcome) = stop::stop(Group::live_with(pid, request.started))
        .into_iter()
        .next()
        .ok_or_else(|| Refusal::new(Code::Failed, "nothing was stopped"))?;
    let outcome = outcome.map_err(conflict)?;
    Ok(ProcessStopped {
        pid,
        program: Some(group.root.name.clone()),
        pane,
        outcome: outcome.name().into(),
        processes: group.size(),
    })
}
