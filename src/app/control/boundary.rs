use std::collections::{HashMap, HashSet};

use super::Asker;
use crate::agents;
use crate::agents::headless::Access;
use crate::agents::probe::Running;
use crate::app::App;
use crate::bots::BOT_ENV;
use crate::config::Config;
use crate::control::bots::BotRequest;
use crate::control::contract::{Code, Refusal};
use crate::control::{self, Command, Configure, HeldAction, Item, Machines};
use crate::log;
use crate::process;
use crate::protocol::ClientMessage;
use crate::term::Term;

mod owning;
mod table;
mod typing;

#[cfg(test)]
pub(in crate::app) use table::NO_KEYS;
pub(in crate::app) use table::OWN_ONLY;
pub(in crate::app) use typing::takes_a_bots_enter;

pub(crate) const NOT_OWN: &str = "a bot can only schedule and change its own queue entries";
pub(crate) const HEAD: &str = "a bot can only type into a pane that runs an agent";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Text {
    Kept,
    Missing,
    Given(String),
    Todo(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Gated {
    Pane { pane: Option<u64>, tab: Option<u64>, text: Text },
    NewShell,
    Access,
    Says(String),
    Settings,
    ServerStop,
    Clone,
    ShowsChat,
    Reveals,
    Closes(Item),
    Closing,
    Flows,
    Machines,
}

#[derive(Default)]
pub(in crate::app) struct BotClients {
    clients: HashMap<u64, String>,
    owners: HashMap<u64, String>,
    former: HashMap<u64, String>,
    keys: HashSet<u64>,
}

impl BotClients {
    fn is_bot(&self, client: u64) -> bool {
        self.clients.contains_key(&client)
    }

    pub(in crate::app) fn bot_of(&self, client: u64) -> Option<&str> {
        self.clients.get(&client).map(String::as_str)
    }

    pub(in crate::app) fn revoke_owners(&mut self, bot: &str) {
        let gone: Vec<u64> = self.owners.iter().filter(|(_, owner)| *owner == bot).map(|(client, _)| *client).collect();
        for client in gone {
            if let Some(owner) = self.owners.remove(&client) {
                self.former.insert(client, owner);
            }
        }
    }

    pub(in crate::app) fn former_of(&self, client: u64) -> Option<&str> {
        self.former.get(&client).map(String::as_str)
    }

    pub(in crate::app) fn owner_of(&self, client: u64) -> Option<&str> {
        self.owners.get(&client).map(String::as_str)
    }

    fn own(&mut self, client: u64, bot: String) {
        self.owners.insert(client, bot);
    }

    #[cfg(test)]
    pub(in crate::app) fn owned(&mut self, client: u64, bot: &str) {
        self.own(client, bot.to_string());
    }

    fn insert(&mut self, client: u64, bot: String) {
        self.clients.insert(client, bot);
    }

    #[cfg(test)]
    pub(in crate::app) fn marked(&mut self, client: u64, bot: &str) {
        self.insert(client, bot.to_string());
    }

    pub(in crate::app) fn forget(&mut self, client: u64, pending: impl Iterator<Item = u64>) {
        self.owners.remove(&client);
        self.former.remove(&client);
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

fn gated(command: &Command) -> Option<Gated> {
    if crate::bots::stops_server(command.name()) {
        return Some(Gated::ServerStop);
    }
    typing(command).or_else(|| places(command)).or_else(|| lifts(command))
}

fn given(text: Option<&String>) -> Text {
    text.filter(|text| !text.is_empty()).map_or(Text::Missing, |text| Text::Given(text.clone()))
}

fn typing(command: &Command) -> Option<Gated> {
    match command {
        Command::Send(send) => Some(Gated::Pane { pane: send.pane, tab: send.tab, text: given(send.text.as_ref()) }),
        Command::TodoSend(send) => Some(Gated::Pane { pane: send.pane, tab: send.tab, text: Text::Todo(send.id) }),
        Command::Held(held) if held.action == HeldAction::SendNow => {
            Some(Gated::Pane { pane: held.pane, tab: held.tab, text: Text::Kept })
        }
        Command::PasteFile(paste) => {
            paste.pane.map(|pane| Gated::Pane { pane: Some(pane), tab: None, text: Text::Kept })
        }
        Command::NewTab(new) if !blank(new.command.as_ref()) => Some(Gated::NewShell),
        Command::Split(split) if !blank(split.command.as_ref()) => Some(Gated::NewShell),
        _ => None,
    }
}

fn places(command: &Command) -> Option<Gated> {
    match command {
        Command::Close(close) => Some(Gated::Closes(close.item)),
        Command::CloseAgent(close) if !close.check => Some(Gated::Closing),
        Command::AgentsClose(close) if !close.check => Some(Gated::Closing),
        Command::FlowImport(import) if !import.check => Some(Gated::Flows),
        Command::FlowSave(save) if !save.check => Some(Gated::Flows),
        Command::Machines(Machines::Set(_) | Machines::Remove(_) | Machines::Update(_)) => Some(Gated::Machines),
        Command::Clone(_) => Some(Gated::Clone),
        _ => None,
    }
}

fn lifts(command: &Command) -> Option<Gated> {
    match command {
        Command::Bot(BotRequest::New(new)) => {
            new.access.as_deref().filter(|a| Access::parse(a) != Some(Access::Cornercase)).map(|_| Gated::Access)
        }
        Command::Bot(BotRequest::Edit(edit)) if edit.access.is_some() => Some(Gated::Access),
        Command::Bot(BotRequest::Edit(edit)) => Some(Gated::Says(edit.bot.clone())),
        Command::Bot(BotRequest::Say(say)) => Some(Gated::Says(say.bot.clone())),
        Command::Bot(BotRequest::Setup(target)) => Some(Gated::Says(target.bot.clone())),
        Command::View(open) if !blank(open.bot.as_ref()) => Some(Gated::ShowsChat),
        Command::ViewSet(set) if !blank(set.bot.as_ref()) => Some(Gated::ShowsChat),
        Command::ViewSet(set) if set.bot.is_none() && set.visible == Some(true) => Some(Gated::Reveals),
        Command::Config(Configure::Get | Configure::Search(_)) => None,
        Command::Config(_) => Some(Gated::Settings),
        Command::ConfigImport(import) if import.apply => Some(Gated::Settings),
        _ => None,
    }
}

fn stop_name(message: &ClientMessage) -> Option<&'static str> {
    match message {
        ClientMessage::KillServer => Some("kill-server"),
        ClientMessage::Restart => Some("restart"),
        _ => None,
    }
}

fn window_name(message: &ClientMessage) -> Option<&'static str> {
    match message {
        ClientMessage::Request(_) | ClientMessage::KillServer | ClientMessage::Restart => None,
        ClientMessage::Hello(_) => Some("hello"),
        ClientMessage::Event(_) => Some("input"),
        ClientMessage::Graphics(_) => Some("graphics"),
        ClientMessage::Activated(_) => Some("notification click"),
        ClientMessage::OpenFailed(_) => Some("open failed"),
        ClientMessage::Machines(_) => Some("machines"),
        ClientMessage::Tunnels(_) => Some("tunnels"),
    }
}

fn failed(why: &str) -> Refusal {
    Refusal::new(Code::Failed, why)
}

fn never_asks(name: &str) -> Refusal {
    failed(&format!(
        "a bot cannot start an agent in a mode that never asks before it acts (`{name}`); name another with --mode, \
         such as `--mode default`"
    ))
}

fn no_chat_view() -> Refusal {
    failed("a bot cannot show a bot's chat in a view")
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
        if let Some(owner) = peer.and_then(|pid| self.owner_of_process(pid)) {
            log::info!("control", "a bot's own client", client = client, bot = owner);
            self.clients.bots.own(client, owner);
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
        let mut states = self.panels.bots.live.iter();
        let contained = |state: &&crate::app::bots::Live| state.trees().any(Running::contained);
        states.find(contained).map(|state| state.bot.id.clone())
    }

    fn owner_of_process(&self, pid: i32) -> Option<String> {
        let owns = |state: &&crate::app::bots::Live| state.trees().any(|tree| tree.owns(pid));
        self.panels.bots.live.iter().find(owns).map(|state| state.bot.id.clone())
    }

    fn bot_of_process(&self, pid: i32) -> Option<String> {
        let holds = |state: &&crate::app::bots::Live| state.trees().any(|tree| tree.holds(pid));
        let running = self.panels.bots.live.iter().find(holds);
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
        if let Command::Queue(queue) = command {
            return self.queue_by_bot(client, queue);
        }
        if let Some(why) = table::closed(command.name()) {
            return Err(failed(why));
        }
        gated(command).map_or(Ok(()), |gated| self.ruled(client, asker, gated))
    }

    fn ruled(&self, client: u64, asker: Asker, gated: Gated) -> Result<(), Refusal> {
        match gated {
            Gated::NewShell => Err(no_new_shell()),
            Gated::Access => Err(failed(crate::bots::ACCESS_CHANGE)),
            Gated::Settings => Err(failed(table::NO_SETTINGS)),
            Gated::ServerStop => Err(failed(crate::bots::STOP_REFUSED)),
            Gated::Clone => {
                Err(failed("a bot cannot clone a repository; clone it yourself and the bot can work in the project"))
            }
            Gated::ShowsChat => Err(no_chat_view()),
            Gated::Closing => Err(failed(OWN_ONLY)),
            Gated::Flows => Err(failed(table::NO_FLOWS)),
            Gated::Machines => Err(failed(crate::bots::NO_MACHINES)),
            Gated::Reveals => self.reveals_chat(asker),
            Gated::Says(bot) => self.says_to(&bot),
            Gated::Closes(item) => self.closes_own(client, item),
            Gated::Pane { pane, tab, text } => self.bot_types(asker, (pane, tab), text),
        }
    }

    pub(super) fn bot_starts(&self, client: u64, kind: &str, mode: Option<&str>) -> Result<(), Refusal> {
        if !self.clients.bots.is_bot(client) {
            return Ok(());
        }
        agents::never_asks(&self.config, kind, mode).map_or(Ok(()), |name| Err(never_asks(&name)))
    }

    pub(super) fn bot_stays_here(&self, client: u64) -> Result<(), Refusal> {
        if self.clients.bots.is_bot(client) { Err(failed(crate::bots::ELSEWHERE)) } else { Ok(()) }
    }

    pub(crate) fn refused_stop(&self, client: u64, message: &ClientMessage) -> Option<&'static str> {
        let asked = stop_name(message).filter(|name| crate::bots::stops_server(name));
        self.refused_message(client, asked, crate::bots::STOP_REFUSED)
    }

    pub(crate) fn refused_window(&self, client: u64, message: &ClientMessage) -> Option<&'static str> {
        self.refused_message(client, window_name(message), crate::bots::WINDOW_REFUSED)
    }

    fn refused_message(&self, client: u64, asked: Option<&str>, why: &'static str) -> Option<&'static str> {
        let asked = asked?;
        let bot = self.clients.bots.bot_of(client)?;
        log::warning!("control", "a bot's message refused", client = client, bot = bot, asked = asked);
        Some(why)
    }

    fn queue_by_bot(&self, client: u64, queue: &control::Queue) -> Result<(), Refusal> {
        let Some(own) = self.clients.bots.bot_of(client) else { return Ok(()) };
        let mine = |id: &u64| self.panels.queued.queue().entry(*id).is_none_or(|entry| entry.bot() == Some(own));
        let names_own = |key: &str| self.bot_at(key).is_ok_and(|at| self.panels.bots.live[at].bot.id == own);
        let allowed = match queue {
            control::Queue::List => true,
            control::Queue::ClearStarted => false,
            control::Queue::Add(add) => add.bot.as_deref().is_some_and(names_own),
            control::Queue::Edit { id, add } => mine(id) && add.bot.as_deref().is_none_or(names_own),
            control::Queue::Remove(id)
            | control::Queue::Run(id)
            | control::Queue::Pause(id)
            | control::Queue::Resume(id) => mine(id),
            control::Queue::Restore(undo) => {
                self.panels.queued.removed(*undo).is_none_or(|entries| entries.iter().all(|e| e.bot() == Some(own)))
            }
        };
        if allowed { Ok(()) } else { Err(Refusal::new(Code::Failed, NOT_OWN)) }
    }

    fn reveals_chat(&self, asker: Asker) -> Result<(), Refusal> {
        match asker.view {
            Some(view) if self.view_names_a_bot(view) => Err(no_chat_view()),
            _ => Ok(()),
        }
    }

    fn says_to(&self, bot: &str) -> Result<(), Refusal> {
        match self.bot_at(bot) {
            Ok(at) if self.panels.bots.live[at].bot.access == Access::Full => {
                Err(failed("a bot cannot write to a bot with full access"))
            }
            _ => Ok(()),
        }
    }

    pub(super) fn bot_still_writes(&self, client: Option<u64>, key: u64, pane: u64) -> Result<(), Refusal> {
        let bots = &self.clients.bots;
        if client.is_some_and(|client| bots.is_bot(client)) || bots.owns(key) { self.types_into(pane) } else { Ok(()) }
    }

    pub(in crate::app) fn bot_request_keys(&self) -> HashSet<u64> {
        let pending = self.clients.requests.pending.iter();
        let bots = pending.filter(|p| p.client.is_some_and(|client| self.clients.bots.is_bot(client)));
        bots.map(|p| p.key).chain(self.clients.bots.keys.iter().copied()).collect()
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
