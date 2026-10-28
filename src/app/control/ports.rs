use std::collections::HashMap;

use super::{Asker, Handled, Pending, Stage, json};
use crate::agents;
use crate::app::App;
use crate::app::event::AppEvent;
use crate::app::ports_dialog::PortsEvent;
use crate::control::contract::{Code, Refusal};
use crate::control::ports::PortInfo;
use crate::control::{Done, Listing, PortStopped, Ports, StopPort};
use crate::error;
use crate::log::{Job, Level};
use crate::ports::stop::{self, Target};
use crate::ports::{self, Pane, Places, Port};
use crate::project::Hierarchy;
use crate::term::Term;

pub(in crate::app) struct PanePids {
    pub(in crate::app) shell: i32,
    pub(in crate::app) foreground: Option<i32>,
    pub(in crate::app) agent: Option<(String, i32)>,
}

impl App {
    pub(super) fn ports_request(&mut self, client: u64, asker: Asker, request: &Ports) -> Handled {
        let only = match (request.all, request.workspace) {
            (true, _) => None,
            (false, Some(id)) => Some(self.projects.workspace(self.usable_workspace(id)?).id),
            (false, None) => {
                let at = self.here(asker).workspace.ok_or("no workspace is open: pass --all to list every port")?;
                Some(self.projects.workspace(at).id)
            }
        };
        let places = self.port_places();
        let key = self.clients.requests.key();
        let job = Job::new(Level::Debug, "control", "ports").with("all", request.all);
        self.spawn_job(
            job,
            move || Ok(ports::scan(&places, only)),
            move |result, job| {
                job.note("found", result.as_ref().map_or(0, Vec::len));
                Some(AppEvent::Ports(PortsEvent::Answered { request: key, result }))
            },
        );
        self.pend_ports(client, key);
        Ok(None)
    }

    pub(super) fn stop_port_request(&mut self, client: u64, request: &StopPort) {
        let places = self.port_places();
        let key = self.clients.requests.key();
        let request = request.clone();
        let job = Job::new(Level::Info, "control", "stop port").with("port", request.port);
        self.spawn_job(
            job,
            move || Ok(stop_port(&places, &request)),
            move |result, _| {
                let result = result.unwrap_or_else(|error| Err(Refusal::new(Code::Failed, error.to_string())));
                Some(AppEvent::Ports(PortsEvent::StopAnswered { request: key, result }))
            },
        );
        self.pend_ports(client, key);
    }

    fn pend_ports(&mut self, client: u64, key: u64) {
        let pending = Pending { client: Some(client), key, timeout: None, done: Done::default(), stage: Stage::Ports };
        self.clients.requests.pending.push(pending);
    }

    pub(in crate::app) fn port_stop_answered(&mut self, key: u64, result: Result<PortStopped, Refusal>) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        self.refuse_or_answer(pending.client, result.map(|stopped| json(&stopped)));
    }

    pub(in crate::app) fn ports_found(&mut self, key: u64, result: error::Result<Vec<Port>>) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        let projects: HashMap<u64, u64> = self
            .projects
            .iter()
            .flat_map(|project| project.workspaces.iter().map(|workspace| (workspace.id, project.id)))
            .collect();
        let reply = result
            .map(|found| {
                let project = |port: &Port| port.place.workspace.and_then(|id| projects.get(&id).copied());
                json(&Listing { ports: found.iter().map(|port| PortInfo::of(port, project(port))).collect() })
            })
            .map_err(|e| format!("cannot list the ports: {e}"));
        self.answer(pending.client, reply);
    }

    pub(in crate::app) fn port_places(&self) -> Places {
        let mut places = Places { server: i32::try_from(std::process::id()).unwrap_or_default(), ..Places::default() };
        for workspace in self.projects.iter().flat_map(|project| &project.workspaces) {
            if workspace.worktree {
                places.worktrees.push((workspace.path.clone(), workspace.id));
            }
            places.worktrees.extend(workspace.role_worktrees.iter().map(|tree| (tree.path.clone(), workspace.id)));
            let panes = workspace.terms().filter_map(|term| self.port_pane(term, workspace.id));
            places.panes.extend(panes);
        }
        places
    }

    fn port_pane(&self, term: &Term, workspace: u64) -> Option<Pane> {
        let pids = self.pane_pids(term)?;
        Some(Pane { id: term.id, shell: pids.shell, workspace, agent: pids.agent })
    }

    pub(in crate::app) fn pane_pids(&self, term: &Term) -> Option<PanePids> {
        let shell = term.shell_pid()?;
        let running = term.agent.agent().map(str::to_string);
        let running = running.or_else(|| agents::detect(&self.config, &term.foreground_args()));
        let known = running.as_deref().and_then(agents::Known::of).map(|agent| agent.name().to_string());
        let foreground = term.foreground_pid();
        Some(PanePids { shell, foreground: foreground.filter(|pid| *pid != shell), agent: known.zip(foreground) })
    }
}

fn stop_port(places: &Places, request: &StopPort) -> Result<PortStopped, Refusal> {
    let found: Vec<Port> = ports::scan(places, None)
        .into_iter()
        .filter(|port| port.number == request.port && request.pid.is_none_or(|pid| pid == port.pid))
        .collect();
    let port = match found.as_slice() {
        [] => return Err(Refusal::new(Code::NotFound, format!("nothing listens on port {}", request.port))),
        [port] => port,
        _ => {
            let pids: Vec<String> = found.iter().map(|port| port.pid.to_string()).collect();
            let message = format!("several programs listen on port {}: pass --pid ({})", request.port, pids.join(", "));
            return Err(Refusal::new(Code::Conflict, message));
        }
    };
    if port.place.workspace.is_none() {
        let message = format!("port {} is not held by a program of a workspace, so it is not stopped", request.port);
        return Err(Refusal::new(Code::Conflict, message));
    }
    let target = Target::of(port);
    let (target, outcome) =
        stop::stop(vec![target]).into_iter().next().ok_or_else(|| Refusal::new(Code::Failed, "nothing was stopped"))?;
    let outcome = outcome.map_err(|reason| Refusal::new(Code::Conflict, reason))?;
    Ok(PortStopped { port: target.port, pid: target.pid, program: target.program, outcome: outcome.name().into() })
}
