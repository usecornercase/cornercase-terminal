use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::App;
use super::event::AppEvent;
use super::session::RESUME_GRACE;
use super::toast::Toast;
use crate::accounts::{self, Accounts, transfer};
use crate::agents::activity::Status;
use crate::agents::launch::Launch;
use crate::agents::{self, AgentState, Known};
use crate::error::Result;
use crate::log::{self, Job, Level};
use crate::project::Hierarchy;
use crate::term::Term;
use crate::ui;

mod pending;
mod shell;

pub(super) use pending::Restarts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Restart {
    Now,
    Waiting,
}

#[derive(Debug, Clone)]
pub(super) struct Waiting {
    kind: Known,
    to: String,
    generation: u32,
}

#[derive(Debug, Default)]
pub(super) struct Moving {
    notice: String,
    to: String,
    job: u64,
    requests: Vec<u64>,
}

#[derive(Debug)]
pub(crate) struct Restarted {
    plan: Plan,
    moved: Result<Option<transfer::Tally>>,
    job: Option<u64>,
}

#[derive(Debug)]
struct Plan {
    pane: u64,
    request: Option<u64>,
    generation: u32,
    agent: Option<(i32, std::time::SystemTime)>,
    kind: Known,
    from: String,
    to: String,
    cwd: PathBuf,
    line: String,
    resume: Option<AgentState>,
    marks: (bool, bool),
}

impl App {
    pub(super) fn panes_on_other_accounts(&self, kind: Known, to: &str) -> (Vec<u64>, Vec<(u64, String)>) {
        let (mut other, mut skipped) = (Vec::new(), Vec::new());
        for term in self.projects.terms() {
            let Some((_, name)) = term.agent_account().filter(|(k, _)| *k == kind) else { continue };
            if name.unwrap_or_default() == to && self.restart_waiting(term.id).is_none() {
                continue;
            }
            match self.not_restartable(term.id, name) {
                Some(why) => skipped.push((term.id, why)),
                None => other.push(term.id),
            }
        }
        (other, skipped)
    }

    pub(super) fn not_restartable(&self, pane: u64, account: Option<&str>) -> Option<String> {
        if account == Some(accounts::CUSTOM) {
            return Some("its account was not set by cornercase".to_string());
        }
        self.seat(pane).map(|(_, role)| format!("it is the {role} of a flow run"))
    }

    pub(super) fn restart_pane(&mut self, pane: u64, kind: Known, to: &str) -> std::result::Result<(), String> {
        self.restart_for(pane, kind, to, None)
    }

    fn restart_for(
        &mut self,
        pane: u64,
        kind: Known,
        to: &str,
        request: Option<u64>,
    ) -> std::result::Result<(), String> {
        let plan = Plan { request, ..self.plan_restart(pane, kind, to)? };
        let home = self.restart_home(kind, to)?;
        let files = plan.resume.as_ref().and_then(|_| self.projects.pane_by(pane)?.context.conversation_files());
        let Some(files) = files else {
            let plan = Plan { resume: None, ..plan };
            self.restarted(Restarted { plan, moved: Ok(None), job: None });
            return Ok(());
        };
        let label = self.config.account_label(kind.name(), to);
        let notice = format!("restarting {} on {label}", kind.name());
        let number = self.agents.restarts.start_move(pane, notice, to);
        let job = Job::new(Level::Info, "accounts", "conversation").with("pane", pane).with("agent", kind.name());
        self.spawn_job(
            job,
            move || transfer::conversation(&home, &files, &transfer::Disk).map(Some),
            move |moved, _| Some(AppEvent::Restarted(Box::new(Restarted { plan, moved, job: Some(number) }))),
        );
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn ask_restart(&mut self, pane: u64, kind: Known, to: &str) -> std::result::Result<Restart, String> {
        self.ask_restart_for(pane, kind, to, None)
    }

    pub(super) fn ask_restart_for(
        &mut self,
        pane: u64,
        kind: Known,
        to: &str,
        request: Option<u64>,
    ) -> std::result::Result<Restart, String> {
        if let Some(moving) = self.agents.restarts.moving_to(pane) {
            if moving != to {
                let label = self.config.account_label(kind.name(), moving);
                return Err(format!("a restart to {label} is already moving its conversation"));
            }
            self.agents.restarts.join_move(pane, request);
            return Ok(Restart::Now);
        }
        if self.restart_waiting(pane).is_some() && self.runs_on(pane, kind, to) {
            self.cancel_restart(pane);
            return Err(format!("pane {pane} already uses that account, so its waiting restart was cancelled"));
        }
        let plan = self.plan_restart(pane, kind, to)?;
        if !self.projects.pane_by(pane).is_some_and(|term| self.busy(term, Instant::now())) {
            self.agents.restarts.stop_waiting(pane);
            self.restart_for(pane, kind, to, request)?;
            return Ok(Restart::Now);
        }
        let waiting = Waiting { kind, to: to.to_string(), generation: plan.generation };
        self.agents.restarts.wait(pane, waiting);
        log::info!("accounts", "a restart waits for the turn", pane = pane, provider = kind.name(), to = shown(to));
        Ok(Restart::Waiting)
    }

    fn runs_on(&self, pane: u64, kind: Known, to: &str) -> bool {
        let account = self.projects.pane_by(pane).and_then(Term::agent_account);
        account.is_some_and(|(k, name)| k == kind && name.unwrap_or_default() == to)
    }

    pub(super) fn restart_waiting(&self, pane: u64) -> Option<(Known, &str)> {
        self.agents.restarts.waiting(pane).map(|waiting| (waiting.kind, waiting.to.as_str()))
    }

    pub(super) fn moving_notice(&self, pane: u64) -> Option<&str> {
        self.agents.restarts.moving_notice(pane)
    }

    pub(super) fn cancel_move(&mut self, pane: u64) -> bool {
        let Some(requests) = self.agents.restarts.cancel_move(pane) else { return false };
        log::info!("accounts", "a restart was cancelled while its conversation moved", pane = pane);
        for key in requests {
            self.account_restarted(key, pane, Err(MOVE_CANCELLED.to_string()));
        }
        true
    }

    pub(super) fn waiting_notice(&self, pane: u64) -> Option<String> {
        let (kind, to) = self.restart_waiting(pane)?;
        Some(format!("restarts on {} after this turn", self.config.account_label(kind.name(), to)))
    }

    pub(super) fn forget_restarts_to(&mut self, kind: Known, account: &str) {
        self.agents.restarts.forget_to(kind, account);
    }

    pub(super) fn cancel_restart(&mut self, pane: u64) {
        if self.agents.restarts.stop_waiting(pane) {
            log::info!("accounts", "a waiting restart was cancelled", pane = pane);
        }
    }

    pub(super) fn drive_restarts(&mut self, now: Instant) {
        if self.agents.watched() != Some(now) {
            return;
        }
        for (pane, Waiting { kind, to, generation }) in self.agents.restarts.all_waiting() {
            let term = self.projects.pane_by(pane).filter(|term| term.generation() == generation);
            let account = term.and_then(Term::agent_account).filter(|(k, _)| *k == kind).map(|(_, name)| name);
            let gone = match account {
                None => Some("the pane or its agent is gone"),
                Some(name) if name.unwrap_or_default() == to => Some("the pane already uses that account"),
                Some(_) if term.is_some_and(|term| self.busy(term, now)) => continue,
                Some(_) => None,
            };
            self.agents.restarts.stop_waiting(pane);
            if let Some(why) = gone {
                log::info!("accounts", "a waiting restart was dropped", pane = pane, why = why);
            } else if let Err(e) = self.restart_pane(pane, kind, &to) {
                log::warning!("accounts", "could not restart an agent", pane = pane, error = e);
                let message = format!("could not restart {} in pane {pane}: {e}", kind.name());
                self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Bug));
            }
        }
    }

    fn plan_restart(&self, pane: u64, kind: Known, to: &str) -> std::result::Result<Plan, String> {
        let term = self.projects.pane_by(pane).ok_or_else(|| format!("pane {pane} is gone"))?;
        let Some((_, name)) = term.agent_account().filter(|(k, _)| *k == kind) else {
            return Err(format!("pane {pane} does not run {}", kind.name()));
        };
        if name.unwrap_or_default() == to {
            return Err(format!("pane {pane} already uses that account"));
        }
        if let Some(why) = self.not_restartable(pane, name) {
            return Err(why);
        }
        let from = name.unwrap_or_default().to_string();
        let resume = term.resume.clone().filter(|state| state.kind == kind.name());
        let resume = resume
            .filter(|_| term.context.conversation_files().is_some())
            .map(|state| AgentState { account: (!to.is_empty()).then(|| to.to_string()), ..state });
        let line = resume.as_ref().and_then(|state| agents::resume_line(&self.config, state)).unwrap_or_else(|| {
            let name = kind.name();
            let mode = agents::mode_of(name, &term.foreground_args(), &agents::modes(&self.config, name));
            agents::fresh_line(&self.config, name, mode.as_deref())
        });
        let cwd = self.pane_folder(pane, term.context.folder().map(Path::to_path_buf).or_else(|| term.cwd()))?;
        let marks = (term.agent.unseen(), term.agent.pending());
        let (generation, agent) = (term.generation(), term.context.agent());
        Ok(Plan { pane, request: None, generation, agent, kind, from, to: to.to_string(), cwd, line, resume, marks })
    }

    pub(super) fn pane_folder(&self, pane: u64, preferred: Option<PathBuf>) -> std::result::Result<PathBuf, String> {
        let fallback = || self.projects.tab_with_pane(pane).map(|(path, _)| path.to_path_buf());
        preferred.filter(|dir| dir.is_dir()).or_else(fallback).ok_or_else(|| "the pane has no folder".to_string())
    }

    fn restart_home(&self, kind: Known, to: &str) -> std::result::Result<PathBuf, String> {
        if to.is_empty() {
            return self.accounts.default_home(kind).ok_or_else(|| format!("{} has no home folder here", kind.name()));
        }
        let chosen = BTreeMap::from([(kind.name().to_string(), to.to_string())]);
        let accounts = Accounts::of(self.accounts.dir(), &chosen);
        let account = accounts.get(kind).ok_or_else(|| format!("the {} account `{to}` is gone", kind.name()))?;
        Ok(account.home.clone())
    }

    pub(super) fn restarted(&mut self, restarted: Restarted) {
        let (pane, request) = (restarted.plan.pane, restarted.plan.request);
        let joined = match restarted.job.map(|job| self.agents.restarts.finish_move(pane, job)) {
            None => Vec::new(),
            Some(Some(requests)) => requests,
            Some(None) => {
                log::info!("accounts", "a restart was dropped: it was cancelled", pane = pane);
                if let Some(key) = request {
                    self.account_restarted(key, pane, Err(MOVE_CANCELLED.to_string()));
                }
                return;
            }
        };
        let outcome = self.finish_restart(restarted);
        for key in request.into_iter().chain(joined) {
            self.account_restarted(key, pane, outcome.clone());
        }
    }

    fn finish_restart(&mut self, Restarted { mut plan, moved, .. }: Restarted) -> std::result::Result<(), String> {
        let current = self.projects.pane_by(plan.pane).filter(|term| {
            term.generation() == plan.generation
                && term.context.agent() == plan.agent
                && term.agent_account().is_some_and(|(k, _)| k == plan.kind)
        });
        let Some(chosen) = current.map(|term| term.accounts().chosen()) else {
            log::info!("accounts", "a restart was dropped: the pane changed", pane = plan.pane);
            return Err("the pane changed before it could restart".to_string());
        };
        let conversation = match moved {
            Ok(tally) => conversation(tally.as_ref()),
            Err(e) => {
                let message = format!("could not restart {} in pane {}: {e}", plan.kind.name(), plan.pane);
                return Err(self.restart_failed(plan.pane, message, e.to_string()));
            }
        };
        let accounts = Accounts::of(self.accounts.dir(), &with_account(chosen, plan.kind, &plan.to));
        let label = self.config.account_label(plan.kind.name(), &plan.to).to_string();
        self.respawn_restarted(plan.pane, std::mem::take(&mut plan.cwd), accounts)?;
        let ended = format!("pane {} restarted its {} on {label}, so this request ended", plan.pane, plan.kind.name());
        self.fail_pane_requests(plan.pane, &ended);
        self.agents.launches.drop_panes(&[plan.pane]);
        self.resume_restarted(plan, &label, conversation);
        Ok(())
    }

    fn respawn_restarted(&mut self, pane: u64, cwd: PathBuf, accounts: Accounts) -> std::result::Result<(), String> {
        match self.respawn_in(pane, cwd, accounts) {
            Ok(true) => Ok(()),
            Ok(false) => Err("the pane is gone".to_string()),
            Err(e) => Err(self.restart_failed(pane, format!("could not restart pane {pane}: {e}"), e.to_string())),
        }
    }

    fn restart_failed(&mut self, pane: u64, message: String, error: String) -> String {
        log::warning!("accounts", "could not restart an agent", pane = pane, error = error);
        self.messages.toast = Some(Toast::new(message, ui::ToastIcon::Bug));
        error
    }

    fn resume_restarted(&mut self, plan: Plan, label: &str, conversation: &str) {
        let now = Instant::now();
        if let Some(term) = self.projects.pane_by_mut(plan.pane)
            && let Some(state) = plan.resume
        {
            term.resume = Some(state);
            term.resuming = Some(now + RESUME_GRACE);
            term.agent.restore(plan.marks.0, plan.marks.1);
        }
        let notice = format!("restarting {} on {label}", plan.kind.name());
        self.agents.launches.add(Launch::restart(plan.pane, plan.line, notice, now, now + RESUME_GRACE));
        log::info!(
            "accounts",
            "agent restarted",
            pane = plan.pane,
            provider = plan.kind.name(),
            from = shown(&plan.from),
            to = shown(&plan.to),
            conversation = conversation,
        );
    }

    fn busy(&self, term: &Term, now: Instant) -> bool {
        term.agent.status() == Some(Status::Waiting) || self.holds_restart(term, now) || self.person_holds(term.id, now)
    }
}

const MOVE_CANCELLED: &str = "the restart was cancelled in the window";

fn conversation(tally: Option<&transfer::Tally>) -> &'static str {
    match tally {
        None => "fresh",
        Some(tally) if tally.copied > 0 => "copied",
        Some(tally) if tally.linked > 0 => "linked",
        Some(_) => "moved",
    }
}

fn with_account(mut chosen: BTreeMap<String, String>, kind: Known, to: &str) -> BTreeMap<String, String> {
    if to.is_empty() {
        chosen.remove(kind.name());
    } else {
        chosen.insert(kind.name().to_string(), to.to_string());
    }
    chosen
}

fn shown(account: &str) -> &str {
    if account.is_empty() { "system" } else { account }
}
