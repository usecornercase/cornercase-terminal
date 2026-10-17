use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::graphics::detect::version as parse_version;

pub(crate) const AUTO: &str = "auto";
pub(crate) const OFF: &str = "off";
pub(crate) const KEPT: usize = 64;
const TITLE: &str = "cornercase";
const MAX_ID: usize = 64;
const FOOT_CLICKS: (u32, u32, u32) = (1, 18, 0);
const KONSOLE_CLICKS: (u32, u32, u32) = (24, 12, 0);
const KONSOLE_CLICKS_NUMBER: u32 = 241_200;
const REPORT_PREFIX: &str = "\x1b]99;";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Channel {
    Osc777,
    Osc9,
    Osc99,
    Bell,
}

impl Channel {
    pub(crate) const ALL: [Self; 4] = [Self::Osc777, Self::Osc9, Self::Osc99, Self::Bell];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Osc777 => "osc777",
            Self::Osc9 => "osc9",
            Self::Osc99 => "osc99",
            Self::Bell => "bell",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id().eq_ignore_ascii_case(id.trim()))
    }

    fn note(self) -> &'static str {
        match self {
            Self::Osc777 => "Ghostty, WezTerm, Warp, Rio, older foot and Konsole",
            Self::Osc9 => "iTerm2",
            Self::Osc99 => "kitty, Contour, VS Code, foot, Konsole",
            Self::Bell => "a beep or a mark on the window, in any terminal",
        }
    }

    pub(crate) fn encode(self, text: &str, token: Option<&str>) -> Vec<u8> {
        let text = clean(text).replace(';', ",");
        match self {
            Self::Osc777 => format!("\x1b]777;notify;{TITLE};{text}\x07"),
            Self::Osc9 => format!("\x1b]9;{text}\x07"),
            Self::Osc99 => match token {
                Some(token) => format!("\x1b]99;i={token}:a=focus,report:o=unfocused;{text}\x1b\\"),
                None => format!("\x1b]99;o=unfocused;{text}\x1b\\"),
            },
            Self::Bell => "\x07".into(),
        }
        .into_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Notification {
    pub(crate) text: String,
    pub(crate) channel: Option<Channel>,
    pub(crate) token: Option<String>,
}

impl Notification {
    pub(crate) fn new(text: &str, setting: &str) -> Option<Self> {
        let off = setting.trim().eq_ignore_ascii_case(OFF);
        (!off).then(|| Self { text: text.into(), channel: Channel::from_id(setting), token: None })
    }

    pub(crate) fn channel_for(&self, detected: Channel) -> Channel {
        self.channel.unwrap_or(detected)
    }

    pub(crate) fn encode(&self, detected: Channel) -> Vec<u8> {
        self.channel_for(detected).encode(&self.text, self.token.as_deref())
    }
}

pub(crate) fn close(token: &str) -> Vec<u8> {
    format!("\x1b]99;i={token}:p=close;\x1b\\").into_bytes()
}

fn valid_id(id: &str) -> bool {
    (1..=MAX_ID).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"_+.-".contains(&b))
}

pub(crate) fn activation(reply: &[u8]) -> Option<String> {
    let body = std::str::from_utf8(reply).ok()?.strip_prefix(REPORT_PREFIX)?;
    let body = body.strip_suffix("\x1b\\").or_else(|| body.strip_suffix('\x07'))?;
    let metadata = body.split(';').next().unwrap_or(body);
    let mut id = None;
    for pair in metadata.split(':') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "i" => id = Some(value),
            "p" if !value.is_empty() && value != "activated" => return None,
            _ => {}
        }
    }
    id.filter(|id| valid_id(id)).map(str::to_string)
}

#[derive(Debug, Default)]
pub(crate) struct Sent(VecDeque<String>);

impl Sent {
    pub(crate) fn push(&mut self, notification: &Notification, detected: Channel) {
        let Some(token) = &notification.token else { return };
        if notification.channel_for(detected) != Channel::Osc99 {
            return;
        }
        if self.0.len() >= KEPT {
            self.0.pop_front();
        }
        self.0.push_back(token.clone());
    }

    pub(crate) fn closes(&self) -> Vec<u8> {
        self.0.iter().flat_map(|token| close(token)).collect()
    }
}

pub(crate) fn choices() -> Vec<(&'static str, &'static str)> {
    let mut choices = vec![(AUTO, "what your terminal understands")];
    choices.extend(Channel::ALL.map(|c| (c.id(), c.note())));
    choices.push((OFF, "only the toast and the marks in cornercase"));
    choices
}

pub(crate) fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

pub(crate) fn detect(version: Option<&str>, var: impl Fn(&str) -> Option<String>) -> Channel {
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    if set("TMUX") || set("STY") || set("ZELLIJ") {
        return Channel::Bell;
    }
    if let Some(version) = version {
        return clicks(version).or_else(|| by_name(version)).unwrap_or(Channel::Bell);
    }
    ["TERM_PROGRAM", "LC_TERMINAL", "TERM"]
        .into_iter()
        .filter_map(&var)
        .find_map(|name| by_name(&name))
        .or_else(|| set("KITTY_WINDOW_ID").then_some(Channel::Osc99))
        .or_else(|| {
            var("KONSOLE_VERSION").filter(|v| !v.is_empty()).map(|v| {
                if v.trim().parse::<u32>().is_ok_and(|n| n >= KONSOLE_CLICKS_NUMBER) {
                    Channel::Osc99
                } else {
                    Channel::Osc777
                }
            })
        })
        .unwrap_or(Channel::Bell)
}

fn clicks(version: &str) -> Option<Channel> {
    let lower = version.to_ascii_lowercase();
    let at_least = |rest: &str, min| parse_version(rest).is_some_and(|v| v >= min);
    let capable = if let Some(rest) = lower.strip_prefix("foot(") {
        at_least(rest, FOOT_CLICKS)
    } else if let Some(rest) = lower.strip_prefix("konsole ") {
        at_least(rest, KONSOLE_CLICKS)
    } else {
        false
    };
    capable.then_some(Channel::Osc99)
}

fn by_name(name: &str) -> Option<Channel> {
    let name = name.to_ascii_lowercase();
    if name.starts_with("xterm.js") {
        return Some(Channel::Osc99);
    }
    name.split(|c: char| !c.is_ascii_alphanumeric()).find_map(|word| match word {
        "ghostty" | "wezterm" | "foot" | "konsole" | "warp" | "warpterminal" | "rio" => Some(Channel::Osc777),
        "iterm" | "iterm2" => Some(Channel::Osc9),
        "kitty" | "contour" | "vscode" => Some(Channel::Osc99),
        "tmux" | "screen" | "zellij" => Some(Channel::Bell),
        _ => None,
    })
}

#[cfg(test)]
mod tests;
