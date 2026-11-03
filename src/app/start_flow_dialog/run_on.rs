use std::collections::BTreeMap;

use super::flow_accounts;
use crate::app::App;
use crate::app::issues_dialog::run_on::lacking;
use crate::app::machines::checks::Checked;
use crate::app::overlay::Overlay;
use crate::control::FlowRun;
use crate::flows::start::{Start, There};
use crate::flows::store::Listing;
use crate::log;
use crate::machines::run_on::{self, RunOn, Target};
use crate::machines::send::Send;
use crate::project::Hierarchy;

pub(in crate::app) struct Run {
    pub(in crate::app) name: String,
    pub(in crate::app) task: String,
    pub(in crate::app) branch: String,
    pub(in crate::app) workspace: Option<String>,
    pub(in crate::app) accounts: BTreeMap<String, String>,
}

impl App {
    pub(in crate::app) fn start_flow_run_on_chosen(&mut self) {
        let Some(Overlay::StartFlow { start, .. }) = &self.overlay else { return };
        if start.machine().is_some() {
            self.start_flow_on_machine();
        } else {
            self.start_flow_back_here(start.project);
        }
    }

    pub(in crate::app) fn start_flow_on_machine(&mut self) {
        let Some(Overlay::StartFlow { start, .. }) = &mut self.overlay else { return };
        let Some(target) = start.machine().cloned() else { return };
        start.set_listing(Listing::default(), None);
        start.accounts = Vec::new();
        let project = start.project;
        let there = self.project_there(target.link, project).map(|there| there.id);
        self.recheck_machine(&target.name, there);
        self.follow_start_flow_run_on();
    }

    fn start_flow_back_here(&mut self, project: u64) {
        let Some(at) = self.projects.project_index(project) else { return };
        let listing = self.flow_listing(Some(&self.projects[at].path));
        let accounts = self.account_choices().map(|choices| flow_accounts(&choices)).unwrap_or_default();
        let Some(Overlay::StartFlow { start, .. }) = &mut self.overlay else { return };
        start.set_listing(listing, None);
        start.accounts = accounts;
    }

    pub(in crate::app) fn follow_start_flow_run_on(&mut self) {
        let Some(Overlay::StartFlow { start, .. }) = &self.overlay else { return };
        let run_on = RunOn::kept(self.run_on_for(start.project), start.run_on.as_ref());
        let target = run_on.as_ref().and_then(RunOn::machine).cloned();
        let read = target.map(|target| self.read_there(start, &target));
        let Some(Overlay::StartFlow { start, .. }) = &mut self.overlay else { return };
        start.run_on = run_on;
        let Some(Read { there, listing, accounts }) = read else { return };
        start.there = Some(there);
        if let Some(listing) = listing {
            start.set_listing(listing, None);
        }
        if let Some(accounts) = accounts {
            start.read_accounts_there(accounts);
        }
    }

    fn read_there(&self, start: &Start, target: &Target) -> Read {
        let era = self.held_by(target.link).map_or(0, |held| held.era);
        let was = start.there().filter(|was| was.era == era).cloned().unwrap_or_default();
        let id = self.project_there(target.link, start.project).map(|there| there.id);
        let listed = was.listed && was.project == id;
        let (listing, unlisted) = taken(id.filter(|_| !listed).map(|id| self.flows_there(&target.name, id)));
        let (accounts, unaccounted) = taken((!was.accounted).then(|| self.flow_accounts_there(&target.name)));
        let tracker = start
            .asked
            .as_ref()
            .zip(id)
            .and_then(|((_, source), id)| lacking(self.tracker_there(&target.name, id, source)));
        let there = There {
            place: self.where_there(target.link, start.project, true),
            cloned: id.is_some(),
            listed: listed || listing.is_some(),
            accounted: was.accounted || accounts.is_some(),
            lacking: tracker.or(unlisted).or(unaccounted),
            project: id,
            era,
        };
        Read { there, listing, accounts }
    }

    pub(in crate::app) fn start_flow_clone(&mut self) {
        let Some(Overlay::StartFlow { start, .. }) = &self.overlay else { return };
        let Some(target) = start.machine().cloned() else { return };
        let project = start.project;
        self.open_clone_on_machine(&target, project);
    }

    pub(in crate::app) fn start_flow_there(&mut self, run: Run) {
        let Some(Overlay::StartFlow { start, .. }) = &self.overlay else { return };
        let Some(target) = start.machine().cloned() else { return };
        if self.still_cloning(target.link, start.project) {
            return;
        }
        let Some(id) = self.project_there(target.link, start.project).map(|there| there.id) else {
            log::info!("flows", "project not on the machine", name = target.name);
            if let Some(Overlay::StartFlow { start, .. }) = &mut self.overlay {
                start.notice = Some(run_on::not_there(&target.name));
            }
            return;
        };
        let (issue, source) = start.asked.clone().unzip();
        let prompt = if issue.is_some() { String::new() } else { run.task };
        log::info!("flows", "start on a machine", name = target.name, project = id);
        let flow = FlowRun {
            name: run.name,
            prompt,
            worktree: Some(run.branch),
            project: Some(id),
            issue,
            source,
            workspace_name: run.workspace,
            accounts: run.accounts,
            ..FlowRun::default()
        };
        let sent = self.send_from_dialog(&target, Send::FlowRun(flow));
        let Some(Overlay::StartFlow { start, .. }) = &mut self.overlay else { return };
        match sent {
            Ok(()) => start.waiting = true,
            Err(why) => start.notice = Some(why),
        }
    }
}

struct Read {
    there: There,
    listing: Option<Listing>,
    accounts: Option<Vec<crate::accounts::AgentAccounts>>,
}

fn taken<T>(checked: Option<Checked<T>>) -> (Option<T>, Option<String>) {
    match checked {
        Some(Checked::Read(read)) => (Some(read), None),
        Some(Checked::Failed(why)) => (None, Some(why)),
        Some(Checked::Waiting) | None => (None, None),
    }
}
