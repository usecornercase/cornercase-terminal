use std::time::Instant;

use super::super::reopen_hint;
use crate::agents::activity::Status;
use crate::app::App;
use crate::app::control::conflict;
use crate::app::notifications::{Message, MessageKind};
use crate::app::toast::{Toast, Undo};
use crate::control::contract::Refusal;
use crate::flows::run::Run;
use crate::flows::{ORCHESTRATOR, brief};
use crate::host::notify::{self, Notification};
use crate::log;
use crate::project::Hierarchy;
use crate::ui;

fn send_refused(pane: u64, role: &str, workspace: u64) -> String {
    format!(
        "pane {pane} is {role} in a flow's run, and its agent exited: what you send would be typed into its shell \
         instead; `cornercase agents reopen --workspace {workspace}` starts it again in that pane, then send again"
    )
}

fn exit_words(roles: &[String], place: &str) -> String {
    match roles {
        [one] => format!("{one}'s agent exited in {place}"),
        _ => format!("the agents of {} exited in {place}", brief::names(roles)),
    }
}

struct Told {
    run: u64,
    workspace: u64,
    place: String,
    seats: Vec<(u64, String)>,
}

impl App {
    pub(in crate::app) fn exited_role_refusal(&self, pane: u64) -> Option<Refusal> {
        let (run, role) = self.runs.seat(pane)?;
        self.role_exited(pane, Instant::now()).then(|| conflict(send_refused(pane, role, run.workspace)))
    }

    fn principal_role(&self, pane: u64, role: &str) -> bool {
        role == ORCHESTRATOR && self.projects.pane_by(pane).is_some_and(|term| term.parent.is_none())
    }

    pub(in crate::app) fn agents_back(&self, run: &Run, now: Instant) -> Vec<String> {
        let runs_again = |pane: u64| {
            self.projects.pane_by(pane).is_none_or(|term| {
                term.agent.status().is_some()
                    || term.resuming.is_some_and(|until| now < until)
                    || self.agents.launches.of_pane(pane).next().is_some()
            })
        };
        let back = |role: &&String| run.pane_of(role).is_none_or(runs_again);
        run.exits_told.iter().filter(back).cloned().collect()
    }

    pub(in crate::app) fn tell_exits(&mut self, run: u64, fresh: &[u64]) {
        let Some(found) = self.runs.get(run) else { return };
        let workspace = found.workspace;
        let seats: Vec<(u64, String)> =
            fresh.iter().filter_map(|&pane| Some((pane, found.role_of(pane)?.to_string()))).collect();
        let Some(at) = self.projects.workspace_index(workspace).filter(|&at| self.workspace_open(at)) else {
            log::info!("flow", "an agent exited while its workspace closes", workspace = workspace);
            return;
        };
        let seats: Vec<(u64, String)> =
            seats.into_iter().filter(|(pane, role)| self.tells(self.principal_role(*pane, role))).collect();
        if !seats.is_empty() {
            let place = self.place_name(at);
            self.say_exits(&Told { run, workspace, place, seats });
        }
    }

    fn say_exits(&mut self, told: &Told) {
        let roles: Vec<String> = told.seats.iter().map(|(_, role)| role.clone()).collect();
        let text = exit_words(&roles, &told.place);
        let mut toast = Toast::new(format!("{text}{}", reopen_hint(roles.len())), ui::ToastIcon::Agent(Status::Failed));
        toast.undo = Some(Undo::Agents(told.workspace));
        let seen = self.seen_tabs();
        let unseen: Vec<&(u64, String)> = told
            .seats
            .iter()
            .filter(|(pane, _)| {
                let tab = self.projects.tab_of_pane(*pane).map(|at| self.projects.tab(at).id);
                tab.is_none_or(|tab| !seen.contains(&tab))
            })
            .collect();
        if !unseen.is_empty() {
            let message = notify::clean(&toast.message);
            self.outbox.notify(Notification::new(&message, &self.config.desktop_notifications));
        }
        for (pane, role) in &told.seats {
            let published = unseen.iter().any(|(unseen, _)| unseen == pane);
            log::info!(
                "flow",
                "a role's agent exited",
                workspace = told.workspace,
                role = role,
                pane = pane,
                told = published
            );
            if published {
                let text = exit_words(std::slice::from_ref(role), &told.place);
                let said = Message {
                    kind: MessageKind::FlowExited,
                    text,
                    workspace: Some(told.workspace),
                    panes: vec![*pane],
                };
                self.publish_message_of(said, Some((told.run, role.clone())));
            }
        }
        self.messages.toast = Some(toast);
    }
}
