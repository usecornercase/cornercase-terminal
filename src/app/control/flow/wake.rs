use std::time::Instant;

use serde_json::Value;

use crate::agents::activity::Status;
use crate::app::App;
use crate::app::agent_watch::Notice;
use crate::app::control::wait::turn_ended;
use crate::app::flow_runs::Wake;
use crate::control::{self, Command, Request};
use crate::flows::ORCHESTRATOR;
use crate::flows::run::Seen;
use crate::flows::{brief, run::Run};
use crate::log;
use crate::project::Hierarchy;

const CLIENTS: u64 = 1 << 59;

fn over(ended: Option<&'static str>) -> bool {
    ended.is_some_and(|ended| ended != Status::Waiting.name())
}

impl App {
    pub(in crate::app) fn follow_sent_work(&mut self, now: Instant) {
        if self.agents.watched() != Some(now) {
            return;
        }
        self.settle_sent_work();
        self.wake_orchestrators(now);
    }

    fn seen(&self, run: &Run, pane: u64, since: Instant) -> Seen {
        let Some(term) = self.projects.pane_by(pane) else { return Seen { gone: true, ..Seen::default() } };
        let own = run.pane_of(ORCHESTRATOR);
        Seen {
            gone: false,
            typed: term.typing.at,
            worked: term.agent.worked_since(since),
            ended: turn_ended(&term.agent),
            covered: own.is_some_and(|own| self.waited_on(own, pane)),
        }
    }

    fn settle_sent_work(&mut self) {
        let seen: Vec<(u64, u64, Seen)> = self
            .runs
            .all()
            .iter()
            .flat_map(|run| {
                run.sent
                    .iter()
                    .filter(|sent| !sent.finished)
                    .map(|sent| (run.id, sent.pane, self.seen(run, sent.pane, sent.at)))
            })
            .collect();
        for (id, pane, seen) in seen {
            if let Some(run) = self.runs.run_mut(id) {
                run.settle(pane, seen);
            }
        }
    }

    fn wake_orchestrators(&mut self, now: Instant) {
        let due: Vec<u64> = self
            .runs
            .all()
            .iter()
            .filter(|run| !run.due().is_empty() && !self.runs.waking(run.id))
            .map(|run| run.id)
            .collect();
        for id in due {
            self.wake(id, now);
        }
    }

    fn wake(&mut self, id: u64, now: Instant) {
        let Some(run) = self.runs.all().iter().find(|run| run.id == id) else { return };
        let workspace = run.workspace;
        let own = run.pane_of(ORCHESTRATOR);
        match own.and_then(|own| self.projects.pane_by(own)) {
            None => {
                let roles = self.take_due_roles(id);
                self.tell_person(workspace, &roles);
            }
            Some(term) if over(turn_ended(&term.agent)) => {
                let Some(own) = own else { return };
                self.send_wake(id, own, now);
            }
            Some(_) => {}
        }
    }

    fn take_due_roles(&mut self, id: u64) -> Vec<(String, u64)> {
        let Some(run) = self.runs.run_mut(id) else { return Vec::new() };
        let due = run.take_due();
        due.into_iter().filter_map(|pane| Some((run.role_of(pane)?.to_string(), pane))).collect()
    }

    fn send_wake(&mut self, id: u64, own: u64, now: Instant) {
        let roles = self.take_due_roles(id);
        let Some(workspace) = self.runs.all().iter().find(|run| run.id == id).map(|run| run.workspace) else { return };
        let client = CLIENTS + self.runs.next_wake();
        self.runs.start_wake(Wake { client, run: id, workspace, roles: roles.clone() });
        let send = control::SendText {
            pane: Some(own),
            text: Some(brief::finished(&roles)),
            enter: true,
            ..control::SendText::default()
        };
        let request = Request {
            caller: None,
            server: None,
            contract: None,
            boot: None,
            view: None,
            command: Command::Send(send),
        };
        log::info!("flow", "orchestrator woken", workspace = workspace, roles = roles.len());
        self.run_request(client, Ok(request), None, now);
    }

    pub(in crate::app) fn wake_answered(&mut self, client: u64, reply: &Result<Value, String>) -> bool {
        if !(CLIENTS..CLIENTS * 2).contains(&client) {
            return false;
        }
        if let Some(wake) = self.runs.woke(client) {
            match reply {
                Ok(_) => log::debug!("flow", "orchestrator wake sent", workspace = wake.workspace),
                Err(reason) => {
                    log::info!("flow", "orchestrator wake refused", workspace = wake.workspace, reason = reason);
                    self.tell_person(wake.workspace, &wake.roles);
                }
            }
        }
        true
    }

    fn tell_person(&mut self, workspace: u64, roles: &[(String, u64)]) {
        let Some(at) = self.projects.workspace_index(workspace) else { return };
        let project = self.projects[at.project()].id;
        for (role, pane) in roles {
            self.notify(Notice {
                agent: role,
                status: Status::Done,
                at: (project, workspace),
                panes: vec![*pane],
                principal: true,
            });
        }
    }
}
