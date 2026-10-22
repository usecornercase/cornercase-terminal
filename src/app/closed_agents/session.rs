use super::kept::{Kept, KeptRun};
use crate::app::App;
use crate::app::session::Parents;
use crate::flows::folder;
use crate::flows::progress::Progress;
use crate::flows::run::Run;
use crate::log::{self};
use crate::project::Hierarchy;
use crate::state::{ClosedState, ProjectState, TabState};
use std::collections::HashMap;

impl App {
    pub(in crate::app) fn save_closed(&self, projects: &mut [ProjectState]) {
        for (saved, project) in projects.iter_mut().zip(self.projects.iter()) {
            for (saved, workspace) in saved.workspaces.iter_mut().zip(&project.workspaces) {
                saved.closed = self.agents.closed.saved_of(workspace.id);
            }
        }
    }

    pub(in crate::app) fn restore_closed(&mut self, workspace: u64, saved: &ClosedState, parents: &mut Parents) {
        let mut tabs = saved.tabs.clone();
        let ids = self.reissue_closed(&mut tabs);
        parents.outside.extend(outside_parents(&mut tabs, &ids, workspace));
        let run = saved.run.as_ref().map(|state| {
            let mut run = Run::new(workspace, state.flow.clone(), state.task.clone());
            run.dir = state.dir.as_deref().filter(|name| folder::valid(name)).map(|name| self.runs.dir().join(name));
            run.ended.clone_from(&state.ended);
            run.id = self.take_id();
            run.progress = Progress::resumed(&run.flow, state.stage, &state.rounds);
            KeptRun { run, seats: state.members.clone() }
        });
        log::info!("agents", "closed agents restored", workspace = workspace, tabs = tabs.len(), run = run.is_some());
        self.agents.closed.restored(Kept { workspace, tabs, run });
    }

    fn reissue_closed(&mut self, tabs: &mut [TabState]) -> HashMap<u64, u64> {
        let mut ids = HashMap::new();
        for pane in tabs.iter_mut().flat_map(|tab| &mut tab.panes) {
            let new = self.take_id();
            if let Some(old) = pane.id.replace(new) {
                ids.insert(old, new);
            }
        }
        ids
    }

    pub(in crate::app) fn adopt_closed(&mut self, parents: &Parents) {
        for &(workspace, at, old) in &parents.outside {
            let found = parents.new_id(old).filter(|&new| self.projects.pane_by(new).is_some());
            self.agents.closed.set_parent(workspace, at, found);
        }
    }

    pub(in crate::app) fn forget_gone_closed(&mut self) {
        let projects = &self.projects;
        let exists = |id: u64| projects.workspace_by(id).is_some();
        let gone = self.agents.closed.forget_gone(exists, |pane| projects.pane_by(pane).is_some());
        for kept in gone {
            if let Some(run) = &kept.run {
                log::info!("flow", "run ended", workspace = kept.workspace, flow = run.run.flow.name);
            }
            log::info!("agents", "closed agents forgotten", workspace = kept.workspace, agents = kept.count());
        }
    }
}

fn outside_parents(tabs: &mut [TabState], ids: &HashMap<u64, u64>, workspace: u64) -> Vec<(u64, (usize, usize), u64)> {
    let mut outside = Vec::new();
    for (t, tab) in tabs.iter_mut().enumerate() {
        outside.extend(tab.panes.iter_mut().enumerate().filter_map(|(p, pane)| {
            let old = pane.parent?;
            pane.parent = ids.get(&old).copied();
            pane.parent.is_none().then_some((workspace, (t, p), old))
        }));
    }
    outside
}
