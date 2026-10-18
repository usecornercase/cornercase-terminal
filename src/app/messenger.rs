use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use serde_json::Value;

use super::{App, AppEvent, Overlay, Toast};
use crate::control::{AwayInfo, Command, Done, Request};
use crate::error::Error;
use crate::issues::{Secret, secrets};
use crate::log::{self, Job, Level};
use crate::messenger::link::{self, Link};
use crate::messenger::{Advice, Chat, Identity, Key, Keyboard, Message, Order, Owned, Saved, Tap, Via};
use crate::settings;
use crate::ui;
use crate::web::auth::{Pairing, random_hex};

mod chat;
mod drafts;
mod notices;
mod pairing;

pub(in crate::app) const CLIENTS: u64 = 1 << 62;
const CLIENTS_PER_VIA: u64 = 1 << 48;
const ROUTES: usize = 256;
const TAGS: u64 = 256;
const RECENT: i64 = 60;
const ASK_FOR: Duration = Duration::from_mins(10);

#[derive(Debug)]
pub(crate) enum MessengerEvent {
    Link(Via, link::Event),
    Checked { via: Via, token: Secret, result: Result<Identity, String>, request: Option<u64> },
    Paired { via: Via, token: Secret, advice: Advice, result: Result<Owned, String> },
    Advised { via: Via, link: u64, advice: Option<&'static str> },
}

impl MessengerEvent {
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::Link(via, _) => via.target(),
            Self::Checked { via, .. } => via.checked(),
            Self::Paired { via, .. } => via.pairing(),
            Self::Advised { via, .. } => via.advised(),
        }
    }
}

impl From<(Via, link::Event)> for AppEvent {
    fn from((via, event): (Via, link::Event)) -> Self {
        Self::Messenger(MessengerEvent::Link(via, event))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Phase {
    Off,
    Listening,
    Problem(String),
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    Pane(u64),
    Branch(u32),
    Prompt(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum By {
    Window,
    Chat,
    Command,
}

impl By {
    fn name(self, via: Via) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Chat => via.target(),
            Self::Command => "command",
        }
    }
}

enum Asked {
    Send { message: i64 },
    Keys { press: String, chat_message: i64, key: String, agent: String },
    Finished { pane: u64, header: String },
    Start { message: i64, place: String, agent: String },
}

pub(super) struct Messenger {
    via: Via,
    api: String,
    path: PathBuf,
    secrets: PathBuf,
    token: Option<String>,
    saved: Saved,
    link: Option<Link>,
    links: u64,
    pub(super) phase: Phase,
    pub(super) away: bool,
    run: String,
    pairing: Option<Pairing>,
    owner: pairing::Owner,
    advice: Option<&'static str>,
    tags: HashMap<u64, Route>,
    next_tag: u64,
    routes: VecDeque<(i64, Route)>,
    open_ask: Option<(Route, Instant)>,
    asked: HashMap<u64, Asked>,
    next_client: u64,
    watches: HashMap<u64, notices::Watch>,
    held: VecDeque<(u32, String, i64)>,
    next_held: u32,
    drafts: VecDeque<drafts::Draft>,
    next_draft: u32,
}

fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

pub(super) fn run_id() -> String {
    random_hex(2).unwrap_or_else(|_| format!("{:04x}", unix_now() & 0xffff))
}

pub(super) fn messengers(config: &Path, secrets: &Path) -> Vec<Messenger> {
    Via::ALL.into_iter().map(|via| Messenger::new(via, config, secrets.to_path_buf())).collect()
}

impl Messenger {
    fn new(via: Via, config: &Path, secrets: PathBuf) -> Self {
        let path = via.saved(config);
        Self {
            via,
            api: via.api(),
            token: secrets::read(&secrets, via.token_key()),
            saved: Saved::load(&path),
            path,
            secrets,
            link: None,
            links: 0,
            phase: Phase::Off,
            away: false,
            run: run_id(),
            pairing: None,
            owner: pairing::Owner::default(),
            advice: None,
            tags: HashMap::new(),
            next_tag: 0,
            routes: VecDeque::new(),
            open_ask: None,
            asked: HashMap::new(),
            next_client: 0,
            watches: HashMap::new(),
            held: VecDeque::new(),
            next_held: 0,
            drafts: VecDeque::new(),
            next_draft: 0,
        }
    }

    fn chat(&self) -> Option<&Chat> {
        self.saved.chat.as_ref().filter(|_| self.token.is_some())
    }

    pub(super) fn paired(&self) -> bool {
        self.chat().is_some()
    }

    pub(super) fn ready(&self) -> bool {
        self.paired() && self.phase != Phase::Rejected
    }

    pub(super) fn bot(&self) -> Option<&str> {
        self.token.as_ref().map(|_| self.saved.bot.as_str()).filter(|name| !name.is_empty())
    }

    pub(super) fn chat_name(&self) -> Option<&str> {
        self.chat().map(|chat| chat.name.as_str())
    }

    pub(super) fn note(&self) -> String {
        match &self.phase {
            _ if self.token.is_none() => self.via.unset().into(),
            Phase::Rejected => self.via.rejected(),
            Phase::Problem(why) => why.clone(),
            Phase::Off | Phase::Listening => match (self.advice, self.away) {
                (Some(advice), true) => format!("away · {advice}"),
                (Some(advice), false) => advice.into(),
                (None, true) => "connected · away: agents message you there".into(),
                (None, false) => "connected".into(),
            },
        }
    }

    fn tap(&self, text: impl Into<String>, tap: &Tap) -> Key {
        Key { text: text.into(), data: tap.data(&self.run) }
    }

    fn tag(&mut self, route: Route) -> u64 {
        self.next_tag += 1;
        let tag = self.next_tag;
        self.tags.insert(tag, route);
        let oldest = tag.saturating_sub(TAGS);
        self.tags.retain(|kept, _| *kept > oldest);
        tag
    }

    fn take_open_ask(&mut self, now: Instant) -> Option<Route> {
        self.open_ask.take().filter(|(_, at)| now.duration_since(*at) < ASK_FOR).map(|(route, _)| route)
    }

    fn remember(&mut self, message: i64, route: Route) {
        self.routes.retain(|(id, _)| *id != message);
        self.routes.push_back((message, route));
        while self.routes.len() > ROUTES {
            self.routes.pop_front();
        }
    }

    fn route(&self, message: i64) -> Option<Route> {
        self.routes.iter().rev().find(|(id, _)| *id == message).map(|(_, route)| *route)
    }

    fn save(&self) {
        if let Err(e) = self.saved.save(&self.path) {
            log::error!(self.via.target(), "cannot save the paired chat", error = e);
        }
    }
}

impl App {
    pub(super) fn messenger(&self, via: Via) -> &Messenger {
        &self.messengers[via.index()]
    }

    fn messenger_mut(&mut self, via: Via) -> &mut Messenger {
        &mut self.messengers[via.index()]
    }

    pub(super) fn sync_messengers(&mut self) {
        for via in Via::ALL {
            self.sync_messenger(via);
        }
    }

    fn sync_messenger(&mut self, via: Via) {
        if !matches!(self.overlay, Some(Overlay::Pairing(shown)) if shown == via) {
            let messenger = self.messenger_mut(via);
            messenger.pairing = None;
            messenger.owner.closed();
        }
        let messenger = self.messenger(via);
        let wanted = messenger.token.is_some() && messenger.phase != Phase::Rejected;
        match (messenger.link.is_some(), wanted) {
            (false, true) => self.start_link(via),
            (true, false) => self.messenger_mut(via).link = None,
            _ => {}
        }
    }

    fn start_link(&mut self, via: Via) {
        let messenger = &mut self.messengers[via.index()];
        let Some(token) = messenger.token.clone() else {
            return;
        };
        messenger.links += 1;
        let id = messenger.links;
        let link = via.start_link(id, &messenger.api, &token, &self.tx, unix_now() - RECENT);
        link.send(Order::Commands);
        log::info!(via.target(), "listening", link = id, paired = messenger.paired());
        messenger.link = Some(link);
        messenger.phase = Phase::Listening;
        self.refresh_messenger_note(via);
        self.read_advice(via, id, token);
    }

    fn read_advice(&self, via: Via, link: u64, token: String) {
        if !via.advises() {
            return;
        }
        let api = self.messenger(via).api.clone();
        let job = Job::new(Level::Debug, via.target(), "read app");
        self.spawn_job(
            job,
            move || via.read_advice(&api, &token).map_err(Error::Api),
            move |result, _| {
                let advice = result.ok()?;
                Some(AppEvent::Messenger(MessengerEvent::Advised { via, link, advice }))
            },
        );
    }

    fn restart_link(&mut self, via: Via) {
        let messenger = self.messenger_mut(via);
        messenger.link = None;
        messenger.phase = Phase::Off;
        self.sync_messenger(via);
    }

    fn post(&self, via: Via, order: Order) {
        if let Some(link) = &self.messenger(via).link {
            link.send(order);
        }
    }

    fn say(&mut self, via: Via, text: String, keyboard: Keyboard, route: Option<Route>, reply_to: Option<i64>) {
        let messenger = self.messenger_mut(via);
        if route.is_some() {
            messenger.open_ask = None;
        }
        let Some(chat) = messenger.chat().map(|c| c.id) else { return };
        let tag = route.map(|route| messenger.tag(route));
        self.post(via, Order::Send { chat, text, keyboard, reply_to, ask: None, tag });
    }

    fn say_long(&mut self, via: Via, header: &str, parts: Vec<String>, keyboard: Keyboard, route: Option<Route>) {
        let count = parts.len();
        if count == 0 {
            self.say(via, header.to_string(), keyboard, route, None);
            return;
        }
        let markup = via.markup();
        for (i, part) in parts.into_iter().enumerate() {
            let joined = i == 0 && markup.fits(header, &part);
            if i == 0 && !joined {
                self.say(via, header.to_string(), Vec::new(), route, None);
            }
            let text = if joined { format!("{header}\n\n{part}") } else { part };
            let keys = if i + 1 == count { keyboard.clone() } else { Vec::new() };
            self.say(via, text, keys, route, None);
        }
    }

    fn ask(&mut self, via: Via, text: String, placeholder: &str, route: Route) {
        let messenger = self.messenger_mut(via);
        let Some(chat) = messenger.chat().map(|c| c.id) else { return };
        let tag = messenger.tag(route);
        if !via.forces_reply() {
            messenger.open_ask = Some((route, Instant::now()));
        }
        let ask = Some(placeholder.chars().take(64).collect());
        self.post(via, Order::Send { chat, text, keyboard: Vec::new(), reply_to: None, ask, tag: Some(tag) });
    }

    fn reply(&mut self, via: Via, message: i64, text: &str) {
        let text = via.markup().escape(text);
        self.say(via, text, Vec::new(), None, Some(message));
    }

    fn answer_press(&self, via: Via, press: &str, text: &str) {
        let text = text.chars().take(190).collect();
        self.post(via, Order::Answer { id: press.to_string(), text });
    }

    fn chat_request(&mut self, via: Via, command: Command, asked: Asked, area: Option<Rect>) {
        let messenger = self.messenger_mut(via);
        messenger.next_client += 1;
        let offset = CLIENTS_PER_VIA * u64::try_from(via.index()).unwrap_or_default();
        let client = CLIENTS + offset + messenger.next_client;
        messenger.asked.insert(client, asked);
        let request = Request { caller: None, server: None, contract: None, boot: None, view: None, command };
        self.run_request(client, Ok(request), 0, area, Instant::now());
    }

    pub(in crate::app) fn chat_answered(&mut self, client: u64, reply: Result<Value, String>) {
        let Some(via) = Via::ALL.into_iter().find(|via| self.messenger(*via).asked.contains_key(&client)) else {
            return;
        };
        let Some(asked) = self.messenger_mut(via).asked.remove(&client) else { return };
        match asked {
            Asked::Send { message } => match reply {
                Ok(_) => self.post_reaction(via, message),
                Err(why) => self.reply(via, message, &why),
            },
            Asked::Keys { press, chat_message, key, agent } => match reply {
                Ok(_) => {
                    self.answer_press(via, &press, &format!("sent {key} to {agent}"));
                    if key.chars().all(|c| c.is_ascii_digit())
                        && let Some(chat) = self.messenger(via).chat().map(|c| c.id)
                    {
                        self.post(via, Order::Buttons { chat, message: chat_message, keyboard: Vec::new() });
                    }
                }
                Err(why) => self.answer_press(via, &press, &why),
            },
            Asked::Finished { pane, header } => self.finished_read(via, pane, &header, reply),
            Asked::Start { message, place, agent } => match reply {
                Ok(value) => self.started(via, &value, message, &place, &agent),
                Err(why) => self.reply(via, message, &why),
            },
        }
    }

    fn post_reaction(&self, via: Via, message: i64) {
        if let Some(chat) = self.messenger(via).chat().map(|c| c.id) {
            self.post(via, Order::React { chat, message, emoji: "👍".into() });
        }
    }

    fn started(&mut self, via: Via, value: &Value, message: i64, place: &str, agent: &str) {
        let done: Done = serde_json::from_value(value.clone()).unwrap_or_default();
        let later = if self.messenger(via).away { "" } else { "\n/away to get its messages here." };
        let m = via.markup();
        let text = format!(
            "Started {} in {}. Reply to this message to write to it.{later}",
            m.bold(&m.escape(agent)),
            m.escape(place)
        );
        let route = done.ids.pane.map(Route::Pane);
        self.say(via, text, Vec::new(), route, Some(message));
    }

    pub(super) fn messenger_event(&mut self, event: MessengerEvent, area: Rect) {
        let current = |app: &Self, via: Via, id: u64| app.messenger(via).link.as_ref().is_some_and(|l| l.id == id);
        match event {
            MessengerEvent::Checked { via, token, result, request } => {
                self.messenger_checked(via, &token, result, request);
            }
            MessengerEvent::Paired { via, token, advice, result } => self.owner_answered(via, &token, advice, result),
            MessengerEvent::Link(via, link::Event::Updates(id, updates)) if current(self, via, id) => {
                for update in updates {
                    self.chat_update(via, update, area);
                }
            }
            MessengerEvent::Link(via, link::Event::Sent { link, tag, message }) if current(self, via, link) => {
                let messenger = self.messenger_mut(via);
                if let Some(route) = messenger.tags.get(&tag).copied() {
                    messenger.remember(message, route);
                }
            }
            MessengerEvent::Link(via, link::Event::Problem(id, problem)) if current(self, via, id) => {
                self.messenger_mut(via).phase = problem.map_or(Phase::Listening, Phase::Problem);
                self.refresh_messenger_note(via);
            }
            MessengerEvent::Link(via, link::Event::Rejected(id)) if current(self, via, id) => {
                let messenger = self.messenger_mut(via);
                messenger.link = None;
                messenger.phase = Phase::Rejected;
                self.refresh_messenger_note(via);
            }
            MessengerEvent::Advised { via, link, advice } if current(self, via, link) => {
                self.messenger_mut(via).advice = advice;
                self.refresh_messenger_note(via);
            }
            MessengerEvent::Link(..) | MessengerEvent::Advised { .. } => {}
        }
    }

    pub(super) fn check_messenger(&mut self, via: Via, token: Secret) {
        self.check_messenger_for(via, token, None);
    }

    pub(super) fn check_messenger_for(&mut self, via: Via, token: Secret, request: Option<u64>) {
        let api = self.messenger(via).api.clone();
        let job = Job::new(Level::Info, via.target(), "check bot");
        let checked = token.0.clone();
        self.spawn_job(
            job,
            move || Ok(via.check(&api, &checked)),
            move |result, _| {
                let result = result.unwrap_or_else(|e| Err(e.to_string()));
                Some(AppEvent::Messenger(MessengerEvent::Checked { via, token, result, request }))
            },
        );
    }

    fn messenger_checked(&mut self, via: Via, token: &Secret, result: Result<Identity, String>, request: Option<u64>) {
        let messenger = self.messenger(via);
        let saved = result.and_then(|found| {
            secrets::write(&messenger.secrets, via.token_key(), &token.0)
                .map(|()| found)
                .map_err(|e| format!("failed to save the token: {e}"))
        });
        let saved = saved.map(|Identity { name, advice }| {
            if let Advice::Read(advice) = advice {
                self.messenger_mut(via).advice = advice;
            }
            name
        });
        if let Ok(name) = &saved {
            log::info!(via.target(), "bot set", bot = name);
            let messenger = self.messenger_mut(via);
            if messenger.saved.bot != *name {
                messenger.saved.chat = None;
            }
            messenger.saved.bot.clone_from(name);
            messenger.save();
            messenger.token = Some(token.0.trim().to_string());
            self.restart_link(via);
        }
        let paired = self.messenger(via).paired();
        if let Some(key) = request {
            self.refresh_messenger_note(via);
            self.secret_answered(key, saved.map(|name| format!("bot @{name} is ready")));
            return;
        }
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.bot_checked(via, saved, paired);
        }
        self.refresh_messenger_note(via);
    }

    pub(super) fn remove_messenger(&mut self, via: Via) -> Result<String, String> {
        let messenger = self.messenger_mut(via);
        secrets::remove(&messenger.secrets, via.token_key()).map_err(|e| format!("failed to remove the token: {e}"))?;
        log::info!(via.target(), "bot removed");
        messenger.token = None;
        messenger.advice = None;
        messenger.saved = Saved::default();
        messenger.save();
        messenger.away = false;
        messenger.phase = Phase::Off;
        self.sync_messenger(via);
        self.refresh_messenger_note(via);
        Ok(format!("the {} bot was removed", via.name()))
    }

    pub(super) fn forget_chat(&mut self, via: Via) -> String {
        log::info!(via.target(), "chat forgotten");
        let messenger = self.messenger_mut(via);
        messenger.saved.chat = None;
        messenger.save();
        messenger.away = false;
        self.refresh_messenger_note(via);
        format!("this computer no longer messages your {}", via.name())
    }

    pub(super) fn pair(&mut self, via: Via) -> Option<&'static str> {
        if self.messenger(via).bot().is_none() {
            return Some("paste the bot's token first");
        }
        if via.pairs_by_owner() {
            self.messenger_mut(via).owner.closed();
        } else {
            match Pairing::new(Instant::now()) {
                Ok(pairing) => self.messenger_mut(via).pairing = Some(pairing),
                Err(e) => log::error!(via.target(), "cannot make a pairing code", error = e),
            }
        }
        self.overlay = Some(Overlay::Pairing(via));
        None
    }

    fn try_pairing(&mut self, via: Via, message: &Message) -> bool {
        let messenger = self.messenger(via);
        let code = via.pairing_code(&message.text);
        let accepted = message.private
            && code.is_some_and(|code| messenger.pairing.as_ref().is_some_and(|p| p.accepts(code, Instant::now())));
        if !accepted {
            return false;
        }
        let paired = u64::try_from(unix_now()).unwrap_or_default();
        self.connect(via, Chat { id: message.chat, user: message.user, name: message.name.clone(), paired });
        self.say(via, chat::welcome(via), Vec::new(), None, None);
        true
    }

    pub(super) fn pairing_view(&self, via: Via) -> ui::RemoteAccess {
        let messenger = self.messenger(via);
        let bot = messenger.bot().unwrap_or_default();
        let code = match &messenger.pairing {
            Some(pairing) => Some(pairing.token.as_str()),
            None => via.pairs_by_owner().then_some(""),
        };
        let (url, mut lines) = match code {
            Some(code) => {
                let (url, lines) = via.pairing_link(bot, messenger.token.as_deref().unwrap_or_default(), code);
                (Some(url).filter(|url| !url.is_empty()), lines)
            }
            None => (None, vec!["could not make a code, see server.log".into()]),
        };
        lines.extend(messenger.owner.failed.clone());
        lines.extend(messenger.advice.map(String::from));
        let code = url.as_deref().map(ui::remote_access::code).unwrap_or_default();
        ui::RemoteAccess { title: via.name(), url, code, lines }
    }

    pub(super) fn pairing_mouse(&mut self, via: Via, ev: MouseEvent, pos: Position, area: Rect) {
        let done = ui::remote_done(area, &self.pairing_view(via));
        if ev.kind == MouseEventKind::Down(MouseButton::Left) && done.contains(pos) {
            self.overlay = None;
        }
    }

    pub(super) fn refresh_messenger_note(&mut self, via: Via) {
        self.settings_changed();
        let view = self.messenger_settings(via);
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            *s.messenger_mut(via) = view;
        }
    }

    pub(super) fn messenger_settings(&self, via: Via) -> settings::Messenger {
        let messenger = self.messenger(via);
        settings::Messenger {
            bot: messenger.bot().map(|name| format!("@{name}")),
            chat: messenger.chat_name().map(String::from),
            note: messenger.note(),
            checking: false,
            refused: messenger.phase == Phase::Rejected,
        }
    }

    pub(super) fn toggle_away(&mut self, via: Via) {
        if !self.messenger(via).ready() {
            self.open_settings_on(&settings::Row::Bot(via));
        } else if self.messenger(via).away {
            self.come_back(via, By::Window);
        } else if let Err(why) = self.go_away(via, By::Window) {
            self.toast = Some(Toast::new(why, ui::ToastIcon::Bug));
        }
    }

    pub(super) fn toggle_away_everywhere(&mut self) {
        let back = self.away_anywhere();
        let vias: Vec<Via> = Via::ALL
            .into_iter()
            .filter(|via| if back { self.messenger(*via).away } else { self.messenger(*via).ready() })
            .collect();
        if vias.is_empty() {
            let unset = Via::ALL.into_iter().find(|via| !self.messenger(*via).ready()).unwrap_or(Via::Telegram);
            self.open_settings_on(&settings::Row::Bot(unset));
            return;
        }
        let (mut went, mut failed) = (Vec::new(), Vec::new());
        for via in vias {
            if back {
                self.come_back(via, By::Window);
                went.push(via.name());
            } else {
                match self.go_away(via, By::Window) {
                    Ok(()) => went.push(via.name()),
                    Err(why) => failed.push(why),
                }
            }
        }
        let names = went.join(" and ");
        if !failed.is_empty() {
            let why = failed.join("; ");
            let text = if went.is_empty() { why } else { format!("away: agents message you on {names}; {why}") };
            self.toast = Some(Toast::new(text, ui::ToastIcon::Bug));
        } else if went.len() > 1 {
            let text = if back {
                format!("back: agents no longer message you on {names}")
            } else {
                format!("away: agents message you on {names}")
            };
            self.toast = Some(Toast::new(text, ui::ToastIcon::Check));
        }
    }

    pub(super) fn go_away(&mut self, via: Via, by: By) -> Result<(), String> {
        let messenger = self.messenger_mut(via);
        if !messenger.paired() {
            return Err(format!("{} is not set up: pair a phone in Settings › Remote", via.name()));
        }
        if messenger.phase == Phase::Rejected {
            return Err(format!("{} refused the bot token: paste it again in Settings › Remote", via.name()));
        }
        if !messenger.away {
            messenger.away = true;
            log::info!(via.target(), "away", by = by.name(via));
        }
        let (status, keyboard) = self.status_message(via);
        let m = via.markup();
        let header = format!(
            "{}: I'll message you here when an agent needs you or finishes, until you send /back or turn it off in \
             cornercase.\n\n{}",
            m.bold("Away"),
            m.bold(notices::AGENTS)
        );
        self.say_long(via, &header, status, keyboard, None);
        if by != By::Chat {
            self.toast = Some(Toast::new(format!("away: agents message you on {}", via.name()), ui::ToastIcon::Check));
        }
        self.refresh_messenger_note(via);
        Ok(())
    }

    pub(super) fn come_back(&mut self, via: Via, by: By) {
        let messenger = self.messenger_mut(via);
        if !messenger.away {
            return;
        }
        messenger.away = false;
        log::info!(via.target(), "back", by = by.name(via));
        let text = format!("{}: I no longer message you here. /away starts again.", via.markup().bold("Back"));
        self.say(via, text, Vec::new(), None, None);
        if by != By::Chat {
            let back = format!("back: agents no longer message you on {}", via.name());
            self.toast = Some(Toast::new(back, ui::ToastIcon::Check));
        }
        self.refresh_messenger_note(via);
    }

    pub(in crate::app) fn away_info(&self) -> Vec<AwayInfo> {
        Via::ALL
            .into_iter()
            .map(|via| {
                let messenger = self.messenger(via);
                AwayInfo { platform: via.target().to_string(), ready: messenger.ready(), away: messenger.away }
            })
            .collect()
    }

    pub(super) fn away_anywhere(&self) -> bool {
        self.messengers.iter().any(|messenger| messenger.away)
    }

    pub(in crate::app) fn away_request(&mut self, off: bool) -> Result<Option<Value>, String> {
        let changed: Vec<Via>;
        if off {
            changed = Via::ALL.into_iter().filter(|via| self.messenger(*via).away).collect();
            for via in Via::ALL {
                self.come_back(via, By::Command);
            }
        } else {
            let paired: Vec<Via> = Via::ALL.into_iter().filter(|via| self.messenger(*via).paired()).collect();
            let mut refused = None;
            let mut went = false;
            for via in if paired.is_empty() { Via::ALL.to_vec() } else { paired } {
                match self.go_away(via, By::Command) {
                    Ok(()) => went = true,
                    Err(why) => refused = refused.or(Some(why)),
                }
            }
            if let (false, Some(why)) = (went, refused) {
                return Err(why);
            }
            changed = Via::ALL.into_iter().filter(|via| self.messenger(*via).away).collect();
        }
        let platforms: Vec<&str> = changed.into_iter().map(Via::name).collect();
        Ok(Some(serde_json::json!({ "away": self.away_anywhere(), "platforms": platforms })))
    }
}

#[cfg(test)]
pub(super) fn capture(app: &mut App, via: Via) -> std::sync::mpsc::Receiver<Order> {
    let (link, orders) = Link::capture(1);
    let messenger = app.messenger_mut(via);
    messenger.links = 1;
    messenger.link = Some(link);
    messenger.phase = Phase::Listening;
    orders
}

#[cfg(test)]
pub(super) fn set_up(app: &mut App, via: Via, token: &str, bot: &str, chat: Option<Chat>) {
    let messenger = app.messenger_mut(via);
    messenger.token = Some(token.into());
    messenger.saved = Saved::new(bot.into(), chat);
}

#[cfg(test)]
pub(super) fn run(app: &App, via: Via) -> &str {
    &app.messenger(via).run
}

#[cfg(test)]
pub(super) fn point(app: &mut App, via: Via, api: &str) {
    app.messenger_mut(via).api = api.into();
}

#[cfg(test)]
mod tests;
