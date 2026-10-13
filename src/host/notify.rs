use serde::{Deserialize, Serialize};

pub const AUTO: &str = "auto";
pub const OFF: &str = "off";
const TITLE: &str = "cornercase";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Channel {
    Osc777,
    Osc9,
    Osc99,
    Bell,
}

impl Channel {
    pub const ALL: [Self; 4] = [Self::Osc777, Self::Osc9, Self::Osc99, Self::Bell];

    pub fn id(self) -> &'static str {
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
            Self::Osc777 => "Ghostty, WezTerm, foot, Konsole, Warp, Rio",
            Self::Osc9 => "iTerm2",
            Self::Osc99 => "kitty, Contour, VS Code",
            Self::Bell => "a beep or a mark on the window, in any terminal",
        }
    }

    pub fn encode(self, text: &str) -> Vec<u8> {
        let text = clean(text).replace(';', ",");
        match self {
            Self::Osc777 => format!("\x1b]777;notify;{TITLE};{text}\x07"),
            Self::Osc9 => format!("\x1b]9;{text}\x07"),
            Self::Osc99 => format!("\x1b]99;o=unfocused;{text}\x1b\\"),
            Self::Bell => "\x07".into(),
        }
        .into_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub text: String,
    pub channel: Option<Channel>,
}

impl Notification {
    pub fn new(text: &str, setting: &str) -> Option<Self> {
        let off = setting.trim().eq_ignore_ascii_case(OFF);
        (!off).then(|| Self { text: text.into(), channel: Channel::from_id(setting) })
    }

    pub fn encode(&self, detected: Channel) -> Vec<u8> {
        self.channel.unwrap_or(detected).encode(&self.text)
    }
}

pub fn choices() -> Vec<(&'static str, &'static str)> {
    let mut choices = vec![(AUTO, "what your terminal understands")];
    choices.extend(Channel::ALL.map(|c| (c.id(), c.note())));
    choices.push((OFF, "only the toast and the marks in cornercase"));
    choices
}

pub fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

pub fn detect(version: Option<&str>, var: impl Fn(&str) -> Option<String>) -> Channel {
    let set = |name: &str| var(name).is_some_and(|value| !value.is_empty());
    if set("TMUX") || set("STY") || set("ZELLIJ") {
        return Channel::Bell;
    }
    if let Some(version) = version {
        return by_name(version).unwrap_or(Channel::Bell);
    }
    ["TERM_PROGRAM", "LC_TERMINAL", "TERM"]
        .into_iter()
        .filter_map(&var)
        .find_map(|name| by_name(&name))
        .or_else(|| set("KITTY_WINDOW_ID").then_some(Channel::Osc99))
        .or_else(|| set("KONSOLE_VERSION").then_some(Channel::Osc777))
        .unwrap_or(Channel::Bell)
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
