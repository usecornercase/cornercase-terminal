use std::time::Instant;

use serde_json::Value;

use super::{Handled, Pending, Stage, conflict, json, malformed, none};
use crate::app::App;
use crate::app::plugins::Spot;
use crate::control::contract::{Code, Refusal};
use crate::control::plugins::ActionRan;
use crate::control::{Done, Item, PluginActions, PluginRun};
use crate::plugins::manifest::{Action, Placement};
use crate::plugins::{Context, Plugin};
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
        let spot = self.plugin_run_spot(request.item, &plugin, &action)?;
        if action.placement == Placement::Popup {
            self.popup_allowed()?;
        }
        let key = self.clients.requests.key();
        if !self.start_plugin_action(spot, plugin, action, Some(key)) {
            let what = request.item.map_or_else(|| "what the window shows".to_string(), |item| item.to_string());
            return Err(not_found(format!("{what} is gone")));
        }
        let pending =
            Pending { client: Some(client), key, timeout: None, done: Done::default(), stage: Stage::Reading };
        self.clients.requests.pending.push(pending);
        Ok(None)
    }

    fn popup_allowed(&self) -> Result<(), Refusal> {
        if !self.has_window() {
            return Err("no window is attached".into());
        }
        self.popup_blocked().map_or(Ok(()), |blocked| Err(conflict(blocked)))
    }

    pub(super) fn plugin_popup_close_request(&mut self) -> Handled {
        if !self.close_plugin_popup() {
            return Err(not_found("no plugin popup is open".into()));
        }
        Ok(Some(json(&Done::default())))
    }

    fn plugin_run_spot(&self, item: Option<Item>, plugin: &Plugin, action: &Action) -> Result<Spot, Refusal> {
        let spot = match item {
            Some(item) => self.plugin_spot(item)?,
            None if action.contexts.contains(&Context::Button) => return Ok(Spot::Shown),
            None => {
                return Err(malformed(format!(
                    "say what to run `{}` of `{}` on: a project, workspace, tab or pane; only a button action runs \
                     without one",
                    action.id, plugin.entry.id
                )));
            }
        };
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
        Ok(spot)
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
                let at = self.projects.workspace_index(id).ok_or_else(|| none("workspace", id))?;
                Spot::Workspace(self.projects[at.project()].id, id)
            }
            Item::Tab(id) => {
                let at = self.projects.tab_index(id).ok_or_else(|| none("tab", id))?;
                Spot::Tab(self.projects[at.project()].id, self.projects.workspace(at.workspace()).id, id)
            }
            Item::Pane(id) => {
                self.projects.tab_of_pane(id).ok_or_else(|| none("pane", id))?;
                Spot::Pane(id)
            }
        };
        let workspace = match spot {
            Spot::Workspace(_, w) | Spot::Tab(_, w, _) => w,
            Spot::Pane(pane) => {
                let at = self.projects.tab_of_pane(pane).ok_or_else(|| none("pane", pane))?;
                self.projects.workspace(at.workspace()).id
            }
            Spot::Project(_) | Spot::Shown => return Ok(spot),
        };
        if self.projects[self.usable_workspace(workspace)?.project()].closing {
            return Err(format!("workspace {workspace} is closing").into());
        }
        Ok(spot)
    }
}
