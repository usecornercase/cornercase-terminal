use std::collections::HashMap;

use super::{Asker, Handled, Pending, Stage, json};
use crate::agents;
use crate::app::App;
use crate::app::event::AppEvent;
use crate::control::ports::PortInfo;
use crate::control::{Done, Listing, Ports};
use crate::error;
use crate::log::{Job, Level};
use crate::ports::{self, Pane, Places, Port};
use crate::project::Hierarchy;
use crate::term::Term;

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
                Some(AppEvent::Ports { request: key, result })
            },
        );
        let pending = Pending { client: Some(client), key, timeout: None, done: Done::default(), stage: Stage::Ports };
        self.clients.requests.pending.push(pending);
        Ok(None)
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
        let shell = term.shell_pid()?;
        let running = term.agent.agent().map(str::to_string);
        let running = running.or_else(|| agents::detect(&self.config, &term.foreground_args()));
        let known = running.as_deref().and_then(agents::Known::of).map(|agent| agent.name().to_string());
        let agent = known.zip(term.foreground_pid());
        Some(Pane { id: term.id, shell, workspace, agent })
    }
}
