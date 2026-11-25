use std::time::{Duration, Instant};

use super::flow::pick;
use super::{Asked, Asker, Handled, NO_PROJECT, Stage, Waits, malformed, none};
use crate::app::App;
use crate::app::machine_memory::REFRESH;
use crate::control::contract::{Code, Refusal};
use crate::control::{Done, FlowRun, Start};
use crate::machines::memory::{Best, Picked};
use crate::machines::send::Send;
use crate::project::Hierarchy;

const WAIT_AT_MOST: Duration = Duration::from_secs(8);
const BOOT_AND_AUTO: &str = "`machine_boot` names the ids of one machine; `auto` picks one, so it takes the ids here";

pub(in crate::app) enum Wanted {
    Start(Box<Start>),
    StartWith(Box<Start>),
    FlowRun(Box<FlowRun>),
}

impl Wanted {
    fn machine_boot(&self) -> Option<&String> {
        match self {
            Self::Start(start) | Self::StartWith(start) => start.machine_boot.as_ref(),
            Self::FlowRun(run) => run.machine_boot.as_ref(),
        }
    }
}

pub(in crate::app) struct AutoWait {
    key: u64,
    asked: Asked,
    wanted: Wanted,
    project: u64,
    since: Instant,
}

impl App {
    pub(super) fn auto_start(&mut self, asked: Asked, start: Start, with: bool) -> Handled {
        let project = self.auto_project(asked.asker, start.project, start.workspace)?;
        let wanted = if with { Wanted::StartWith(Box::new(start)) } else { Wanted::Start(Box::new(start)) };
        self.auto_or_wait(asked, wanted, project)
    }

    pub(super) fn auto_flow_run(&mut self, asked: Asked, run: FlowRun) -> Handled {
        let project = self.auto_project(asked.asker, run.project, None)?;
        self.auto_or_wait(asked, Wanted::FlowRun(Box::new(run)), project)
    }

    fn auto_project(&self, asker: Asker, project: Option<u64>, workspace: Option<u64>) -> Result<u64, Refusal> {
        if let Some(id) = project {
            return self.projects.project_index(id).map(|_| id).ok_or_else(|| none("project", id));
        }
        if let Some(id) = workspace {
            let at = self.projects.workspace_index(id).ok_or_else(|| none("workspace", id))?;
            return Ok(self.projects[at.project()].id);
        }
        let p = self.here(asker).project.ok_or(NO_PROJECT)?;
        Ok(self.projects[p].id)
    }

    fn auto_or_wait(&mut self, asked: Asked, wanted: Wanted, project: u64) -> Handled {
        if wanted.machine_boot().is_some() {
            return Err(malformed(BOOT_AND_AUTO));
        }
        self.refresh_memory(REFRESH);
        if !self.memory_asking() {
            return self.auto_go(asked, wanted, project);
        }
        let key = self.clients.requests.key();
        let waits = Waits { done: Done::default(), stage: Stage::Reading, timeout: None };
        self.clients.requests.push(asked.client, key, waits);
        let since = Instant::now();
        self.machines.memory.waiting.push(AutoWait { key, asked, wanted, project, since });
        Ok(None)
    }

    pub(in crate::app) fn auto_waiting(&mut self, now: Instant) {
        let asking = self.memory_asking();
        let waiting = std::mem::take(&mut self.machines.memory.waiting);
        let (ready, still): (Vec<AutoWait>, Vec<AutoWait>) =
            waiting.into_iter().partition(|wait| !asking || now.saturating_duration_since(wait.since) >= WAIT_AT_MOST);
        self.machines.memory.waiting = still;
        for wait in ready {
            if self.clients.requests.take(wait.key).is_none() {
                continue;
            }
            let handled = self.auto_go(wait.asked, wait.wanted, wait.project);
            self.answer_handled(wait.asked.client, handled);
        }
    }

    pub(in crate::app) fn auto_wake(&self, now: Instant) -> Option<Duration> {
        let since = self.machines.memory.waiting.iter().map(|wait| wait.since).min()?;
        Some((since + WAIT_AT_MOST).saturating_duration_since(now))
    }

    fn memory_needed(&self, wanted: &Wanted, project: u64) -> Result<u64, Refusal> {
        let Wanted::FlowRun(run) = wanted else { return Ok(self.config.default_memory()) };
        Ok(self.flow_needs(project, &run.name)?)
    }

    pub(in crate::app) fn flow_needs(&self, project: u64, name: &str) -> Result<u64, String> {
        let p = self.projects.project_index(project).ok_or_else(|| none("project", project).message)?;
        let (flow, _) = pick(&self.flow_listing(Some(&self.projects[p].path)), name)?;
        Ok(flow.memory_needed().unwrap_or_else(|| self.config.default_memory()))
    }

    fn auto_go(&mut self, asked: Asked, wanted: Wanted, project: u64) -> Handled {
        let needs = self.memory_needed(&wanted, project)?;
        let picked = self.pick_machine(project, needs, Instant::now());
        let label = self.project_name(project);
        match &picked {
            Picked::Fits(best) => {
                self.clients.requests.picked.insert(asked.client, picked.why(&label));
                self.auto_fits(asked, wanted, project, (best, needs))
            }
            Picked::Nowhere { .. } => Err(Refusal::new(Code::NoRoom, picked.refusal(&label))),
        }
    }

    pub(in crate::app) fn project_name(&self, project: u64) -> String {
        let p = self.projects.project_index(project);
        p.map(|p| self.project_label(&self.projects[p])).unwrap_or_default()
    }

    fn auto_fits(&mut self, asked: Asked, wanted: Wanted, project: u64, (best, needs): (&Best, u64)) -> Handled {
        self.reserve_memory(best.name.as_deref(), needs, Instant::now());
        let Some(name) = best.name.clone() else {
            return match wanted {
                Wanted::Start(start) | Wanted::StartWith(start) => {
                    self.start_request(asked, Start { machine: None, ..*start })
                }
                Wanted::FlowRun(run) => self.flow_run_request(asked, FlowRun { machine: None, ..*run }),
            };
        };
        let link = self.machine_link(&name)?;
        let there = self.project_there(link, project).map(|there| there.id).ok_or_else(|| {
            Refusal::new(Code::Unavailable, format!("the project is no longer on {name}; nothing was started"))
        })?;
        let placed =
            |start: Box<Start>| Start { machine: Some(name.clone()), project: Some(there), workspace: None, ..*start };
        let send = match wanted {
            Wanted::Start(start) => Send::Start(placed(start)),
            Wanted::StartWith(start) => Send::StartWith(placed(start)),
            Wanted::FlowRun(run) => {
                Send::FlowRun(FlowRun { machine: Some(name.clone()), project: Some(there), ..*run })
            }
        };
        self.relay_request(asked.client, &name, send)
    }
}
