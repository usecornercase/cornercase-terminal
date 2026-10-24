use std::cell::{Cell, RefCell};
use std::fmt;
use std::thread;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD};
use serde_json::{Value, json};

use crate::http::{self, Method, Service};
use crate::messenger::link::{Next, Transport};
use crate::messenger::{Advice, COMMANDS, Chat, Key, Keyboard, Message, Order, Press, Update};
use crate::time;
use crate::version;

pub(crate) mod forum;
pub(crate) mod gateway;
pub(crate) mod markdown;

pub(crate) const API: &str = "https://discord.com/api/v10";
pub(crate) const API_ENV: &str = "CORNERCASE_DISCORD_API";
pub(crate) const TOKEN_KEY: &str = "discord_bot_token";
pub(crate) const DATA_BYTES: usize = 100;
const CANNOT_MESSAGE: i64 = 50007;
const MISSING_PERMISSION: i64 = 50013;
const MISSING_ACCESS: i64 = 50001;
const UNKNOWN_CHANNEL: i64 = 10003;
const UNKNOWN_GUILD: i64 = 10004;
const THREAD_ARCHIVED: i64 = 50083;
const CALL_TIMEOUT: Duration = Duration::from_secs(20);
const ACK_TIMEOUT: Duration = Duration::from_secs(3);
const ACK_WAIT: u64 = 1;
const SERVICE: Service = Service { name: "Discord", rejected: "Discord refused the bot token" };
const ROWS: usize = 5;
const BUTTONS: usize = 5;
const LABEL: usize = 80;
const MORE: &str = "…";
const ASK: &str = "-# reply to this message";
const EPHEMERAL: u64 = 64;
const DEFERRED_UPDATE: u64 = 6;
const DEFERRED_MESSAGE: u64 = 5;
const SLASH: u64 = 1;
const USER_INSTALL: u64 = 1;
const BOT_DM: u64 = 1;
const DISCORD_EPOCH_MS: i64 = 1_420_070_400_000;
pub(crate) const NO_USER_INSTALL: &str = "turn on User Install in the Developer Portal";
pub(crate) const PUBLIC: &str = "public app: Install Link to None, then Public Bot off";

#[derive(Clone)]
pub(crate) struct Bot {
    base: String,
    token: String,
    dm: Option<i64>,
    forum: bool,
}

impl fmt::Debug for Bot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bot").field("base", &self.base).finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    Rejected,
    Closed,
    Wait(u64),
    Missing(&'static str),
    Removed,
    Lost,
    Archived,
    Refused(String),
    Unreachable(String),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected => write!(f, "Discord refused the bot token: paste it again from the Developer Portal"),
            Self::Closed => write!(f, "Discord cannot message you yet: add the app to your account first"),
            Self::Wait(seconds) => write!(f, "Discord asks to wait {seconds} s"),
            Self::Missing(permission) => write!(f, "missing permission: {permission}"),
            Self::Removed => write!(f, "the app is not in the server: add it again"),
            Self::Lost => write!(f, "the channel is gone from Discord"),
            Self::Archived => write!(f, "the post is archived"),
            Self::Refused(why) => write!(f, "Discord said: {why}"),
            Self::Unreachable(why) => write!(f, "{why}"),
        }
    }
}

impl From<String> for Failure {
    fn from(why: String) -> Self {
        Self::Unreachable(why)
    }
}

#[derive(Debug, Default)]
pub(crate) struct Sent {
    calls: Cell<usize>,
    ids: RefCell<Vec<i64>>,
    thread: Cell<Option<i64>>,
}

#[derive(Debug, Clone, PartialEq)]
struct Call {
    method: Method,
    path: String,
    body: Option<Value>,
}

impl Call {
    fn new(method: Method, path: String, body: Option<Value>) -> Self {
        Self { method, path, body }
    }
}

fn application_id(token: &str) -> Option<String> {
    let first = token.trim().split('.').next()?;
    let bytes = STANDARD_NO_PAD
        .decode(first.trim_end_matches('='))
        .or_else(|_| URL_SAFE_NO_PAD.decode(first.trim_end_matches('=')))
        .ok()?;
    let id = String::from_utf8(bytes).ok()?;
    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).then_some(id)
}

pub(crate) fn install_link(token: &str) -> Option<String> {
    application_id(token).map(|id| {
        format!("https://discord.com/oauth2/authorize?client_id={id}&integration_type=1&scope=applications.commands")
    })
}

pub(crate) fn advice(app: &Value) -> Option<&'static str> {
    let installs = &app["integration_types_config"];
    if installs.is_object() && installs.get("1").is_none_or(Value::is_null) {
        return Some(NO_USER_INSTALL);
    }
    (app["bot_public"].as_bool() == Some(true)).then_some(PUBLIC)
}

fn snowflake(value: &Value) -> Option<i64> {
    value.as_str().and_then(|id| id.parse().ok()).or_else(|| value.as_i64())
}

impl Bot {
    pub(crate) fn new(base: &str, token: &str) -> Self {
        let (base, token) = (base.trim_end_matches('/').to_string(), token.trim().to_string());
        Self { base, token, dm: None, forum: false }
    }

    pub(crate) fn posting(self, dm: Option<i64>) -> Self {
        Self { dm, ..self }
    }

    fn scoped(&self, forum: bool) -> Self {
        Self { forum, ..self.clone() }
    }

    fn on_forum(&self, order: &Order) -> bool {
        match order {
            Order::Post { .. }
            | Order::Retag { .. }
            | Order::Close { .. }
            | Order::Reopen { .. }
            | Order::Remove { .. } => true,
            Order::Send { chat, .. }
            | Order::Edit { chat, .. }
            | Order::Buttons { chat, .. }
            | Order::React { chat, .. } => self.dm.is_some_and(|dm| dm != *chat),
            Order::Answer { .. } | Order::Commands => false,
        }
    }

    fn scrub(&self, text: &str) -> String {
        if self.token.is_empty() { text.to_string() } else { text.replace(&self.token, "<token>") }
    }

    fn call(&self, method: Method, path: &str, body: Option<&Value>) -> Result<Value, Failure> {
        self.call_within(method, path, body, CALL_TIMEOUT)
    }

    fn call_within(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        within: Duration,
    ) -> Result<Value, Failure> {
        let url = format!("{}{path}", self.base);
        let auth = format!("Bot {}", self.token);
        let agent = format!("DiscordBot (https://usecornercase.dev, {})", version::CURRENT);
        let headers = [("Authorization", auth.as_str()), ("User-Agent", agent.as_str())];
        let call = http::Call { method, url: &url, headers: &headers, body };
        let answer =
            http::call_within(&SERVICE, call, within).map_err(|e| Failure::Unreachable(self.scrub(&e.to_string())))?;
        if answer.ok() {
            return Ok(answer.json);
        }
        Err(match (answer.status, answer.json["code"].as_i64()) {
            (401, _) => Failure::Rejected,
            (429, _) => Failure::Wait(retry_after(&answer.json)),
            (_, Some(CANNOT_MESSAGE)) => Failure::Closed,
            (_, Some(MISSING_PERMISSION)) => Failure::Missing(forum::SOME_PERMISSION),
            (_, Some(MISSING_ACCESS | UNKNOWN_GUILD)) if self.forum => Failure::Removed,
            (_, Some(UNKNOWN_CHANNEL)) if self.forum => Failure::Lost,
            (_, Some(THREAD_ARCHIVED)) => Failure::Archived,
            (status, code) => {
                let said = answer.json["message"].as_str().map_or_else(|| format!("status {status}"), String::from);
                let said = code.map_or(said.clone(), |code| format!("{said} (code {code})"));
                Failure::Refused(self.scrub(&said))
            }
        })
    }

    pub(crate) fn me(&self) -> Result<String, Failure> {
        let me = self.call(Method::Get, "/users/@me", None)?;
        let name = me["username"].as_str().filter(|name| !name.is_empty()).map(String::from);
        name.ok_or_else(|| Failure::Refused("the bot has no username".into()))
    }

    fn application(&self) -> Result<String, Failure> {
        if let Some(id) = application_id(&self.token) {
            return Ok(id);
        }
        let me = self.call(Method::Get, "/users/@me", None)?;
        me["id"].as_str().map(String::from).ok_or_else(|| Failure::Refused("the bot has no id".into()))
    }

    fn app(&self) -> Result<Value, Failure> {
        self.call(Method::Get, "/oauth2/applications/@me", None)
    }

    pub(crate) fn read_advice(&self) -> Result<Option<&'static str>, Failure> {
        self.app().map(|app| advice(&app))
    }

    fn owner(app: &Value) -> Result<(i64, String), Failure> {
        let team = &app["team"];
        let found = if team.is_object() {
            snowflake(&team["owner_user_id"]).map(|id| {
                let members = team["members"].as_array().cloned().unwrap_or_default();
                let member = members.iter().find(|m| snowflake(&m["user"]["id"]) == Some(id));
                let name = member.and_then(|m| m["user"]["username"].as_str()).or_else(|| team["name"].as_str());
                (id, name.unwrap_or_default().to_string())
            })
        } else {
            snowflake(&app["owner"]["id"])
                .map(|id| (id, app["owner"]["username"].as_str().unwrap_or_default().to_string()))
        };
        found.ok_or_else(|| Failure::Refused("the app has no owner".into()))
    }

    fn open_dm(&self, user: i64) -> Result<i64, Failure> {
        let channel =
            self.call(Method::Post, "/users/@me/channels", Some(&json!({ "recipient_id": user.to_string() })))?;
        snowflake(&channel["id"]).ok_or_else(|| Failure::Refused("no direct message channel".into()))
    }

    pub(crate) fn pair(&self, welcome: &str) -> (Advice, Result<Chat, Failure>) {
        match self.app() {
            Ok(app) => (Advice::Read(advice(&app)), self.write_owner(&app, welcome)),
            Err(failure) => (Advice::Unread, Err(failure)),
        }
    }

    fn write_owner(&self, app: &Value, welcome: &str) -> Result<Chat, Failure> {
        let (user, name) = Self::owner(app)?;
        let channel = self.open_dm(user)?;
        let body = json!({ "content": welcome, "allowed_mentions": { "parse": [] } });
        self.call(Method::Post, &format!("/channels/{channel}/messages"), Some(&body))?;
        let paired = u64::try_from(time::now()).unwrap_or_default();
        Ok(Chat { id: channel, user, name, paired })
    }

    fn gateway(&self) -> Result<String, Failure> {
        let gateway = self.call(Method::Get, "/gateway/bot", None)?;
        let url = gateway["url"].as_str().filter(|url| !url.is_empty()).map(String::from);
        url.ok_or_else(|| Failure::Refused("no gateway address".into()))
    }

    fn ack(&self, id: &str) -> Result<(), Failure> {
        self.callback(id, &json!({ "type": DEFERRED_UPDATE }))
    }

    fn acknowledge(&self, invoked: &Invoked) -> Result<(), Failure> {
        self.callback(&invoked.id, &json!({ "type": DEFERRED_MESSAGE, "data": { "flags": EPHEMERAL } }))
    }

    fn dismiss(&self, invoked: &Invoked) -> Result<(), Failure> {
        let app = match &invoked.app {
            Some(app) => app.clone(),
            None => self.application()?,
        };
        let token = invoked.id.split_once(':').map_or("", |(_, token)| token);
        self.call_within(Method::Delete, &format!("/webhooks/{app}/{token}/messages/@original"), None, ACK_TIMEOUT)
            .map(|_| ())
    }

    fn callback(&self, id: &str, body: &Value) -> Result<(), Failure> {
        let (interaction, token) = id.split_once(':').unwrap_or((id, ""));
        let path = format!("/interactions/{interaction}/{token}/callback");
        match self.call_within(Method::Post, &path, Some(body), ACK_TIMEOUT) {
            Err(Failure::Wait(seconds)) if seconds <= ACK_WAIT => {
                thread::sleep(Duration::from_secs(seconds));
                self.call_within(Method::Post, &path, Some(body), ACK_TIMEOUT).map(|_| ())
            }
            done => done.map(|_| ()),
        }
    }
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to a positive hour first")]
fn retry_after(json: &Value) -> u64 {
    json["retry_after"].as_f64().unwrap_or(5.0).clamp(0.0, 3600.0).ceil() as u64
}

impl Transport for Bot {
    type Failure = Failure;
    type Attempt = Sent;

    fn perform(&self, order: &Order, attempt: &Sent) -> Result<Option<i64>, Failure> {
        let follows_up = matches!(order, Order::Answer { text, .. } if !text.is_empty());
        let app = if follows_up || matches!(order, Order::Commands) { self.application()? } else { String::new() };
        let bot = self.scoped(self.on_forum(order));
        while let Some(call) = request(order, &app, attempt.thread.get()).into_iter().nth(attempt.calls.get()) {
            let answer = bot.call(call.method, &call.path, call.body.as_ref()).map_err(|f| forum::needs(f, order))?;
            record(order, &answer, attempt);
            attempt.calls.set(attempt.calls.get() + 1);
        }
        Ok(attempt.ids.borrow().first().copied())
    }

    fn more(attempt: &Sent) -> Vec<i64> {
        attempt.ids.borrow().iter().skip(1).copied().collect()
    }

    fn posted(attempt: &Sent) -> Option<i64> {
        attempt.thread.get()
    }

    fn gone(order: &Order, failure: &Failure) -> Option<i64> {
        if *failure != Failure::Lost {
            return None;
        }
        match order {
            Order::Send { chat, .. }
            | Order::Edit { chat, .. }
            | Order::Buttons { chat, .. }
            | Order::React { chat, .. } => Some(*chat),
            Order::Retag { thread, .. }
            | Order::Close { thread, .. }
            | Order::Reopen { thread }
            | Order::Remove { thread } => Some(*thread),
            Order::Answer { .. } | Order::Commands | Order::Post { .. } => None,
        }
    }

    fn archived(order: &Order, failure: &Failure) -> Option<i64> {
        match (order, failure) {
            (Order::Retag { thread, .. } | Order::Close { thread, .. }, Failure::Archived) => Some(*thread),
            _ => None,
        }
    }

    fn next(failure: Failure, _: &mut Sent) -> Next<Failure> {
        match failure {
            Failure::Wait(seconds) => Next::Wait(seconds),
            failure => Next::Done(Err(failure)),
        }
    }

    fn method(order: &Order) -> &'static str {
        match order {
            Order::Send { .. } => "send",
            Order::Edit { .. } => "edit",
            Order::Buttons { .. } => "buttons",
            Order::Answer { .. } => "answer",
            Order::React { .. } => "react",
            Order::Commands => "commands",
            Order::Post { .. } => "post",
            Order::Retag { .. } => "retag",
            Order::Close { .. } => "close",
            Order::Reopen { .. } => "reopen",
            Order::Remove { .. } => "remove",
        }
    }
}

fn record(order: &Order, answer: &Value, attempt: &Sent) {
    let Some(id) = snowflake(&answer["id"]) else { return };
    match order {
        Order::Post { .. } if attempt.thread.get().is_none() => {
            attempt.thread.set(Some(id));
            attempt.ids.borrow_mut().push(snowflake(&answer["message"]["id"]).unwrap_or(id));
        }
        Order::Send { .. } | Order::Post { .. } => attempt.ids.borrow_mut().push(id),
        _ => {}
    }
}

fn button(key: &Key) -> Value {
    let label: String = key.text.chars().take(LABEL).collect();
    let label = if label.trim().is_empty() { "·".to_string() } else { label };
    json!({ "type": 2, "style": 2, "label": label, "custom_id": key.data })
}

fn pages(keyboard: &Keyboard) -> Vec<Vec<Value>> {
    let rows: Vec<Value> = keyboard
        .iter()
        .flat_map(|row| row.chunks(BUTTONS))
        .map(|keys| json!({ "type": 1, "components": keys.iter().map(button).collect::<Vec<_>>() }))
        .collect();
    rows.chunks(ROWS).map(<[Value]>::to_vec).collect()
}

fn asking(text: &str) -> String {
    let room = markdown::MAX.saturating_sub(markdown::length(ASK) + 1);
    let text: String = text.chars().take(room).collect();
    format!("{text}\n{ASK}")
}

fn message(content: &str, components: &[Value]) -> Value {
    json!({ "content": content, "components": components, "allowed_mentions": { "parse": [] } })
}

fn request(order: &Order, app: &str, thread: Option<i64>) -> Vec<Call> {
    match order {
        Order::Send { chat, text, keyboard, reply_to, ask, .. } => {
            sending(*chat, text, keyboard, *reply_to, ask.is_some())
        }
        Order::Edit { chat, message: id, text, keyboard } => {
            let body = message(text, pages(keyboard).first().map_or(&[], Vec::as_slice));
            vec![Call::new(Method::Patch, format!("/channels/{chat}/messages/{id}"), Some(body))]
        }
        Order::Buttons { chat, message: id, keyboard } => {
            let body = json!({ "components": pages(keyboard).into_iter().next().unwrap_or_default() });
            vec![Call::new(Method::Patch, format!("/channels/{chat}/messages/{id}"), Some(body))]
        }
        Order::Answer { id, text } if !text.is_empty() => {
            let token = id.split_once(':').map_or("", |(_, token)| token);
            let body = json!({ "content": text, "flags": EPHEMERAL, "allowed_mentions": { "parse": [] } });
            vec![Call::new(Method::Post, format!("/webhooks/{app}/{token}"), Some(body))]
        }
        Order::React { chat, message: id, emoji } => {
            let path = format!("/channels/{chat}/messages/{id}/reactions/{}/@me", http::path_segment(emoji));
            vec![Call::new(Method::Put, path, None)]
        }
        Order::Commands => vec![Call::new(Method::Put, format!("/applications/{app}/commands"), Some(command_list()))],
        Order::Post { forum, title, tag_id, text, keyboard, .. } => {
            forum::posting(&forum::Post { forum: *forum, title, tag_id: *tag_id, text, keyboard }, thread)
        }
        Order::Retag { thread, tag_id } => vec![forum::retagging(*thread, *tag_id)],
        Order::Close { thread, lock } => vec![forum::archiving(*thread, true, *lock)],
        Order::Reopen { thread } => vec![forum::archiving(*thread, false, false)],
        Order::Remove { thread } => vec![forum::removing(*thread)],
        Order::Answer { .. } => Vec::new(),
    }
}

fn sending(chat: i64, text: &str, keyboard: &Keyboard, reply_to: Option<i64>, ask: bool) -> Vec<Call> {
    let path = format!("/channels/{chat}/messages");
    let mut pages = pages(keyboard).into_iter();
    let content = if ask { asking(text) } else { text.to_string() };
    let mut first = message(&content, &pages.next().unwrap_or_default());
    if let Some(id) = reply_to {
        first["message_reference"] = json!({ "message_id": id.to_string(), "fail_if_not_exists": false });
    }
    let mut calls = vec![Call::new(Method::Post, path.clone(), Some(first))];
    calls.extend(pages.map(|page| Call::new(Method::Post, path.clone(), Some(message(MORE, &page)))));
    calls
}

fn command_list() -> Value {
    let commands: Vec<Value> = COMMANDS
        .iter()
        .map(|(name, about)| {
            json!({ "name": name, "description": about, "type": SLASH,
                "integration_types": [USER_INSTALL], "contexts": [BOT_DM] })
        })
        .collect();
    Value::Array(commands)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Invoked {
    id: String,
    app: Option<String>,
}

fn invoked(kind: &str, d: &Value) -> Option<Invoked> {
    if kind != "INTERACTION_CREATE" || d["type"].as_u64() != Some(2) {
        return None;
    }
    let id = format!("{}:{}", d["id"].as_str()?, d["token"].as_str()?);
    Some(Invoked { id, app: d["application_id"].as_str().map(String::from) })
}

pub(crate) fn update(kind: &str, d: &Value, forum: Option<i64>) -> Option<Update> {
    match kind {
        "MESSAGE_CREATE" => written(d, forum),
        "INTERACTION_CREATE" if d["type"].as_u64() == Some(2) => slash_command(d),
        "INTERACTION_CREATE" if d["type"].as_u64() == Some(3) => pressed(d),
        _ => None,
    }
}

fn written(d: &Value, forum: Option<i64>) -> Option<Update> {
    let author = &d["author"];
    let ordinary = matches!(d["type"].as_u64(), None | Some(0 | 19));
    let in_guild = d.get("guild_id").is_some_and(|g| !g.is_null());
    let guild = snowflake(&d["guild_id"]).filter(|guild| Some(*guild) == forum);
    if (in_guild && guild.is_none()) || author["bot"].as_bool() == Some(true) || !ordinary {
        return None;
    }
    Some(Update::Message(Message {
        id: snowflake(&d["id"])?,
        chat: snowflake(&d["channel_id"])?,
        user: snowflake(&author["id"])?,
        private: !in_guild,
        guild,
        date: d["timestamp"].as_str().and_then(time::parse_time).unwrap_or_default(),
        name: author["username"].as_str().unwrap_or_default().to_string(),
        text: d["content"].as_str().unwrap_or_default().to_string(),
        reply_to: snowflake(&d["message_reference"]["message_id"]),
    }))
}

fn slash_command(d: &Value) -> Option<Update> {
    let user = &d["user"];
    let user = if user.is_object() { user } else { &d["member"]["user"] };
    let id = snowflake(&d["id"])?;
    Some(Update::Message(Message {
        id,
        chat: snowflake(&d["channel_id"]).or_else(|| snowflake(&d["channel"]["id"]))?,
        user: snowflake(&user["id"])?,
        private: true,
        guild: None,
        date: ((id >> 22) + DISCORD_EPOCH_MS) / 1000,
        name: user["username"].as_str().unwrap_or_default().to_string(),
        text: format!("/{}", d["data"]["name"].as_str().filter(|name| !name.is_empty())?),
        reply_to: None,
    }))
}

fn pressed(d: &Value) -> Option<Update> {
    let user = snowflake(&d["user"]["id"]).or_else(|| snowflake(&d["member"]["user"]["id"]))?;
    let chat = snowflake(&d["channel_id"]).or_else(|| snowflake(&d["channel"]["id"]))?;
    Some(Update::Press(Press {
        id: format!("{}:{}", d["id"].as_str()?, d["token"].as_str()?),
        chat,
        user,
        message: snowflake(&d["message"]["id"])?,
        data: d["data"]["custom_id"].as_str().unwrap_or_default().to_string(),
    }))
}

#[cfg(test)]
mod tests;
