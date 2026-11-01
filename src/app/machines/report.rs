use super::Held;
use crate::app::App;
use crate::control::{MachineSeen, ProjectInfo, ShownRemote};
use crate::machines::{self, Info};

impl App {
    pub(in crate::app) fn machines_seen(&self) -> Vec<MachineSeen> {
        self.machines_held().into_iter().filter_map(|(info, held)| self.machine_seen(&info, held)).collect()
    }

    pub(super) fn machines_held(&self) -> Vec<(Info, &Held)> {
        let held = |destination: &str| self.machines.held.iter().find(|held| held.destination == destination);
        let listed = self.config.machines.iter().filter_map(|machine| Some((machine, held(&machine.destination)?)));
        listed.map(|(machine, held)| (Info::of(machine, Some(&held.status), None, None), held)).collect()
    }

    fn machine_seen(&self, info: &Info, held: &Held) -> Option<MachineSeen> {
        let seen = held.seen.as_deref()?;
        let paired = self.paired(seen);
        let projects = seen.projects.iter().zip(paired).map(|(project, paired)| on(project, &held.name, paired));
        Some(MachineSeen {
            name: held.name.clone(),
            icon: info.icon.clone(),
            state: info.state,
            seen: held.seen_at.and_then(machines::stamp),
            boot: seen.boot.clone(),
            projects: projects.collect(),
        })
    }

    pub(in crate::app) fn shown_remote_info(&self) -> Option<ShownRemote> {
        let shown = self.shown_remote()?;
        Some(ShownRemote {
            machine: shown.machine,
            boot: shown.boot,
            project: shown.project,
            workspace: shown.workspace,
            tab: shown.tab,
            path: shown.path,
        })
    }
}

fn on(project: &ProjectInfo, machine: &str, paired: Option<u64>) -> ProjectInfo {
    let mut project = project.clone();
    project.machine = Some(machine.to_string());
    project.paired = Some(paired);
    project.group = None;
    for workspace in &mut project.workspaces {
        workspace.machine = Some(machine.to_string());
    }
    project
}
