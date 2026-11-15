use crate::agents::activity::Status;
use crate::app::App;
use crate::app::closed_agents::reopen_hint;
use crate::app::notifications::{Message, MessageKind};
use crate::app::toast::{Toast, Undo};
use crate::flows::{ORCHESTRATOR, brief};
use crate::host::notify::{self, Notification};
use crate::log;
use crate::project::Hierarchy;
use crate::ui;

pub(super) fn exit_words(roles: &[String], place: &str) -> String {
    match roles {
        [one] => format!("{one}'s agent exited in {place}"),
        _ => format!("{} agents exited in {place}: {}", roles.len(), brief::names(roles)),
    }
}

struct Told {
    run: u64,
    workspace: u64,
    place: String,
    fresh: Vec<(u64, String)>,
    named: Vec<String>,
}

impl App {
    fn principal_role(&self, pane: u64, role: &str) -> bool {
        role == ORCHESTRATOR && self.projects.pane_by(pane).is_some_and(|term| term.parent.is_none())
    }

    pub(in crate::app) fn tell_exits(&mut self, run: u64, fresh: &[u64]) {
        let Some(found) = self.runs.get(run) else { return };
        let workspace = found.workspace;
        let Some(at) = self.projects.workspace_index(workspace).filter(|&at| self.workspace_open(at)) else {
            log::info!("flow", "an agent exited while its workspace closes", workspace = workspace);
            return;
        };
        let told = |pane: u64| {
            let role = found.role_of(pane)?;
            self.tells(self.principal_role(pane, role)).then(|| (pane, role.to_string()))
        };
        let fresh: Vec<(u64, String)> = fresh.iter().filter_map(|&pane| told(pane)).collect();
        let named: Vec<String> = found.exited.iter().filter_map(|&pane| told(pane)).map(|(_, role)| role).collect();
        if !fresh.is_empty() {
            let place = self.place_name(at);
            self.say_exits(&Told { run, workspace, place, fresh, named });
        }
    }

    fn say_exits(&mut self, told: &Told) {
        let text = exit_words(&told.named, &told.place);
        let message = format!("{text}{}", reopen_hint(told.named.len()));
        let mut toast = Toast::new(message, ui::ToastIcon::Agent(Status::Failed));
        toast.undo = Some(Undo::Agents(told.workspace));
        let unseen = self.unseen_exits(&told.fresh);
        if !unseen.is_empty() {
            let message = notify::clean(&toast.message);
            self.outbox.notify(Notification::new(&message, &self.config.desktop_notifications));
        }
        for (pane, role) in &told.fresh {
            let published = unseen.contains(pane);
            let (workspace, pane) = (told.workspace, *pane);
            log::info!(
                "flow",
                "a role's agent exited",
                workspace = workspace,
                role = role,
                pane = pane,
                told = published
            );
            if published {
                self.publish_exit(told, pane, role);
            }
        }
        self.messages.toast = Some(toast);
    }

    fn unseen_exits(&self, fresh: &[(u64, String)]) -> Vec<u64> {
        let seen = self.seen_tabs();
        let unseen = |pane: &u64| {
            let tab = self.projects.tab_of_pane(*pane).map(|at| self.projects.tab(at).id);
            tab.is_none_or(|tab| !seen.contains(&tab))
        };
        fresh.iter().map(|(pane, _)| *pane).filter(unseen).collect()
    }

    fn publish_exit(&mut self, told: &Told, pane: u64, role: &str) {
        self.publish_message(Message {
            kind: MessageKind::FlowExited,
            text: exit_words(&[role.to_string()], &told.place),
            workspace: Some(told.workspace),
            panes: vec![pane],
            run: Some(told.run),
            role: Some(role.to_string()),
        });
    }
}

#[cfg(test)]
mod tests;
