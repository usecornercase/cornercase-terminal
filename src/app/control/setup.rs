use std::collections::BTreeMap;

use super::{Asked, Asker, Handled, Reply, Stage, Waits, conflict, json, sized};
use crate::app::App;
use crate::control::contract::Refusal;
use crate::control::{Done, Item, RunSetup};
use crate::git::worktree::setup;
use crate::project::{Hierarchy, Workspace, WorkspaceAt};

const NO_WORKSPACE: &str = "no workspace is open here; pass --workspace or --worktree (`cornercase status` lists them)";

impl App {
    pub(super) fn run_setup_request(&mut self, asked: Asked, run: &RunSetup) -> Handled {
        let Asked { client, asker, area, .. } = asked;
        let area = sized(area)?;
        let at = self.setup_place(asker, run.workspace)?;
        if let Some(refusal) = self.setup_refusal(at) {
            return Err(conflict(refusal));
        }
        let accounts = self.requested_accounts_from(asker.caller, None, &BTreeMap::new())?;
        let tab = self.push_setup_tab_in(at, area, None, accounts).map_err(|e| e.to_string())?;
        let ids = self.ids_of_tab(tab);
        let pane = ids.pane.unwrap_or_default();
        self.adopt(pane, asker.caller);
        let projection = self.show_if(asker.view, run.focus, Item::Pane(pane))?;
        let waits = Waits {
            done: Done { ids, projection, ..Done::default() },
            stage: Stage::Setup(ids.workspace.unwrap_or_default()),
            timeout: None,
        };
        let key = self.clients.requests.key();
        self.clients.requests.push(client, key, waits);
        Ok(None)
    }

    fn setup_place(&self, asker: Asker, workspace: Option<u64>) -> Result<WorkspaceAt, Refusal> {
        let id = match workspace {
            Some(id) => id,
            None => self.projects.workspace(self.here(asker).workspace.ok_or(NO_WORKSPACE)?).id,
        };
        self.usable_workspace(id)
    }

    pub(super) fn setup_reply(&self, workspace: u64, done: &Done) -> Reply {
        let ended = self.projects.workspace_by(workspace).and_then(Workspace::setup);
        if let Some((pane, setup::Pane::Failed { code, .. })) = ended {
            return Err(format!(
                "the setup of workspace {workspace} failed with {code}; `cornercase read --pane {pane}` shows its output"
            ));
        }
        if done.ids.pane.and_then(|pane| self.projects.pane_by(pane)).is_none() {
            return Err(format!("the setup of workspace {workspace} was closed before it ended"));
        }
        Ok(json(done))
    }
}
