use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

const PREFIX: &str = "bad message: ";

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Wire {
    Argv(Vec<String>),
    Stdin(String),
    StdinBase64(String),
    Eof(bool),
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Input {
    Text(String),
    Bytes(Vec<u8>),
    Eof,
}

impl Input {
    pub(super) fn into_bytes(self) -> Option<Vec<u8>> {
        match self {
            Self::Text(mut text) => {
                text.push('\n');
                Some(text.into_bytes())
            }
            Self::Bytes(bytes) => Some(bytes),
            Self::Eof => None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Output {
    Stdout(String),
    Stderr(String),
    Exit(i32),
}

impl Output {
    pub(super) fn json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

fn wire(text: &str) -> Result<Wire, String> {
    serde_json::from_str::<Wire>(text).map_err(|e| {
        let why = match e.classify() {
            serde_json::error::Category::Syntax | serde_json::error::Category::Io => "not one JSON object with one key",
            serde_json::error::Category::Eof => "the JSON ends too early",
            serde_json::error::Category::Data => {
                "not one of `argv`, `stdin`, `stdin_base64` and `eof` with a value of its type"
            }
        };
        format!("{PREFIX}{why}")
    })
}

fn input(wire: Wire) -> Result<Input, String> {
    match wire {
        Wire::Argv(_) => Err("`argv` was already sent".into()),
        Wire::Stdin(text) => Ok(Input::Text(text)),
        Wire::StdinBase64(text) => STANDARD
            .decode(text)
            .map(Input::Bytes)
            .map_err(|_| format!("{PREFIX}`stdin_base64` is not standard base64")),
        Wire::Eof(true) => Ok(Input::Eof),
        Wire::Eof(false) => Err(format!("{PREFIX}`eof` must be true")),
    }
}

pub(super) fn first(text: &str) -> Result<Vec<String>, String> {
    match wire(text)? {
        Wire::Argv(argv) => Ok(argv),
        _ => Err("the first message must be `argv`".into()),
    }
}

pub(super) fn next(text: &str) -> Result<Input, String> {
    input(wire(text)?)
}

pub(super) fn refusal(message: &str) -> String {
    message.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

#[cfg(test)]
mod tests;
