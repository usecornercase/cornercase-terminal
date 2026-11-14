use std::collections::HashSet;

use super::Asker;
use crate::agents;
use crate::agents::headless::Access;
use crate::app::App;
use crate::bots::BOT_ENV;
use crate::control::Command;
use crate::control::HeldAction;
use crate::control::contract::{Code, Refusal};
use crate::log;
use crate::process;
use crate::project::Hierarchy;

pub(crate) const HEAD: &str = "a bot can only type into a pane that runs an agent";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Writes {
    Pane { pane: Option<u64>, tab: Option<u64> },
    Popup,
    NewShell,
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
        let Some(pid) = peer.filter(|_| !self.panels.bots.live.is_empty()) else { return };
        if let Some(bot) = self.bot_of_process(pid) {
            log::info!("control", "a bot's client", client = client, bot = bot);
            self.clients.bots.insert(client, bot);
        }
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

    pub(in crate::app) fn bot_client(&self, client: u64) -> Option<&str> {
        self.clients.bots.get(&client).map(String::as_str)
    }

    pub(super) fn bot_writes(&self, client: u64, asker: Asker, command: &Command) -> Result<(), Refusal> {
        if self.bot_client(client).is_none() {
            return Ok(());
        }
        match writes(command) {
            None => Ok(()),
            Some(Writes::Popup) => Err(no_popup()),
            Some(Writes::NewShell) => Err(no_new_shell()),
            Some(Writes::Pane { pane, tab }) => self.types_into(self.target(asker, pane, tab)?),
        }
    }

    pub(super) fn bot_still_writes(&self, client: Option<u64>, key: u64, pane: u64) -> Result<(), Refusal> {
        if self.is_bots_request(client, key) { self.types_into(pane) } else { Ok(()) }
    }

    fn is_bots_request(&self, client: Option<u64>, key: u64) -> bool {
        client.is_some_and(|client| self.clients.bots.contains_key(&client)) || self.clients.bot_keys.contains(&key)
    }

    fn types_into(&self, pane: u64) -> Result<(), Refusal> {
        let term = self.projects.pane_by(pane).ok_or_else(|| super::none("pane", pane))?;
        match agents::detect(&self.config, &term.foreground_args()) {
            Some(_) => Ok(()),
            None => Err(runs_none(pane)),
        }
    }

    pub(in crate::app) fn bot_request_keys(&self) -> HashSet<u64> {
        let pending = self.clients.requests.pending.iter();
        let bots = pending.filter(|p| p.client.is_some_and(|client| self.clients.bots.contains_key(&client)));
        bots.map(|p| p.key).chain(self.clients.bot_keys.iter().copied()).collect()
    }

    pub(in crate::app) fn refuse_bots_enter(&mut self, key: u64, pane: u64) {
        self.answer_read(key, |_| Err(runs_none(pane)));
    }

    pub(super) fn forget_bot(&mut self, client: u64) {
        if self.clients.bots.remove(&client).is_none() {
            return;
        }
        let kept = self.clients.requests.pending.iter().filter(|p| p.client == Some(client)).map(|p| p.key);
        self.clients.bot_keys.extend(kept);
    }

    pub(super) fn prune_bot_keys(&mut self) {
        let launches: HashSet<u64> = self.agents.launches.keys().collect();
        let pending: HashSet<u64> = self.clients.requests.pending.iter().map(|p| p.key).collect();
        self.clients.bot_keys.retain(|key| launches.contains(key) || pending.contains(key));
    }
}

#[cfg(test)]
mod tests;
