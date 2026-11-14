use std::collections::{HashMap, HashSet};

use super::Asker;
use crate::agents;
use crate::agents::headless::Access;
use crate::app::App;
use crate::bots::BOT_ENV;
use crate::config::Config;
use crate::control::bots::BotRequest;
use crate::control::contract::{Code, Refusal};
use crate::control::{Command, Configure, HeldAction};
use crate::log;
use crate::process;
use crate::project::Hierarchy;
use crate::term::Term;

pub(crate) const HEAD: &str = "a bot can only type into a pane that runs an agent";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Writes {
    Pane { pane: Option<u64>, tab: Option<u64> },
    Popup,
    NewShell,
    Access,
    Says(String),
    Settings,
}

#[derive(Default)]
pub(in crate::app) struct BotClients {
    clients: HashMap<u64, String>,
    keys: HashSet<u64>,
}

impl BotClients {
    fn is_bot(&self, client: u64) -> bool {
        self.clients.contains_key(&client)
    }

    fn insert(&mut self, client: u64, bot: String) {
        self.clients.insert(client, bot);
    }

    pub(in crate::app) fn forget(&mut self, client: u64, pending: impl Iterator<Item = u64>) {
        if self.clients.remove(&client).is_some() {
            self.keys.extend(pending);
        }
    }

    fn owns(&self, key: u64) -> bool {
        self.keys.contains(&key)
    }

    fn prune(&mut self, alive: &HashSet<u64>) {
        self.keys.retain(|key| alive.contains(key));
    }
}

pub(in crate::app) fn fronts_agent(config: &Config, term: &Term) -> bool {
    agents::detect(config, &term.foreground_args()).is_some()
}

fn blank(command: Option<&String>) -> bool {
    command.is_none_or(|command| command.trim().is_empty())
}

fn writes(command: &Command) -> Option<Writes> {
    match command {
        Command::Send(send) => Some(Writes::Pane { pane: send.pane, tab: send.tab }),
        Command::Keys(keys) => Some(Writes::Pane { pane: keys.pane, tab: keys.tab }),
        Command::TodoSend(send) => Some(Writes::Pane { pane: send.pane, tab: send.tab }),
        Command::Held(held) if held.action == HeldAction::SendNow => {
            Some(Writes::Pane { pane: held.pane, tab: held.tab })
        }
        Command::Input(input) if input.popup => Some(Writes::Popup),
        Command::Input(input) => input.pane.map(|pane| Writes::Pane { pane: Some(pane), tab: None }),
        Command::PasteFile(paste) => paste.pane.map(|pane| Writes::Pane { pane: Some(pane), tab: None }),
        Command::NewTab(new) if !blank(new.command.as_ref()) => Some(Writes::NewShell),
        Command::Split(split) if !blank(split.command.as_ref()) => Some(Writes::NewShell),
        _ => lifts(command),
    }
}

fn lifts(command: &Command) -> Option<Writes> {
    match command {
        Command::Bot(BotRequest::New(new)) => {
            new.access.as_deref().filter(|a| !a.eq_ignore_ascii_case("cornercase")).map(|_| Writes::Access)
        }
        Command::Bot(BotRequest::Edit(edit)) => edit.access.as_ref().map(|_| Writes::Access),
        Command::Bot(BotRequest::Say(say)) => Some(Writes::Says(say.bot.clone())),
        Command::Config(Configure::Get | Configure::Search(_)) => None,
        Command::Config(_) => Some(Writes::Settings),
        Command::ConfigImport(import) if import.apply => Some(Writes::Settings),
        _ => None,
    }
}

fn runs_none(pane: u64) -> Refusal {
    Refusal::new(
        Code::Failed,
        format!("{HEAD}, and pane {pane} runs none; `cornercase status --panes` lists the agents"),
    )
}

fn no_popup() -> Refusal {
    Refusal::new(Code::Failed, format!("{HEAD}, not into a plugin's popup"))
}

fn no_access() -> Refusal {
    Refusal::new(Code::Failed, "a bot cannot change a bot's access")
}

fn no_full_bot() -> Refusal {
    Refusal::new(Code::Failed, "a bot cannot write to a bot with full access")
}

fn no_settings() -> Refusal {
    Refusal::new(Code::Failed, "a bot cannot change cornercase's settings")
}

fn no_new_shell() -> Refusal {
    Refusal::new(
        Code::Failed,
        format!(
            "{HEAD}, and a new pane's command is typed into its shell; start an agent with `cornercase start` instead"
        ),
    )
}

impl App {
    pub(crate) fn client_connected(&mut self, client: u64, peer: Option<i32>) {
        if self.panels.bots.live.is_empty() {
            return;
        }
        let bot = match peer {
            Some(pid) => self.bot_of_process(pid),
            None => self.contained_bot(),
        };
        if let Some(bot) = bot {
            log::info!("control", "a bot's client", client = client, bot = bot);
            self.clients.bots.insert(client, bot);
        }
    }

    fn contained_bot(&self) -> Option<String> {
        let mut turns = self.panels.bots.live.iter();
        turns
            .find(|state| state.turn.as_ref().is_some_and(|turn| turn.running.contained()))
            .map(|state| state.bot.id.clone())
    }

    fn bot_of_process(&self, pid: i32) -> Option<String> {
        let running =
            self.panels.bots.live.iter().find(|state| state.turn.as_ref().is_some_and(|turn| turn.running.holds(pid)));
        if let Some(state) = running {
            return Some(state.bot.id.clone());
        }
        let named = process::env(pid).into_iter().find(|(key, value)| key == BOT_ENV && !value.is_empty());
        named.map(|(_, id)| id).filter(|id| !self.full_bot(id))
    }

    fn full_bot(&self, id: &str) -> bool {
        let found = self.panels.bots.live.iter().find(|state| state.bot.id == id);
        found.is_some_and(|state| state.bot.access == Access::Full)
    }

    pub(super) fn bot_writes(&self, client: u64, asker: Asker, command: &Command) -> Result<(), Refusal> {
        if !self.clients.bots.is_bot(client) {
            return Ok(());
        }
        match writes(command) {
            None => Ok(()),
            Some(Writes::Popup) => Err(no_popup()),
            Some(Writes::NewShell) => Err(no_new_shell()),
            Some(Writes::Access) => Err(no_access()),
            Some(Writes::Settings) => Err(no_settings()),
            Some(Writes::Says(bot)) => self.says_to(&bot),
            Some(Writes::Pane { pane, tab }) => self.types_into(self.target(asker, pane, tab)?),
        }
    }

    fn says_to(&self, bot: &str) -> Result<(), Refusal> {
        match self.bot_at(bot) {
            Ok(at) if self.panels.bots.live[at].bot.access == Access::Full => Err(no_full_bot()),
            _ => Ok(()),
        }
    }

    pub(super) fn bot_still_writes(&self, client: Option<u64>, key: u64, pane: u64) -> Result<(), Refusal> {
        let bots = &self.clients.bots;
        if client.is_some_and(|client| bots.is_bot(client)) || bots.owns(key) { self.types_into(pane) } else { Ok(()) }
    }

    fn types_into(&self, pane: u64) -> Result<(), Refusal> {
        let term = self.projects.pane_by(pane).ok_or_else(|| super::none("pane", pane))?;
        if fronts_agent(&self.config, term) { Ok(()) } else { Err(runs_none(pane)) }
    }

    pub(in crate::app) fn bot_request_keys(&self) -> HashSet<u64> {
        let pending = self.clients.requests.pending.iter();
        let bots = pending.filter(|p| p.client.is_some_and(|client| self.clients.bots.is_bot(client)));
        bots.map(|p| p.key).chain(self.clients.bots.keys.iter().copied()).collect()
    }

    pub(in crate::app) fn refuse_bots_enter(&mut self, key: u64, pane: u64) {
        self.answer_read(key, |_| Err(runs_none(pane)));
    }

    pub(super) fn forget_bot(&mut self, client: u64) {
        let pending = self.clients.requests.pending.iter().filter(|p| p.client == Some(client)).map(|p| p.key);
        let keys: Vec<u64> = pending.collect();
        self.clients.bots.forget(client, keys.into_iter());
    }

    pub(super) fn prune_bot_keys(&mut self) {
        let mut alive: HashSet<u64> = self.agents.launches.keys().collect();
        alive.extend(self.clients.requests.pending.iter().map(|p| p.key));
        self.clients.bots.prune(&alive);
    }
}

#[cfg(test)]
mod tests;
