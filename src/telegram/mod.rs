use std::fmt;
use std::io;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::http::{self, Service};
use crate::persist;

pub(crate) mod html;
pub(crate) mod link;

pub(crate) const API: &str = "https://api.telegram.org";
pub(crate) const API_ENV: &str = "CORNERCASE_TELEGRAM_API";
pub(crate) const TOKEN_KEY: &str = "telegram_bot_token";
pub(crate) const COMMANDS: [(&str, &str); 4] = [
    ("status", "every agent and what it is doing"),
    ("new", "start an agent"),
    ("away", "message me here when an agent needs me"),
    ("back", "stop messaging me here"),
];
const VERSION: u32 = 1;
const CALL_TIMEOUT: Duration = Duration::from_secs(20);
const SERVICE: Service = Service { name: "Telegram", rejected: "Telegram refused the bot token" };
const MOST_CHOICES: usize = 9;
pub(crate) const MARKS: &str = "│┃|❯›>▶●○◯";
const DATA_BYTES: usize = 64;

#[derive(Clone)]
pub(crate) struct Bot {
    base: String,
    token: String,
}

impl fmt::Debug for Bot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bot").field("base", &self.base).finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    Rejected,
    Conflict,
    Wait(u64),
    Refused(String),
    Unreachable(String),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected => write!(f, "Telegram refused the bot token: paste it again from @BotFather"),
            Self::Conflict => write!(f, "another program reads this bot's messages: stop it, or make another bot"),
            Self::Wait(seconds) => write!(f, "Telegram asks to wait {seconds} s"),
            Self::Refused(why) => write!(f, "Telegram said: {why}"),
            Self::Unreachable(why) => write!(f, "{why}"),
        }
    }
}

impl Bot {
    pub(crate) fn new(base: &str, token: &str) -> Self {
        Self { base: base.trim_end_matches('/').to_string(), token: token.trim().to_string() }
    }

    fn scrub(&self, text: &str) -> String {
        if self.token.is_empty() { text.to_string() } else { text.replace(&self.token, "<token>") }
    }

    fn call(&self, method: &str, body: &Value, timeout: Duration) -> Result<Value, Failure> {
        let url = format!("{}/bot{}/{method}", self.base, self.token);
        let answer = http::post_within(&SERVICE, &url, body, timeout)
            .map_err(|e| Failure::Unreachable(self.scrub(&e.to_string())))?;
        if answer.json["ok"].as_bool() == Some(true) {
            return Ok(answer.json["result"].clone());
        }
        let said =
            answer.json["description"].as_str().map_or_else(|| format!("status {}", answer.status), String::from);
        Err(match answer.status {
            401 | 404 => Failure::Rejected,
            409 => Failure::Conflict,
            429 => Failure::Wait(answer.json["parameters"]["retry_after"].as_u64().unwrap_or(5)),
            _ => Failure::Refused(self.scrub(&said)),
        })
    }

    pub(crate) fn me(&self) -> Result<String, Failure> {
        let me = self.call("getMe", &json!({}), CALL_TIMEOUT)?;
        let name = me["username"].as_str().filter(|name| !name.is_empty()).map(String::from);
        name.ok_or_else(|| Failure::Refused("the bot has no username".into()))
    }

    pub(crate) fn updates(&self, offset: i64, wait: Duration) -> Result<Vec<Value>, Failure> {
        let body = json!({
            "offset": offset,
            "timeout": wait.as_secs(),
            "allowed_updates": ["message", "callback_query"],
        });
        let result = self.call("getUpdates", &body, wait + CALL_TIMEOUT)?;
        Ok(result.as_array().cloned().unwrap_or_default())
    }

    pub(crate) fn perform(&self, order: &Order, rich: bool) -> Result<Option<i64>, Failure> {
        let (method, body) = order.request(rich);
        let result = self.call(method, &body, CALL_TIMEOUT)?;
        Ok(result["message_id"].as_i64())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Key {
    pub(crate) text: String,
    pub(crate) data: String,
}

pub(crate) type Keyboard = Vec<Vec<Key>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Order {
    Send { chat: i64, html: String, keyboard: Keyboard, reply_to: Option<i64>, ask: Option<String>, tag: Option<u64> },
    Edit { chat: i64, message: i64, html: String, keyboard: Keyboard },
    Buttons { chat: i64, message: i64, keyboard: Keyboard },
    Answer { id: String, text: String },
    React { chat: i64, message: i64, emoji: String },
    Commands,
}

fn markup(keyboard: &Keyboard) -> Value {
    let rows: Vec<Value> = keyboard
        .iter()
        .map(|row| Value::Array(row.iter().map(|k| json!({"text": k.text, "callback_data": k.data})).collect()))
        .collect();
    json!({ "inline_keyboard": rows })
}

fn text(body: &mut Value, html: &str, rich: bool) {
    if rich {
        body["text"] = json!(html);
        body["parse_mode"] = json!("HTML");
    } else {
        body["text"] = json!(html::plain(html));
    }
    body["link_preview_options"] = json!({ "is_disabled": true });
}

impl Order {
    pub(crate) fn method(&self) -> &'static str {
        match self {
            Self::Send { .. } => "sendMessage",
            Self::Edit { .. } => "editMessageText",
            Self::Buttons { .. } => "editMessageReplyMarkup",
            Self::Answer { .. } => "answerCallbackQuery",
            Self::React { .. } => "setMessageReaction",
            Self::Commands => "setMyCommands",
        }
    }

    pub(crate) fn tag(&self) -> Option<u64> {
        match self {
            Self::Send { tag, .. } => *tag,
            _ => None,
        }
    }

    pub(crate) fn request(&self, rich: bool) -> (&'static str, Value) {
        let body = match self {
            Self::Send { chat, html, keyboard, reply_to, ask, .. } => {
                let mut body = json!({ "chat_id": chat });
                text(&mut body, html, rich);
                if let Some(id) = reply_to {
                    body["reply_parameters"] = json!({ "message_id": id, "allow_sending_without_reply": true });
                }
                if let Some(placeholder) = ask {
                    body["reply_markup"] = json!({ "force_reply": true, "input_field_placeholder": placeholder });
                } else if !keyboard.is_empty() {
                    body["reply_markup"] = markup(keyboard);
                }
                body
            }
            Self::Edit { chat, message, html, keyboard } => {
                let mut body = json!({ "chat_id": chat, "message_id": message, "reply_markup": markup(keyboard) });
                text(&mut body, html, rich);
                body
            }
            Self::Buttons { chat, message, keyboard } => {
                json!({ "chat_id": chat, "message_id": message, "reply_markup": markup(keyboard) })
            }
            Self::Answer { id, text } => json!({ "callback_query_id": id, "text": text }),
            Self::React { chat, message, emoji } => {
                json!({ "chat_id": chat, "message_id": message, "reaction": [{ "type": "emoji", "emoji": emoji }] })
            }
            Self::Commands => {
                let commands: Vec<Value> = COMMANDS
                    .iter()
                    .map(|(command, about)| json!({ "command": command, "description": about }))
                    .collect();
                json!({ "commands": commands })
            }
        };
        (self.method(), body)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Message {
    pub(crate) id: i64,
    pub(crate) chat: i64,
    pub(crate) user: i64,
    pub(crate) private: bool,
    pub(crate) date: i64,
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) reply_to: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Press {
    pub(crate) id: String,
    pub(crate) chat: i64,
    pub(crate) user: i64,
    pub(crate) message: i64,
    pub(crate) data: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Update {
    Message(Message),
    Press(Press),
}

impl Update {
    pub(crate) fn date(&self) -> Option<i64> {
        match self {
            Self::Message(message) => Some(message.date),
            Self::Press(_) => None,
        }
    }
}

pub(crate) fn parse(values: &[Value]) -> (Vec<Update>, Option<i64>) {
    let next = values.iter().filter_map(|v| v["update_id"].as_i64()).max().map(|id| id + 1);
    (values.iter().filter_map(update).collect(), next)
}

fn update(value: &Value) -> Option<Update> {
    if let Some(m) = value.get("message") {
        let from = &m["from"];
        let name = from["username"].as_str().map(|u| format!("@{u}"));
        let name = name.or_else(|| from["first_name"].as_str().map(String::from)).unwrap_or_default();
        return Some(Update::Message(Message {
            id: m["message_id"].as_i64()?,
            chat: m["chat"]["id"].as_i64()?,
            user: from["id"].as_i64()?,
            private: m["chat"]["type"].as_str() == Some("private"),
            date: m["date"].as_i64().unwrap_or_default(),
            name,
            text: m["text"].as_str().unwrap_or_default().to_string(),
            reply_to: m["reply_to_message"]["message_id"].as_i64(),
        }));
    }
    let press = value.get("callback_query")?;
    Some(Update::Press(Press {
        id: press["id"].as_str()?.to_string(),
        chat: press["message"]["chat"]["id"].as_i64()?,
        user: press["from"]["id"].as_i64()?,
        message: press["message"]["message_id"].as_i64()?,
        data: press["data"].as_str().unwrap_or_default().to_string(),
    }))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Chat {
    pub(crate) id: i64,
    pub(crate) user: i64,
    pub(crate) name: String,
    pub(crate) paired: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Saved {
    version: u32,
    pub(crate) bot: String,
    pub(crate) chat: Option<Chat>,
}

impl Saved {
    #[cfg(test)]
    pub(crate) fn new(bot: String, chat: Option<Chat>) -> Self {
        Self { version: VERSION, bot, chat }
    }

    pub(crate) fn load(path: &Path) -> Self {
        let saved = std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str::<Self>(&text).ok());
        saved.filter(|saved| saved.version == VERSION).unwrap_or_default()
    }

    pub(crate) fn save(&self, path: &Path) -> io::Result<()> {
        persist::write(path, &Self { version: VERSION, ..self.clone() }, true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Choice {
    Project(u64),
    Workspace(u64),
    Worktree,
    Agent(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Tap {
    Key(u64, String),
    Screen(u64),
    Write(u64),
    Deliver(u64, u32),
    New,
    Draft(u32, Choice),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unread {
    Stale,
    Unknown,
}

impl Tap {
    pub(crate) fn data(&self, run: &str) -> String {
        match self {
            Self::Key(pane, key) => format!("{run}:k:{pane}:{key}"),
            Self::Screen(pane) => format!("{run}:s:{pane}"),
            Self::Write(pane) => format!("{run}:w:{pane}"),
            Self::Deliver(pane, text) => format!("{run}:d:{pane}:{text}"),
            Self::New => format!("{run}:n"),
            Self::Draft(draft, Choice::Project(id)) => format!("{run}:np:{draft}:{id}"),
            Self::Draft(draft, Choice::Workspace(id)) => format!("{run}:nw:{draft}:{id}"),
            Self::Draft(draft, Choice::Worktree) => format!("{run}:nt:{draft}"),
            Self::Draft(draft, Choice::Agent(kind)) => format!("{run}:na:{draft}:{kind}"),
        }
    }

    pub(crate) fn fits(&self, run: &str) -> bool {
        self.data(run).len() <= DATA_BYTES
    }

    pub(crate) fn read(data: &str, run: &str) -> Result<Self, Unread> {
        let mut parts = data.splitn(4, ':');
        if parts.next() != Some(run) {
            return Err(Unread::Stale);
        }
        let verb = parts.next().unwrap_or_default();
        let first = parts.next().and_then(|n| n.parse::<u64>().ok());
        let second = parts.next();
        let draft = || first.and_then(|n| u32::try_from(n).ok());
        let tap = match (verb, first, second) {
            ("k", Some(pane), Some(key)) => Some(Self::Key(pane, key.to_string())),
            ("s", Some(pane), None) => Some(Self::Screen(pane)),
            ("w", Some(pane), None) => Some(Self::Write(pane)),
            ("d", Some(pane), Some(text)) => text.parse().ok().map(|text| Self::Deliver(pane, text)),
            ("n", None, None) => Some(Self::New),
            ("np", _, Some(id)) => draft().zip(id.parse().ok()).map(|(d, id)| Self::Draft(d, Choice::Project(id))),
            ("nw", _, Some(id)) => draft().zip(id.parse().ok()).map(|(d, id)| Self::Draft(d, Choice::Workspace(id))),
            ("nt", _, None) => draft().map(|d| Self::Draft(d, Choice::Worktree)),
            ("na", _, Some(kind)) => draft().map(|d| Self::Draft(d, Choice::Agent(kind.to_string()))),
            _ => None,
        };
        tap.ok_or(Unread::Unknown)
    }
}

pub(crate) fn choices(screen: &str) -> Vec<(char, String)> {
    let lines: Vec<&str> = screen.lines().rev().filter(|line| !line.trim().is_empty()).take(15).collect();
    let mut found: Vec<(char, String)> = Vec::new();
    for line in lines {
        let line = line.trim_matches(|c: char| c.is_whitespace() || MARKS.contains(c));
        let mut chars = line.chars();
        let (Some(digit), Some('.')) = (chars.next(), chars.next()) else { continue };
        let label = chars.as_str().trim();
        if !('1'..='9').contains(&digit) || label.is_empty() || found.iter().any(|(d, _)| *d == digit) {
            continue;
        }
        found.push((digit, label.to_string()));
        if digit == '1' || found.len() == MOST_CHOICES {
            break;
        }
    }
    found.reverse();
    found
}

#[cfg(test)]
mod tests;
