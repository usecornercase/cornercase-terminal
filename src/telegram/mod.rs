use std::fmt;
use std::time::Duration;

use serde_json::{Value, json};

use crate::http::{self, Service};
use crate::messenger::link::{Next, Transport};
use crate::messenger::{COMMANDS, Keyboard, Message, Order, Press, Update};

pub(crate) mod html;
pub(crate) mod link;

pub(crate) const API: &str = "https://api.telegram.org";
pub(crate) const API_ENV: &str = "CORNERCASE_TELEGRAM_API";
pub(crate) const TOKEN_KEY: &str = "telegram_bot_token";
pub(crate) const DATA_BYTES: usize = 64;
const CALL_TIMEOUT: Duration = Duration::from_secs(20);
const SERVICE: Service = Service { name: "Telegram", rejected: "Telegram refused the bot token" };
const NOT_MODIFIED: &str = "message is not modified";
const NO_ENTITIES: &str = "can't parse entities";

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

#[derive(Debug, Default)]
pub(crate) struct Plain(bool);

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

impl From<String> for Failure {
    fn from(why: String) -> Self {
        Self::Unreachable(why)
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

    fn send_order(&self, order: &Order, rich: bool) -> Result<Option<i64>, Failure> {
        let (method, body) = request(order, rich);
        let result = self.call(method, &body, CALL_TIMEOUT)?;
        Ok(result["message_id"].as_i64())
    }
}

impl Transport for Bot {
    type Failure = Failure;
    type Attempt = Plain;

    fn perform(&self, order: &Order, attempt: &Plain) -> Result<Option<i64>, Failure> {
        if matches!(
            order,
            Order::Post { .. }
                | Order::Retag { .. }
                | Order::Close { .. }
                | Order::Reopen { .. }
                | Order::Remove { .. }
        ) {
            return Ok(None);
        }
        self.send_order(order, !attempt.0)
    }

    fn next(failure: Failure, attempt: &mut Plain) -> Next<Failure> {
        match failure {
            Failure::Wait(seconds) => Next::Wait(seconds),
            Failure::Refused(why) if !attempt.0 && why.contains(NO_ENTITIES) => {
                attempt.0 = true;
                Next::Again
            }
            Failure::Refused(why) if why.contains(NOT_MODIFIED) => Next::Done(Ok(None)),
            failure => Next::Done(Err(failure)),
        }
    }

    fn method(order: &Order) -> &'static str {
        method(order)
    }
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

fn method(order: &Order) -> &'static str {
    match order {
        Order::Send { .. } => "sendMessage",
        Order::Edit { .. } => "editMessageText",
        Order::Buttons { .. } => "editMessageReplyMarkup",
        Order::Answer { .. } => "answerCallbackQuery",
        Order::React { .. } => "setMessageReaction",
        Order::Commands => "setMyCommands",
        Order::Post { .. }
        | Order::Retag { .. }
        | Order::Close { .. }
        | Order::Reopen { .. }
        | Order::Remove { .. } => "unsupported",
    }
}

fn request(order: &Order, rich: bool) -> (&'static str, Value) {
    let body = match order {
        Order::Send { chat, text: html, keyboard, reply_to, ask, .. } => {
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
        Order::Edit { chat, message, text: html, keyboard } => {
            let mut body = json!({ "chat_id": chat, "message_id": message, "reply_markup": markup(keyboard) });
            text(&mut body, html, rich);
            body
        }
        Order::Buttons { chat, message, keyboard } => {
            json!({ "chat_id": chat, "message_id": message, "reply_markup": markup(keyboard) })
        }
        Order::Answer { id, text } => json!({ "callback_query_id": id, "text": text }),
        Order::React { chat, message, emoji } => {
            json!({ "chat_id": chat, "message_id": message, "reaction": [{ "type": "emoji", "emoji": emoji }] })
        }
        Order::Commands => {
            let commands: Vec<Value> =
                COMMANDS.iter().map(|(command, about)| json!({ "command": command, "description": about })).collect();
            json!({ "commands": commands })
        }
        Order::Post { .. }
        | Order::Retag { .. }
        | Order::Close { .. }
        | Order::Reopen { .. }
        | Order::Remove { .. } => {
            json!({})
        }
    };
    (method(order), body)
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
            guild: None,
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

#[cfg(test)]
mod tests;
