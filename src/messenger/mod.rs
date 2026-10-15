use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::discord;
use crate::persist;
use crate::telegram;

pub(crate) mod link;
pub(crate) mod markup;

pub(crate) use markup::Markup;

pub(crate) const COMMANDS: [(&str, &str); 4] = [
    ("status", "every agent and what it is doing"),
    ("new", "start an agent"),
    ("away", "message me here when an agent needs me"),
    ("back", "stop messaging me here"),
];
const VERSION: u32 = 1;
const MOST_CHOICES: usize = 9;
pub(crate) const MARKS: &str = "│┃|❯›>▶●○◯";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Via {
    Telegram,
    Discord,
}

impl Via {
    pub(crate) const ALL: [Self; 2] = [Self::Telegram, Self::Discord];

    pub(crate) fn index(self) -> usize {
        match self {
            Self::Telegram => 0,
            Self::Discord => 1,
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Telegram => "Telegram",
            Self::Discord => "Discord",
        }
    }

    pub(crate) fn target(self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
            Self::Discord => "discord",
        }
    }

    pub(crate) fn checked(self) -> &'static str {
        match self {
            Self::Telegram => "telegram bot checked",
            Self::Discord => "discord bot checked",
        }
    }

    pub(crate) fn advised(self) -> &'static str {
        match self {
            Self::Telegram => "telegram app read",
            Self::Discord => "discord app read",
        }
    }

    pub(crate) fn pairing(self) -> &'static str {
        match self {
            Self::Telegram => "telegram pairing",
            Self::Discord => "discord pairing",
        }
    }

    pub(crate) fn token_field(self) -> &'static str {
        match self {
            Self::Telegram => "Telegram bot token, from @BotFather",
            Self::Discord => "Discord bot token (needs User Install on, Install Link None, Public Bot off)",
        }
    }

    pub(crate) fn markup(self) -> &'static dyn Markup {
        match self {
            Self::Telegram => &telegram::html::Html,
            Self::Discord => &discord::markdown::Markdown,
        }
    }

    pub(crate) fn data_bytes(self) -> usize {
        match self {
            Self::Telegram => telegram::DATA_BYTES,
            Self::Discord => discord::DATA_BYTES,
        }
    }

    pub(crate) fn token_key(self) -> &'static str {
        match self {
            Self::Telegram => telegram::TOKEN_KEY,
            Self::Discord => discord::TOKEN_KEY,
        }
    }

    pub(crate) fn saved(self, config: &Path) -> PathBuf {
        match self {
            Self::Telegram => crate::paths::telegram(config),
            Self::Discord => crate::paths::discord(config),
        }
    }

    pub(crate) fn api(self) -> String {
        match self {
            Self::Telegram => std::env::var(telegram::API_ENV).unwrap_or_else(|_| telegram::API.into()),
            Self::Discord => std::env::var(discord::API_ENV).unwrap_or_else(|_| discord::API.into()),
        }
    }

    pub(crate) fn start_link<E: From<(Self, link::Event)> + Send + 'static>(
        self,
        id: u64,
        api: &str,
        token: &str,
        tx: &std::sync::mpsc::Sender<E>,
        since: i64,
    ) -> link::Link {
        match self {
            Self::Telegram => {
                telegram::link::start(id, &telegram::Bot::new(api, token), tx, telegram::link::POLL, since)
            }
            Self::Discord => discord::gateway::start(id, &discord::Bot::new(api, token), tx, since),
        }
    }

    pub(crate) fn check(self, api: &str, token: &str) -> Result<Identity, String> {
        match self {
            Self::Telegram => {
                let name = telegram::Bot::new(api, token).me().map_err(|e| e.to_string())?;
                Ok(Identity { name, advice: Advice::Read(None) })
            }
            Self::Discord => {
                let bot = discord::Bot::new(api, token);
                let name = bot.me().map_err(|e| e.to_string())?;
                Ok(Identity { name, advice: bot.read_advice().map_or(Advice::Unread, Advice::Read) })
            }
        }
    }

    pub(crate) fn advises(self) -> bool {
        match self {
            Self::Telegram => false,
            Self::Discord => true,
        }
    }

    pub(crate) fn read_advice(self, api: &str, token: &str) -> Result<Option<&'static str>, String> {
        match self {
            Self::Telegram => Ok(None),
            Self::Discord => discord::Bot::new(api, token).read_advice().map_err(|e| e.to_string()),
        }
    }

    pub(crate) fn rejected(self) -> String {
        match self {
            Self::Telegram => telegram::Failure::Rejected.to_string(),
            Self::Discord => discord::Failure::Rejected.to_string(),
        }
    }

    pub(crate) fn unset(self) -> &'static str {
        match self {
            Self::Telegram => "make a bot with @BotFather and paste its token here",
            Self::Discord => "make a Discord app with User Install and paste its bot token",
        }
    }

    pub(crate) fn pairing_code(self, text: &str) -> Option<&str> {
        match self {
            Self::Telegram => text.trim().strip_prefix("/start").map(str::trim).filter(|code| !code.is_empty()),
            Self::Discord => None,
        }
    }

    pub(crate) fn pairing_link(self, bot: &str, token: &str, code: &str) -> (String, Vec<String>) {
        match self {
            Self::Telegram => (
                format!("https://t.me/{bot}?start={code}"),
                vec![
                    "scan the code with your phone's camera, then tap Start in Telegram.".into(),
                    "the bot then answers only that chat.".into(),
                ],
            ),
            Self::Discord => match discord::install_link(token) {
                Some(url) => (
                    url,
                    vec![
                        "open the link, add the app to your Discord account and authorize it.".into(),
                        "cornercase then writes to you, and the bot answers only you.".into(),
                    ],
                ),
                None => (String::new(), vec!["the token does not name its app: paste it again".into()]),
            },
        }
    }

    pub(crate) fn pairs_by_owner(self) -> bool {
        match self {
            Self::Telegram => false,
            Self::Discord => true,
        }
    }

    pub(crate) fn forces_reply(self) -> bool {
        match self {
            Self::Telegram => true,
            Self::Discord => false,
        }
    }

    pub(crate) fn pair(self, api: &str, token: &str, welcome: &str) -> (Advice, Result<Owned, String>) {
        match self {
            Self::Telegram => (Advice::Read(None), Ok(Owned::Later(Duration::ZERO))),
            Self::Discord => {
                let (advice, paired) = discord::Bot::new(api, token).pair(welcome);
                let owned = match paired {
                    Ok(chat) => Ok(Owned::Chat(chat)),
                    Err(discord::Failure::Closed) => Ok(Owned::Later(Duration::ZERO)),
                    Err(discord::Failure::Wait(seconds)) => Ok(Owned::Later(Duration::from_secs(seconds))),
                    Err(failure) => Err(failure.to_string()),
                };
                (advice, owned)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Identity {
    pub(crate) name: String,
    pub(crate) advice: Advice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Advice {
    Unread,
    Read(Option<&'static str>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Owned {
    Chat(Chat),
    Later(Duration),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Key {
    pub(crate) text: String,
    pub(crate) data: String,
}

pub(crate) type Keyboard = Vec<Vec<Key>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Order {
    Send { chat: i64, text: String, keyboard: Keyboard, reply_to: Option<i64>, ask: Option<String>, tag: Option<u64> },
    Edit { chat: i64, message: i64, text: String, keyboard: Keyboard },
    Buttons { chat: i64, message: i64, keyboard: Keyboard },
    Answer { id: String, text: String },
    React { chat: i64, message: i64, emoji: String },
    Commands,
}

impl Order {
    pub(crate) fn tag(&self) -> Option<u64> {
        match self {
            Self::Send { tag, .. } => *tag,
            _ => None,
        }
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

    pub(crate) fn fits(&self, run: &str, via: Via) -> bool {
        self.data(run).len() <= via.data_bytes()
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
