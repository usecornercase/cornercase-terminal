use serde_json::Value;

use super::IssueStart;
use crate::app::App;
use crate::app::machines::checks::{AgentThere, Checked};
use crate::app::machines::relay::{Sent, Then};
use crate::app::overlay::Overlay;
use crate::control::contract::{Code, Refusal};
use crate::control::{self, NewTab, ProjectInfo};
use crate::flows::start::Start;
use crate::issues::Issue;
use crate::issues::browser::{Accounts, AgentNote, Browser, There};
use crate::log;
use crate::machines::run_on::{RunOn, Target};
use crate::machines::send::Send;

impl App {
    pub(super) fn run_on_chosen(&mut self) {
        let Some(Overlay::Issues(b)) = &self.overlay else { return };
        if let Some((name, link)) = b.machine().map(|target| (target.name.clone(), target.link)) {
            let there = self.project_there(link, issue_project(b)).map(|there| there.id);
            self.recheck_machine(&name, there);
        } else {
            let accounts = self.account_choices();
            if let Some(Overlay::Issues(b)) = &mut self.overlay {
                b.accounts = accounts;
            }
        }
        self.follow_issues_run_on();
    }

    pub(in crate::app) fn follow_issues_run_on(&mut self) {
        let Some(Overlay::Issues(b)) = &self.overlay else { return };
        let had = b.machine().is_some();
        let run_on = self.run_on_for(b.project).map(|fresh| kept(fresh, b.run_on.as_ref()));
        let target = run_on.as_ref().and_then(RunOn::machine);
        let accounts = target.map(|target| self.machine_accounts(&target.name, b.accounts.as_ref()));
        let there = target.and_then(|target| self.there_for(b, target, accounts.as_ref().and_then(Option::as_ref)));
        let accounts = match accounts {
            Some(accounts) => Some(accounts.filter(|_| self.config.account_picker)),
            None if had => Some(self.account_choices()),
            None => None,
        };
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return };
        b.run_on = run_on;
        b.there = there;
        if let Some(accounts) = accounts {
            b.accounts = accounts;
        }
    }

    fn machine_accounts(&self, name: &str, shown: Option<&Accounts>) -> Option<Accounts> {
        let Checked::Read(Some(mut accounts)) = self.accounts_there(name) else { return None };
        let chosen = shown.and_then(|shown| shown.chosen.clone());
        accounts.chosen = chosen.filter(|(kind, name)| {
            accounts.of(kind).is_some_and(|agent| name.is_empty() || agent.names.contains(name))
        });
        Some(accounts)
    }

    fn there_for(&self, b: &Browser, target: &Target, accounts: Option<&Accounts>) -> Option<There> {
        let issue = b.focused()?;
        let place = b.place_for(&issue);
        let project = place.map_or(b.project, |place| place.project);
        let worktree = place.map_or(b.worktrees, |place| place.worktree);
        let id = self.project_there(target.link, project).map(|there| there.id);
        let command = b.agents.pick().and_then(|kind| self.command_there(&target.name, &kind));
        let agent = b.agents.pick().map_or(AgentNote::Ready, |kind| {
            let account = accounts.and_then(|accounts| accounts.pick(&kind)).unwrap_or_default();
            note(self.agent_there(&target.name, &kind, account))
        });
        let tracker = id.and_then(|id| lacking(self.tracker_there(&target.name, id, issue.source.id())));
        let start_hint = self.where_there(target.link, project, worktree);
        Some(There { start_hint, cloned: id.is_some(), agent, command, tracker })
    }

    pub(super) fn clone_there(&mut self) {
        let Some(Overlay::Issues(b)) = &self.overlay else { return };
        let Some(target) = b.machine().cloned() else { return };
        let project = issue_project(b);
        self.open_clone_on_machine(&target, project);
    }

    pub(super) fn start_issue_there(&mut self, issue: &Issue, start: &IssueStart) -> bool {
        let Some(Overlay::Issues(b)) = &self.overlay else { return false };
        let Some(target) = b.machine().cloned() else { return false };
        let missing = b.there().and_then(|there| there.tracker.clone());
        let project = start.place.as_ref().map_or(b.project, |place| place.project);
        let sent = match (missing, self.project_there(target.link, project).map(|there| there.id)) {
            (Some(missing), _) => Err(missing),
            (None, None) => Err(format!("the project is not on {}", target.name)),
            (None, Some(id)) => {
                let start = self.start_there(issue, start, id);
                log::info!(
                    "issues",
                    "start on a machine",
                    source = issue.source.name(),
                    key = issue.key,
                    agent = start.agent.as_deref().unwrap_or_default(),
                    name = target.name,
                    project = id,
                );
                self.send_from_dialog(&target, Send::Start(start))
            }
        };
        self.dialog_sent(sent);
        true
    }

    fn start_there(&self, issue: &Issue, start: &IssueStart, project: u64) -> control::Start {
        let (worktree, workspace_name) =
            start.names.clone().map_or((None, None), |(branch, name)| (Some(branch), Some(name)));
        control::Start {
            agent: Some(start.agent.to_string()),
            project: Some(project),
            worktree,
            workspace_name,
            issue: Some(issue.key.clone()),
            source: Some(issue.source.id().to_string()),
            accounts: self.chosen_accounts(),
            ..control::Start::default()
        }
    }

    pub(super) fn open_login_there(&mut self) {
        let Some(Overlay::Issues(b)) = &self.overlay else { return };
        let (Some(target), Some(kind)) = (b.machine().cloned(), b.agents.pick()) else { return };
        let Some(there) = self.project_there(target.link, issue_project(b)) else { return };
        let tab = NewTab {
            workspace: root_workspace(there),
            name: Some(format!("{kind} login")),
            command: self.login_there(&target.name, &kind),
            accounts: self.chosen_accounts(),
            ..NewTab::default()
        };
        let sent = self.send_from_dialog(&target, Send::NewTab(tab));
        self.dialog_sent(sent);
    }

    pub(in crate::app) fn send_from_dialog(&mut self, target: &Target, send: Send) -> Result<(), String> {
        let key = self.clients.requests.key();
        let then = Then::Dialog { link: target.link, key };
        self.send_to_machine(&target.name, send, then).map_err(|refusal| refusal.message)?;
        if let Some(run_on) = self.dialog_run_on() {
            run_on.awaiting = Some(key);
        }
        Ok(())
    }

    fn dialog_run_on(&mut self) -> Option<&mut RunOn> {
        match &mut self.overlay {
            Some(Overlay::Issues(b)) => b.run_on.as_mut(),
            Some(Overlay::StartFlow { start, .. }) => start.run_on.as_mut(),
            Some(Overlay::NewWorkspace { run_on, .. }) => run_on.as_mut(),
            _ => None,
        }
    }

    fn dialog_sent(&mut self, sent: Result<(), String>) {
        let Some(Overlay::Issues(b)) = &mut self.overlay else { return };
        b.starting = sent.is_ok();
        b.error = sent.err();
    }

    pub(in crate::app) fn dialog_relayed(&mut self, link: u64, key: u64, result: Result<Value, Refusal>) {
        let Some(held) = self.machines.held().iter().find(|held| held.link.id() == link) else { return };
        let boot = held.seen.as_ref().map(|seen| seen.boot.clone());
        let sent = Sent { machine: held.name.clone(), link, boot };
        let error = match result {
            Ok(value) => {
                self.await_made(&sent, &value);
                None
            }
            Err(refusal) if refusal.code == Code::StaleBoot => {
                Some(format!("{} restarted; nothing was started there", sent.machine))
            }
            Err(refusal) => Some(refusal.message),
        };
        let Some(run_on) = self.dialog_run_on().filter(|run_on| run_on.awaiting == Some(key)) else { return };
        run_on.awaiting = None;
        let close = match &mut self.overlay {
            Some(Overlay::Issues(b)) => settled(&mut b.starting, &mut b.error, error),
            Some(Overlay::StartFlow { start, .. }) => settled(&mut start.waiting, &mut start.notice, error),
            Some(Overlay::NewWorkspace { creating, error: shown, .. }) => settled(creating, shown, error),
            _ => false,
        };
        if close {
            self.overlay = None;
        }
    }

    pub(super) fn on_the_chosen_machine(&self, mut start: Start, issue: &Issue) -> Start {
        start.asked = Some((issue.key.clone(), issue.source.id().to_string()));
        let Some(Overlay::Issues(b)) = &self.overlay else { return start };
        let Some(link) = b.machine().map(|target| target.link) else { return start };
        if let Some(run_on) = &mut start.run_on {
            run_on.chosen = run_on.targets.iter().position(|target| target.link == link);
            start.notice = None;
        }
        start
    }
}

fn settled(waiting: &mut bool, shown: &mut Option<String>, error: Option<String>) -> bool {
    if error.is_none() && *waiting {
        return true;
    }
    *waiting = false;
    *shown = error;
    false
}

fn root_workspace(there: &ProjectInfo) -> Option<u64> {
    let root = there.workspaces.iter().find(|workspace| !workspace.worktree);
    root.or_else(|| there.workspaces.first()).map(|workspace| workspace.id)
}

fn issue_project(b: &Browser) -> u64 {
    let place = b.focused().and_then(|issue| b.place_for(&issue).map(|place| place.project));
    place.unwrap_or(b.project)
}

pub(in crate::app) fn kept(mut fresh: RunOn, shown: Option<&RunOn>) -> RunOn {
    let chosen = shown.and_then(RunOn::machine).map(|target| target.link);
    fresh.chosen = chosen.and_then(|link| fresh.targets.iter().position(|target| target.link == link));
    fresh.awaiting = shown.and_then(|shown| shown.awaiting);
    fresh
}

fn note(agent: AgentThere) -> AgentNote {
    match agent {
        AgentThere::NotInstalled => AgentNote::NotInstalled,
        AgentThere::SignedOut => AgentNote::SignedOut,
        AgentThere::Ready | AgentThere::Unknown => AgentNote::Ready,
    }
}

pub(in crate::app) fn lacking(tracker: Checked<Option<String>>) -> Option<String> {
    match tracker {
        Checked::Read(missing) => missing,
        Checked::Failed(why) => Some(why),
        Checked::Waiting => None,
    }
}
