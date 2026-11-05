use std::collections::HashMap;
use std::time::SystemTime;

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::relay::Then;
use crate::accounts::login::State;
use crate::agents::{self, Known};
use crate::app::App;
use crate::app::issues_dialog::run_on::issue_project;
use crate::app::machine_usage::{Cached, UNREADABLE};
use crate::app::overlay::Overlay;
use crate::app::start_flow_dialog::flow_accounts;
use crate::config::SYSTEM_ACCOUNT;
use crate::control::accounts::{AccountsInfo, ProviderInfo};
use crate::control::contract::Refusal;
use crate::control::issues::{SourceState, Sources};
use crate::control::{FlowInfo, Flows};
use crate::flows::store::{Listing, Origin, Saved};
use crate::issues::browser::{Accounts, AgentAccounts};
use crate::machines::run_on::{capped, clipped_or, has_text, reason};
use crate::machines::send::Send;

const UNNAMED: &str = "the project";
const SETTINGS: &str = "settings";
const FAILING: &str = "failing";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Check {
    Accounts,
    Sources(u64),
    Flows(u64),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::app) struct Wanted {
    pub(in crate::app) accounts: bool,
    pub(in crate::app) sources: bool,
    pub(in crate::app) flows: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) struct Checking {
    link: u64,
    era: u64,
    generation: u64,
    check: Check,
}

#[derive(Default)]
pub(in crate::app) struct Checks {
    generation: u64,
    accounts: Cached<AccountsInfo>,
    sources: HashMap<u64, Cached<Sources>>,
    flows: HashMap<u64, Cached<Flows>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) enum Checked<T> {
    Waiting,
    Read(T),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum AgentThere {
    Ready,
    NotInstalled,
    SignedOut,
    Unknown,
}

trait Slot {
    fn wanted(&self, again: bool) -> bool;
    fn ask(&mut self);
    fn heard(&mut self, result: Result<Value, Refusal>, machine: &str);
}

impl<T: DeserializeOwned> Slot for Cached<T> {
    fn wanted(&self, again: bool) -> bool {
        !self.asking && self.read.is_none() && (again || self.error.is_none())
    }

    fn ask(&mut self) {
        self.asking = true;
        self.error = None;
    }

    fn heard(&mut self, result: Result<Value, Refusal>, machine: &str) {
        self.asking = false;
        let read = result
            .map_err(|refusal| reason(machine, &refusal.message))
            .and_then(|value| serde_json::from_value(value).map_err(|_| UNREADABLE.to_string()));
        match read {
            Ok(read) => {
                self.read = Some((read, SystemTime::now()));
                self.error = None;
            }
            Err(why) => self.error = Some(why),
        }
    }
}

impl Check {
    fn send(self) -> Send {
        match self {
            Self::Accounts => Send::AccountList,
            Self::Sources(project) => Send::IssueSources(project),
            Self::Flows(project) => Send::FlowList(project),
        }
    }
}

impl Checks {
    pub(super) fn relinked(&mut self) {
        self.generation += 1;
        unasked(&mut self.accounts);
        self.sources.values_mut().for_each(unasked);
        self.flows.values_mut().for_each(unasked);
    }

    fn slot(&mut self, check: Check) -> &mut dyn Slot {
        match check {
            Check::Accounts => &mut self.accounts,
            Check::Sources(project) => self.sources.entry(project).or_default(),
            Check::Flows(project) => self.flows.entry(project).or_default(),
        }
    }
}

fn unasked<T>(cached: &mut Cached<T>) {
    cached.asking = false;
    cached.error = None;
}

#[cfg(test)]
#[cfg(unix)]
impl Checks {
    pub(in crate::app) fn asking_accounts(&self) -> bool {
        self.accounts.asking
    }
}

impl App {
    pub(in crate::app) fn check_machine(&mut self, name: &str, project: Option<u64>, wanted: Wanted) {
        self.ask_checks(name, project, wanted, true);
    }

    pub(in crate::app) fn check_shown(&mut self, again: bool) {
        let (target, project) = match &self.overlay {
            Some(Overlay::Issues(b)) => (b.machine().cloned(), issue_project(b)),
            Some(Overlay::StartFlow { start, .. }) => (start.machine().cloned(), start.project),
            _ => return,
        };
        let Some(target) = target else { return };
        let there = self.project_there(target.link, project).map(|there| there.id);
        let wanted = self.wanted_checks();
        self.ask_checks(&target.name, there, wanted, again);
    }

    fn ask_checks(&mut self, name: &str, project: Option<u64>, wanted: Wanted, again: bool) {
        let sources = project.filter(|_| wanted.sources).map(Check::Sources);
        let flows = project.filter(|_| wanted.flows).map(Check::Flows);
        let accounts = wanted.accounts.then_some(Check::Accounts);
        for check in [accounts, sources, flows].into_iter().flatten() {
            self.ask_check(name, check, again);
        }
    }

    pub(in crate::app) fn recheck_machine(&mut self, name: &str, project: Option<u64>) {
        if let Some(held) = self.machines.named(name) {
            held.checks = Checks { generation: held.checks.generation + 1, ..Checks::default() };
        }
        self.check_cloned(name, project);
    }

    pub(in crate::app) fn check_cloned(&mut self, name: &str, project: Option<u64>) {
        let wanted = self.wanted_checks();
        self.check_machine(name, project, wanted);
    }

    fn wanted_checks(&self) -> Wanted {
        match &self.overlay {
            Some(Overlay::Issues(_)) => Wanted { accounts: true, sources: true, flows: false },
            Some(Overlay::StartFlow { start, .. }) => {
                Wanted { accounts: true, sources: start.asked.is_some(), flows: true }
            }
            _ => Wanted::default(),
        }
    }

    fn ask_check(&mut self, name: &str, check: Check, again: bool) {
        let Some(held) = self.machines.named(name) else { return };
        if !held.checks.slot(check).wanted(again) {
            return;
        }
        let then =
            Then::Check(Checking { link: held.link.id(), era: held.era, generation: held.checks.generation, check });
        let sent = self.send_to_machine(name, check.send(), then);
        let Some(held) = self.machines.named(name) else { return };
        let slot = held.checks.slot(check);
        match sent {
            Ok(_) => slot.ask(),
            Err(refusal) => slot.heard(Err(refusal), name),
        }
    }

    pub(super) fn check_heard(&mut self, checking: Checking, result: Result<Value, Refusal>) {
        let Some(held) = self.machines.by_link(checking.link) else { return };
        if held.era == checking.era && held.checks.generation == checking.generation {
            held.checks.slot(checking.check).heard(result, &held.name);
        }
    }

    pub(in crate::app) fn accounts_there(&self, name: &str) -> Checked<Option<Accounts>> {
        checked(self.checks_of(name).map(|checks| &checks.accounts), accounts)
    }

    pub(in crate::app) fn flow_accounts_there(&self, name: &str) -> Checked<Vec<crate::accounts::AgentAccounts>> {
        let read = |info: &AccountsInfo| accounts(info).map(|choices| flow_accounts(&choices)).unwrap_or_default();
        checked(self.checks_of(name).map(|checks| &checks.accounts), read)
    }

    pub(in crate::app) fn agent_there(&self, name: &str, agent: &str, account: &str) -> AgentThere {
        let cached = self.checks_of(name).map(|checks| &checks.accounts);
        match checked(cached, |info| agent_state(info, agent, account)) {
            Checked::Read(state) => state,
            Checked::Waiting | Checked::Failed(_) => AgentThere::Unknown,
        }
    }

    pub(in crate::app) fn tracker_there(&self, name: &str, project: u64, source: &str) -> Checked<Option<String>> {
        let Some(held) = self.machines.held().iter().find(|held| held.name.eq_ignore_ascii_case(name)) else {
            return Checked::Waiting;
        };
        let projects = held.seen.iter().flat_map(|seen| &seen.projects);
        let named = projects.filter(|there| there.id == project).map(|there| there.name.as_str()).next();
        let there = Tracked { machine: &held.name, project: named.unwrap_or(UNNAMED), source };
        checked(held.checks.sources.get(&project), |sources| there.lacks(sources))
    }

    pub(in crate::app) fn flows_there(&self, name: &str, project: u64) -> Checked<Listing> {
        checked(self.checks_of(name).and_then(|checks| checks.flows.get(&project)), listing)
    }

    pub(in crate::app) fn login_there(&self, name: &str, agent: &str) -> Option<String> {
        let (info, _) = self.checks_of(name)?.accounts.read.as_ref()?;
        let found = info.agents.iter().find(|found| found.agent == agent && found.installed)?;
        let command = Some(found.command.as_str()).filter(|command| has_text(command)).unwrap_or(agent);
        let login = Known::of(agent).and_then(Known::account).map_or(&[][..], |home| home.login);
        let line = std::iter::once(command).chain(login.iter().copied()).map(agents::quote);
        Some(line.collect::<Vec<_>>().join(" "))
    }

    pub(in crate::app) fn command_there(&self, name: &str, agent: &str) -> Option<String> {
        let (info, _) = self.checks_of(name)?.accounts.read.as_ref()?;
        let found = info.agents.iter().find(|found| found.agent == agent && found.installed)?;
        Some(found.command.clone()).filter(|command| has_text(command))
    }

    fn checks_of(&self, name: &str) -> Option<&Checks> {
        let held = self.machines.held().iter().find(|held| held.name.eq_ignore_ascii_case(name));
        held.map(|held| &held.checks)
    }
}

fn checked<T, U>(cached: Option<&Cached<T>>, read: impl FnOnce(&T) -> U) -> Checked<U> {
    match cached {
        Some(Cached { read: Some((value, _)), .. }) => Checked::Read(read(value)),
        Some(Cached { error: Some(why), asking: false, .. }) => Checked::Failed(why.clone()),
        _ => Checked::Waiting,
    }
}

fn accounts(info: &AccountsInfo) -> Option<Accounts> {
    let listed =
        info.providers.iter().filter(|provider| provider.accounts.iter().any(|account| has_text(&account.name)));
    let agents: HashMap<String, AgentAccounts> =
        listed.map(|provider| (provider.provider.clone(), agent_accounts(provider))).collect();
    (!agents.is_empty()).then_some(Accounts { agents, chosen: None })
}

fn agent_accounts(provider: &ProviderInfo) -> AgentAccounts {
    let kept: Vec<_> = provider.accounts.iter().filter(|account| has_text(&account.name)).collect();
    let names: Vec<String> = kept.iter().map(|account| account.name.clone()).collect();
    let labels = std::iter::once(label(&provider.system_label, ""))
        .chain(kept.iter().map(|account| label(&account.label, &account.name)))
        .collect();
    let active = Some(&provider.active).filter(|active| names.contains(active)).cloned().unwrap_or_default();
    AgentAccounts { names, labels, active }
}

fn label(saved: &str, name: &str) -> String {
    [saved, name].into_iter().find(|text| has_text(text)).unwrap_or(SYSTEM_ACCOUNT).to_string()
}

fn agent_state(info: &AccountsInfo, agent: &str, account: &str) -> AgentThere {
    let Some(found) = info.agents.iter().find(|found| found.agent == agent) else { return AgentThere::Unknown };
    match (found.installed, signed_out(info, agent, account)) {
        (false, _) => AgentThere::NotInstalled,
        (true, true) => AgentThere::SignedOut,
        (true, false) => AgentThere::Ready,
    }
}

fn signed_out(info: &AccountsInfo, agent: &str, account: &str) -> bool {
    let Some(provider) = info.providers.iter().find(|provider| provider.provider == agent) else { return false };
    let login = match account {
        "" => provider.system_login.as_ref(),
        name => provider.accounts.iter().find(|there| there.name == name).and_then(|there| there.login.as_ref()),
    };
    let tokens = Known::of(agent).and_then(Known::account).map_or(&[][..], |home| home.tokens);
    let token = tokens.iter().any(|token| info.tokens.iter().any(|set| set == token));
    login.is_some_and(|login| login.state == State::SignedOut) && !token
}

struct Tracked<'a> {
    machine: &'a str,
    project: &'a str,
    source: &'a str,
}

impl Tracked<'_> {
    fn lacks(&self, sources: &Sources) -> Option<String> {
        let Self { machine, project, source } = self;
        let project = &clipped_or(project, UNNAMED);
        let Some(found) = sources.sources.iter().find(|found| found.source == *source) else {
            return Some(format!("{machine} cannot read {source} issues"));
        };
        let name = &clipped_or(&found.name, source);
        match found.state {
            SourceState::Connected => None,
            SourceState::Missing if found.missing.iter().any(|missing| missing == "token") => {
                Some(format!("{machine} has no {name} token for {project}"))
            }
            SourceState::Missing => {
                let missing = capped(found.missing.iter()).unwrap_or_else(|| SETTINGS.into());
                Some(format!("{machine} lacks {missing} for {name} in {project}"))
            }
            SourceState::Failing => Some(format!(
                "{name} on {machine}: {}",
                clipped_or(found.reason.as_deref().unwrap_or_default(), FAILING)
            )),
            SourceState::Other => Some(format!("{machine} cannot tell whether {name} works for {project}")),
        }
    }
}

fn listing(flows: &Flows) -> Listing {
    let saved = |info: &FlowInfo| Saved { flow: info.flow.clone().into_flow(), origin: origin(info) };
    Listing {
        flows: flows.flows.iter().map(saved).collect(),
        hidden: flows.hidden.iter().map(|info| info.flow.clone().into_flow()).collect(),
        broken: flows.broken.iter().map(|broken| (broken.path.clone(), broken.error.clone())).collect(),
        builtins: flows.builtins.clone(),
        default: flows.flows.iter().find(|info| info.default).map(|info| info.name.clone()),
    }
}

fn origin(info: &FlowInfo) -> Origin {
    let path = info.path.clone().unwrap_or_default();
    match info.origin.as_str() {
        "user" => Origin::User(path),
        "project" => Origin::Project(path),
        _ => Origin::BuiltIn,
    }
}

#[cfg(test)]
mod tests;
