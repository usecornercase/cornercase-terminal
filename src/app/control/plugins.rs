use std::time::Instant;

use serde_json::Value;

use super::{Handled, Pending, Stage, conflict, json, malformed, none};
use crate::app::App;
use crate::app::plugins::Spot;
use crate::control::contract::{Code, Refusal};
use crate::control::plugins::ActionRan;
use crate::control::{Done, Item, PluginActions, PluginRun};
use crate::project::Hierarchy;

fn not_found(message: String) -> Refusal {
    Refusal::new(Code::NotFound, message)
}

impl App {
    pub(super) fn plugin_list_request(&mut self, now: Instant) -> Value {
        self.reload_plugins();
        json(&self.plugin_infos(now))
    }

    pub(super) fn plugin_actions_request(&mut self, request: &PluginActions) -> Value {
        self.reload_plugins();
        json(&self.plugin_action_infos(request.context))
    }

    pub(super) fn plugin_run_request(&mut self, client: u64, request: &PluginRun) -> Handled {
        self.reload_plugins();
        let now = self.plugin_revision();
        if request.revision != now {
            return Err(conflict(format!(
                "the plugins changed since revision {} (they are at {now}); list the actions again with \
                 `cornercase plugin actions`",
                request.revision
            )));
        }
        let (plugin, action) = self.plugin_action(&request.plugin, &request.action).map_err(not_found)?;
        let spot = self.plugin_spot(request.item)?;
        if !action.contexts.contains(&spot.context()) {
            let contexts: Vec<&str> = action.contexts.iter().map(|c| c.name()).collect();
            return Err(malformed(format!(
                "`{}` of `{}` runs on {}, not on a {}",
                action.id,
                plugin.entry.id,
                contexts.join(", "),
                spot.context().name()
            )));
        }
        let key = self.clients.requests.key();
        if !self.start_plugin_action(spot, plugin, action, Some(key)) {
            return Err(not_found(format!("{} is gone", request.item)));
        }
        let pending =
            Pending { client: Some(client), key, timeout: None, done: Done::default(), stage: Stage::Reading };
        self.clients.requests.pending.push(pending);
        Ok(None)
    }

    pub(in crate::app) fn plugin_ran(&mut self, request: u64, ran: Result<ActionRan, String>) {
        let Some(pending) = self.clients.requests.take(request) else { return };
        self.refuse_or_answer(pending.client, ran.map(|ran| json(&ran)).map_err(Refusal::from));
    }

    fn plugin_spot(&self, item: Item) -> Result<Spot, Refusal> {
        let spot = match item {
            Item::Group(_) => {
                return Err(malformed("plugin actions run on a project, workspace, tab or pane, not a group"));
            }
            Item::Project(id) => {
                let p = self.projects.project_index(id).ok_or_else(|| none("project", id))?;
                if self.projects[p].closing {
                    return Err(format!("project {id} is closing").into());
                }
                return Ok(Spot::Project(id));
            }
            Item::Workspace(id) => {
                let (p, w) = self.projects.workspace_index(id).ok_or_else(|| none("workspace", id))?;
                Spot::Workspace(self.projects[p].id, self.projects[p].workspaces[w].id)
            }
            Item::Tab(id) => {
                let (p, w, t) = self.projects.tab_index(id).ok_or_else(|| none("tab", id))?;
                let workspace = &self.projects[p].workspaces[w];
                Spot::Tab(self.projects[p].id, workspace.id, workspace.tabs[t].id)
            }
            Item::Pane(id) => {
                self.projects.tab_of_pane(id).ok_or_else(|| none("pane", id))?;
                Spot::Pane(id)
            }
        };
        let workspace = match spot {
            Spot::Workspace(_, w) | Spot::Tab(_, w, _) => w,
            Spot::Pane(pane) => {
                let (p, w, _) = self.projects.tab_of_pane(pane).ok_or_else(|| none("pane", pane))?;
                self.projects[p].workspaces[w].id
            }
            Spot::Project(_) => return Ok(spot),
        };
        let (p, _) = self.usable_workspace(workspace)?;
        if self.projects[p].closing {
            return Err(format!("workspace {workspace} is closing").into());
        }
        Ok(spot)
    }
}
