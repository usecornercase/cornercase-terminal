use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{Event, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};
use serde_json::Value;

use super::{App, AppEvent, Overlay, Toast};
use crate::control::{Command, Done, Request};
use crate::issues::{Secret, secrets};
use crate::log::{self, Job, Level};
use crate::paths;
use crate::settings;
use crate::telegram::link::{self, Link};
use crate::telegram::{self, Bot, Chat, Key, Keyboard, Order, Saved, Tap, html};
use crate::ui;
use crate::web::auth::{Pairing, random_hex};

mod chat;
mod drafts;
mod notices;

pub(in crate::app) const CLIENTS: u64 = 1 << 62;
const ROUTES: usize = 256;
const RECENT: i64 = 60;
const NOT_PAIRED: &str = "Telegram is not set up: pair a phone in Settings › Agents › Telegram";
const AWAY: &str = "away: agents message you on Telegram until you type in a window";
const BACK: &str = "back: agents no longer message you on Telegram";
const AWAY_HTML: &str = "<b>Away</b>: I'll message you here when an agent needs you or finishes. Typing in a \
    cornercase window, or /back, stops it.";
const BACK_HTML: &str = "<b>Back</b>: I no longer message you here. /away starts again.";
const BACK_AT_HTML: &str = "<b>Back at the computer</b>: I no longer message you here. /away starts again.";
const CONNECTED: &str = "Telegram connected";

#[derive(Debug)]
pub(crate) enum TelegramEvent {
    Link(link::Event),
    Checked { token: Secret, result: Result<String, String> },
}

impl TelegramEvent {
    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::Link(_) => "telegram",
            Self::Checked { .. } => "telegram bot checked",
        }
    }
}

impl From<link::Event> for AppEvent {
    fn from(event: link::Event) -> Self {
        Self::Telegram(TelegramEvent::Link(event))
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
    Telegram,
    Command,
}

impl By {
    fn name(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Telegram => "telegram",
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

pub(super) struct Telegram {
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
    tags: HashMap<u64, Route>,
    next_tag: u64,
    routes: VecDeque<(i64, Route)>,
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

fn run_id() -> String {
    random_hex(2).unwrap_or_else(|_| format!("{:04x}", unix_now() & 0xffff))
}

impl Telegram {
    pub(super) fn new(config: &Path, secrets: PathBuf) -> Self {
        let path = paths::telegram(config);
        Self {
            api: std::env::var(telegram::API_ENV).unwrap_or_else(|_| telegram::API.into()),
            token: secrets::read(&secrets, telegram::TOKEN_KEY),
            saved: Saved::load(&path),
            path,
            secrets,
            link: None,
            links: 0,
            phase: Phase::Off,
            away: false,
            run: run_id(),
            pairing: None,
            tags: HashMap::new(),
            next_tag: 0,
            routes: VecDeque::new(),
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

    pub(super) fn bot(&self) -> Option<&str> {
        self.token.as_ref().map(|_| self.saved.bot.as_str()).filter(|name| !name.is_empty())
    }

    pub(super) fn chat_name(&self) -> Option<&str> {
        self.chat().map(|chat| chat.name.as_str())
    }

    pub(super) fn note(&self) -> String {
        match &self.phase {
            _ if self.token.is_none() => "make a bot with @BotFather and paste its token here".into(),
            Phase::Rejected => telegram::Failure::Rejected.to_string(),
            Phase::Problem(why) => why.clone(),
            Phase::Off | Phase::Listening if self.away => "connected · away: agents message you there".into(),
            Phase::Off | Phase::Listening => "connected".into(),
        }
    }

    fn tap(&self, text: impl Into<String>, tap: &Tap) -> Key {
        Key { text: text.into(), data: tap.data(&self.run) }
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
            log::error!("telegram", "cannot save the paired chat", error = e);
        }
    }
}

impl App {
    pub(super) fn sync_telegram(&mut self) {
        if !matches!(self.overlay, Some(Overlay::Telegram)) {
            self.telegram.pairing = None;
        }
        let wanted = self.telegram.token.is_some() && self.telegram.phase != Phase::Rejected;
        match (self.telegram.link.is_some(), wanted) {
            (false, true) => self.start_link(),
            (true, false) => self.telegram.link = None,
            _ => {}
        }
    }

    fn start_link(&mut self) {
        let Some(token) = self.telegram.token.as_deref() else { return };
        self.telegram.links += 1;
        let id = self.telegram.links;
        let bot = Bot::new(&self.telegram.api, token);
        let link = Link::start(id, &bot, &self.tx, link::POLL, unix_now() - RECENT);
        link.send(Order::Commands);
        log::info!("telegram", "listening", link = id, paired = self.telegram.paired());
        self.telegram.link = Some(link);
        self.telegram.phase = Phase::Listening;
        self.refresh_telegram_note();
    }

    fn restart_link(&mut self) {
        self.telegram.link = None;
        self.telegram.phase = Phase::Off;
        self.sync_telegram();
    }

    fn post(&self, order: Order) {
        if let Some(link) = &self.telegram.link {
            link.send(order);
        }
    }

    fn say(&mut self, html: String, keyboard: Keyboard, route: Option<Route>, reply_to: Option<i64>) {
        let Some(chat) = self.telegram.chat().map(|c| c.id) else { return };
        let tag = route.map(|route| {
            self.telegram.next_tag += 1;
            self.telegram.tags.insert(self.telegram.next_tag, route);
            self.telegram.next_tag
        });
        self.post(Order::Send { chat, html, keyboard, reply_to, ask: None, tag });
    }

    fn say_long(&mut self, header: &str, parts: Vec<String>, keyboard: Keyboard, route: Option<Route>) {
        let count = parts.len();
        if count == 0 {
            self.say(header.to_string(), keyboard, route, None);
            return;
        }
        for (i, part) in parts.into_iter().enumerate() {
            let joined = i == 0 && html::fits(header, &part);
            if i == 0 && !joined {
                self.say(header.to_string(), Vec::new(), route, None);
            }
            let html = if joined { format!("{header}\n\n{part}") } else { part };
            let keys = if i + 1 == count { keyboard.clone() } else { Vec::new() };
            self.say(html, keys, route, None);
        }
    }

    fn ask(&mut self, html: String, placeholder: &str, route: Route) {
        let Some(chat) = self.telegram.chat().map(|c| c.id) else { return };
        self.telegram.next_tag += 1;
        let tag = self.telegram.next_tag;
        self.telegram.tags.insert(tag, route);
        let ask = Some(placeholder.chars().take(64).collect());
        self.post(Order::Send { chat, html, keyboard: Vec::new(), reply_to: None, ask, tag: Some(tag) });
    }

    fn reply(&mut self, message: i64, text: &str) {
        self.say(html::escape(text), Vec::new(), None, Some(message));
    }

    fn answer_press(&self, press: &str, text: &str) {
        let text = text.chars().take(190).collect();
        self.post(Order::Answer { id: press.to_string(), text });
    }

    fn telegram_request(&mut self, command: Command, asked: Asked, area: Option<Rect>) {
        self.telegram.next_client += 1;
        let client = CLIENTS + self.telegram.next_client;
        self.telegram.asked.insert(client, asked);
        let request = Request { caller: None, server: None, command };
        self.run_request(client, Ok(request), 0, area, Instant::now());
    }

    pub(in crate::app) fn telegram_answered(&mut self, client: u64, reply: Result<Value, String>) {
        let Some(asked) = self.telegram.asked.remove(&client) else { return };
        match asked {
            Asked::Send { message } => match reply {
                Ok(_) => self.post_reaction(message),
                Err(why) => self.reply(message, &why),
            },
            Asked::Keys { press, chat_message, key, agent } => match reply {
                Ok(_) => {
                    self.answer_press(&press, &format!("sent {key} to {agent}"));
                    if key.chars().all(|c| c.is_ascii_digit())
                        && let Some(chat) = self.telegram.chat().map(|c| c.id)
                    {
                        self.post(Order::Buttons { chat, message: chat_message, keyboard: Vec::new() });
                    }
                }
                Err(why) => self.answer_press(&press, &why),
            },
            Asked::Finished { pane, header } => self.finished_read(pane, &header, reply),
            Asked::Start { message, place, agent } => match reply {
                Ok(value) => self.started(&value, message, &place, &agent),
                Err(why) => self.reply(message, &why),
            },
        }
    }

    fn post_reaction(&self, message: i64) {
        if let Some(chat) = self.telegram.chat().map(|c| c.id) {
            self.post(Order::React { chat, message, emoji: "👍".into() });
        }
    }

    fn started(&mut self, value: &Value, message: i64, place: &str, agent: &str) {
        let done: Done = serde_json::from_value(value.clone()).unwrap_or_default();
        let later = if self.telegram.away { "" } else { "\n/away to get its messages here." };
        let html = format!(
            "Started <b>{}</b> in {}. Reply to this message to write to it.{later}",
            html::escape(agent),
            html::escape(place)
        );
        let route = done.ids.pane.map(Route::Pane);
        self.say(html, Vec::new(), route, Some(message));
    }

    pub(super) fn telegram_event(&mut self, event: TelegramEvent, area: Rect) {
        let current = |app: &Self, id: u64| app.telegram.link.as_ref().is_some_and(|link| link.id == id);
        match event {
            TelegramEvent::Checked { token, result } => self.telegram_checked(&token, result),
            TelegramEvent::Link(link::Event::Updates(id, updates)) if current(self, id) => {
                for update in updates {
                    self.telegram_update(update, area);
                }
            }
            TelegramEvent::Link(link::Event::Sent { link, tag, message }) if current(self, link) => {
                if let Some(route) = self.telegram.tags.remove(&tag) {
                    self.telegram.remember(message, route);
                }
            }
            TelegramEvent::Link(link::Event::Problem(id, problem)) if current(self, id) => {
                self.telegram.phase = problem.map_or(Phase::Listening, Phase::Problem);
                self.refresh_telegram_note();
            }
            TelegramEvent::Link(link::Event::Rejected(id)) if current(self, id) => {
                self.telegram.link = None;
                self.telegram.phase = Phase::Rejected;
                self.refresh_telegram_note();
            }
            TelegramEvent::Link(_) => {}
        }
    }

    pub(super) fn check_telegram(&mut self, token: Secret) {
        let bot = Bot::new(&self.telegram.api, &token.0);
        let job = Job::new(Level::Info, "telegram", "check bot");
        self.spawn_job(
            job,
            move || Ok(bot.me().map_err(|e| e.to_string())),
            move |result, _| {
                let result = result.unwrap_or_else(|e| Err(e.to_string()));
                Some(AppEvent::Telegram(TelegramEvent::Checked { token, result }))
            },
        );
    }

    fn telegram_checked(&mut self, token: &Secret, result: Result<String, String>) {
        let saved = result.and_then(|name| {
            secrets::write(&self.telegram.secrets, telegram::TOKEN_KEY, &token.0)
                .map(|()| name)
                .map_err(|e| format!("failed to save the token: {e}"))
        });
        if let Ok(name) = &saved {
            log::info!("telegram", "bot set", bot = name);
            if self.telegram.saved.bot != *name {
                self.telegram.saved.chat = None;
            }
            self.telegram.saved.bot.clone_from(name);
            self.telegram.save();
            self.telegram.token = Some(token.0.trim().to_string());
            self.restart_link();
        }
        let paired = self.telegram.paired();
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.telegram_checked(saved, paired);
        }
        self.refresh_telegram_note();
    }

    pub(super) fn remove_telegram(&mut self) -> Result<&'static str, String> {
        secrets::remove(&self.telegram.secrets, telegram::TOKEN_KEY)
            .map_err(|e| format!("failed to remove the token: {e}"))?;
        log::info!("telegram", "bot removed");
        self.telegram.token = None;
        self.telegram.saved = Saved::default();
        self.telegram.save();
        self.telegram.away = false;
        self.telegram.phase = Phase::Off;
        self.sync_telegram();
        self.refresh_telegram_note();
        Ok("the Telegram bot was removed")
    }

    pub(super) fn forget_telegram_chat(&mut self) -> &'static str {
        log::info!("telegram", "chat forgotten");
        self.telegram.saved.chat = None;
        self.telegram.save();
        self.telegram.away = false;
        self.refresh_telegram_note();
        "this computer no longer messages your Telegram"
    }

    pub(super) fn pair_telegram(&mut self) -> Option<&'static str> {
        if self.telegram.bot().is_none() {
            return Some("paste the bot's token first");
        }
        match Pairing::new(Instant::now()) {
            Ok(pairing) => self.telegram.pairing = Some(pairing),
            Err(e) => log::error!("telegram", "cannot make a pairing code", error = e),
        }
        self.overlay = Some(Overlay::Telegram);
        None
    }

    fn try_pairing(&mut self, message: &telegram::Message) -> bool {
        let token = message.text.trim().strip_prefix("/start").map(str::trim).unwrap_or_default();
        let accepted = message.private
            && !token.is_empty()
            && self.telegram.pairing.as_ref().is_some_and(|p| p.accepts(token, Instant::now()));
        if !accepted {
            return false;
        }
        self.telegram.pairing = None;
        let paired = u64::try_from(unix_now()).unwrap_or_default();
        let chat = Chat { id: message.chat, user: message.user, name: message.name.clone(), paired };
        log::info!("telegram", "paired", chat = chat.id);
        self.telegram.saved.chat = Some(chat);
        self.telegram.save();
        if matches!(self.overlay, Some(Overlay::Telegram)) {
            self.overlay = None;
        }
        let who = if message.name.is_empty() { String::new() } else { format!(" with {}", message.name) };
        self.toast = Some(Toast::new(format!("{CONNECTED}{who}"), ui::ToastIcon::Check));
        self.say(chat::welcome(), Vec::new(), None, None);
        self.refresh_telegram_note();
        true
    }

    pub(super) fn telegram_view(&self) -> ui::RemoteAccess {
        let bot = self.telegram.bot().unwrap_or_default();
        let (url, lines) = match &self.telegram.pairing {
            Some(pairing) => (
                Some(format!("https://t.me/{bot}?start={}", pairing.token)),
                vec![
                    "scan the code with your phone's camera, then tap Start in Telegram.".into(),
                    "the bot then answers only that chat.".into(),
                ],
            ),
            None => (None, vec!["could not make a code, see server.log".into()]),
        };
        let code = url.as_deref().map(ui::remote_access::code).unwrap_or_default();
        ui::RemoteAccess { title: ui::TELEGRAM_TITLE, url, code, lines }
    }

    pub(super) fn telegram_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let done = ui::remote_done(area, &self.telegram_view());
        if ev.kind == MouseEventKind::Down(MouseButton::Left) && done.contains(pos) {
            self.overlay = None;
        }
    }

    pub(super) fn refresh_telegram_note(&mut self) {
        let view = self.telegram_settings();
        if let Some(Overlay::Settings(s)) = &mut self.overlay {
            s.telegram = view;
        }
    }

    pub(super) fn telegram_settings(&self) -> settings::Telegram {
        settings::Telegram {
            bot: self.telegram.bot().map(|name| format!("@{name}")),
            chat: self.telegram.chat_name().map(String::from),
            note: self.telegram.note(),
            checking: false,
        }
    }

    pub(super) fn away_button(&self) -> Option<bool> {
        self.telegram.paired().then_some(self.telegram.away)
    }

    pub(super) fn go_away(&mut self, by: By) -> Result<(), String> {
        if !self.telegram.paired() {
            return Err(NOT_PAIRED.into());
        }
        if !self.telegram.away {
            self.telegram.away = true;
            log::info!("telegram", "away", by = by.name());
        }
        let (status, keyboard) = self.status_message();
        self.say(format!("{AWAY_HTML}\n\n{status}"), keyboard, None, None);
        if by != By::Telegram {
            self.toast = Some(Toast::new(AWAY, ui::ToastIcon::Check));
        }
        self.refresh_telegram_note();
        Ok(())
    }

    pub(super) fn come_back(&mut self, by: By) {
        if !self.telegram.away {
            return;
        }
        self.telegram.away = false;
        log::info!("telegram", "back", by = by.name());
        let html = if by == By::Window { BACK_AT_HTML } else { BACK_HTML };
        self.say(html.into(), Vec::new(), None, None);
        if by == By::Command {
            self.toast = Some(Toast::new(BACK, ui::ToastIcon::Check));
        }
        self.refresh_telegram_note();
    }

    pub(in crate::app) fn away_request(&mut self, off: bool) -> Result<Option<Value>, String> {
        if off {
            self.come_back(By::Command);
        } else {
            self.go_away(By::Command)?;
        }
        Ok(Some(serde_json::json!({ "away": self.telegram.away })))
    }

    pub(super) fn returning(&self, event: &Event, area: Rect) -> bool {
        let used = match event {
            Event::Key(key) => key.kind == KeyEventKind::Press,
            Event::Paste(_) => true,
            Event::Mouse(mouse) => {
                let pressed = matches!(
                    mouse.kind,
                    MouseEventKind::Down(_) | MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                );
                let at = Position::new(mouse.column, mouse.row);
                pressed && !self.on_away_button(at, area)
            }
            _ => false,
        };
        self.telegram.away && used
    }

    fn on_away_button(&self, at: Position, area: Rect) -> bool {
        let areas = self.layout(area).shown(self.nav);
        self.away_button().is_some_and(|on| ui::away_button(areas.quit, on).contains(at))
    }
}

#[cfg(test)]
pub(super) fn capture(app: &mut App) -> std::sync::mpsc::Receiver<Order> {
    let (link, orders) = Link::capture(1);
    app.telegram.links = 1;
    app.telegram.link = Some(link);
    app.telegram.phase = Phase::Listening;
    orders
}

#[cfg(test)]
pub(super) fn set_up(app: &mut App, token: &str, bot: &str, chat: Option<Chat>) {
    app.telegram.token = Some(token.into());
    app.telegram.saved = Saved::new(bot.into(), chat);
}

#[cfg(test)]
pub(super) fn run(app: &App) -> &str {
    &app.telegram.run
}
