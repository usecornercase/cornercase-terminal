use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::project::split::{Dir, Place};

const MODIFIERS: [(KeyModifiers, &str); 3] =
    [(KeyModifiers::CONTROL, "ctrl"), (KeyModifiers::ALT, "alt"), (KeyModifiers::SHIFT, "shift")];

const REFUSED: [(char, &str); 3] =
    [('c', "it stops programs"), ('d', "it ends the shell"), ('z', "it suspends programs")];

const CLASHES: [(&str, &str); 6] = [
    ("ctrl+b", "tmux's prefix, so it never reaches a tmux inside a pane"),
    ("ctrl+a", "screen's prefix and the shell's start of line"),
    ("ctrl+space", "switches the input source on macOS"),
    ("ctrl+r", "the shell's history search"),
    ("ctrl+l", "clears the screen in shells"),
    ("ctrl+e", "the shell's end of line"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Prefix {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl Prefix {
    pub(crate) fn read(text: &str) -> Result<Self, String> {
        const HOW: &str = "use ctrl or alt with a key, or a function key";
        Self::spell(text).ok_or_else(|| HOW.to_string()).and_then(Self::from_key)
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        Self::read(text).ok()
    }

    fn spell(text: &str) -> Option<KeyEvent> {
        let text = text.trim().to_lowercase();
        if text.is_empty() {
            return None;
        }
        let mut modifiers = KeyModifiers::NONE;
        let mut rest = text.as_str();
        while let Some((name, tail)) = rest.split_once('+').filter(|(_, tail)| !tail.is_empty()) {
            let (modifier, _) = MODIFIERS.iter().find(|(_, n)| *n == name)?;
            modifiers |= *modifier;
            rest = tail;
        }
        let code = match rest {
            "space" => KeyCode::Char(' '),
            name if name.starts_with('f') && name.len() > 1 => KeyCode::F(name[1..].parse().ok()?),
            name => {
                let mut chars = name.chars();
                let c = chars.next()?;
                if chars.next().is_some() {
                    return None;
                }
                KeyCode::Char(c)
            }
        };
        Some(KeyEvent::new(code, modifiers))
    }

    pub(crate) fn from_key(key: KeyEvent) -> Result<Self, String> {
        let prefix = Self::normal(key);
        match prefix.code {
            KeyCode::F(n) if (1..=24).contains(&n) => Ok(prefix),
            KeyCode::Char(c) if prefix.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                let refused = REFUSED.iter().find(|(r, _)| *r == c && prefix.modifiers == KeyModifiers::CONTROL);
                match refused {
                    Some((_, why)) => Err(format!("ctrl+{c} cannot be the prefix: {why}")),
                    None => Ok(prefix),
                }
            }
            _ => Err("use ctrl or alt with a key, or a function key".into()),
        }
    }

    fn normal(key: KeyEvent) -> Self {
        let mut modifiers = key.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
        let code = match key.code {
            KeyCode::Char(c) if c.is_ascii_uppercase() => {
                modifiers |= KeyModifiers::SHIFT;
                KeyCode::Char(c.to_ascii_lowercase())
            }
            KeyCode::Char(c) => {
                if !c.is_ascii_alphabetic() {
                    modifiers -= KeyModifiers::SHIFT;
                }
                let legacy = modifiers.contains(KeyModifiers::CONTROL) && ('4'..='7').contains(&c);
                KeyCode::Char(if legacy { ['\\', ']', '^', '_'][usize::from(c as u8 - b'4')] } else { c })
            }
            code => code,
        };
        Self { code, modifiers }
    }

    pub(crate) fn matches(self, key: KeyEvent) -> bool {
        Self::normal(key) == self
    }

    pub(crate) fn clash(self) -> Option<&'static str> {
        let name = self.to_string();
        CLASHES.iter().find(|(key, _)| *key == name).map(|(_, why)| *why)
    }
}

impl fmt::Display for Prefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (modifier, name) in MODIFIERS {
            if self.modifiers.contains(modifier) {
                write!(f, "{name}+")?;
            }
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("space"),
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::F(n) => write!(f, "f{n}"),
            _ => f.write_str("?"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    NextTab,
    PreviousTab,
    Tab(usize),
    NextWorkspace,
    PreviousWorkspace,
    NextProject,
    PreviousProject,
    Pane(Place),
    Agent,
    Mark,
    Search,
    FindSaid,
    NewTab,
    Split(Dir),
    ClosePane,
    RenameTab,
    FindNames,
    FindText,
    Files,
    Changes,
    Base,
    PullRequest,
    Flow,
    NewWorkspace,
    StartFlow,
    RenameWorkspace,
    CloseWorkspace,
    Issues,
    Todo,
    Settings,
    Usage,
    Help,
    Away,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Group {
    Find,
    Git,
    Workspace,
}

impl Group {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Find => "Find",
            Self::Git => "Git",
            Self::Workspace => "Workspace",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Find => "find…",
            Self::Git => "git…",
            Self::Workspace => "workspace…",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Run(Action),
    Open(Group),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Item {
    pub(crate) key: &'static str,
    pub(crate) label: &'static str,
    pub(crate) step: Option<Step>,
}

const fn run(key: &'static str, label: &'static str, action: Action) -> Item {
    Item { key, label, step: Some(Step::Run(action)) }
}

const fn open(key: &'static str, group: Group) -> Item {
    Item { key, label: group.label(), step: Some(Step::Open(group)) }
}

const ROOT: [Item; 26] = [
    run("n", "next tab", Action::NextTab),
    run("p", "previous tab", Action::PreviousTab),
    Item { key: "1-9", label: "go to tab", step: None },
    run("]", "next workspace", Action::NextWorkspace),
    run("[", "previous workspace", Action::PreviousWorkspace),
    run("}", "next project", Action::NextProject),
    run("{", "previous project", Action::PreviousProject),
    Item { key: "←↑↓→", label: "pane on that side", step: None },
    run("a", "agent that needs you", Action::Agent),
    run("m", "mark for review", Action::Mark),
    run("/", "search", Action::Search),
    run("c", "new tab", Action::NewTab),
    run("|", "split right", Action::Split(Dir::Right)),
    run("-", "split down", Action::Split(Dir::Down)),
    run("x", "close pane", Action::ClosePane),
    run("r", "rename tab", Action::RenameTab),
    open("f", Group::Find),
    open("g", Group::Git),
    open("w", Group::Workspace),
    run("i", "issues", Action::Issues),
    run("t", "todo", Action::Todo),
    run(",", "settings", Action::Settings),
    run("u", "usage", Action::Usage),
    run("h", "help", Action::Help),
    run("o", "away on chat apps", Action::Away),
    run("q", "quit", Action::Quit),
];

const FIND: [Item; 5] = [
    run("f", "files by name", Action::FindNames),
    run("w", "text in files", Action::FindText),
    run("e", "file tree", Action::Files),
    run("p", "projects, workspaces, tabs", Action::Search),
    run("s", "said in sessions", Action::FindSaid),
];

const GIT: [Item; 3] = [
    run("d", "changes", Action::Changes),
    run("b", "compare against a branch", Action::Base),
    run("p", "pull request and its checks", Action::PullRequest),
];

const WORKSPACE: [Item; 5] = [
    run("n", "new workspace", Action::NewWorkspace),
    run("s", "start a flow", Action::StartFlow),
    run("r", "rename workspace", Action::RenameWorkspace),
    run("f", "flow of this workspace", Action::Flow),
    run("x", "close workspace", Action::CloseWorkspace),
];

pub(crate) fn items(group: Option<Group>) -> &'static [Item] {
    match group {
        None => &ROOT,
        Some(Group::Find) => &FIND,
        Some(Group::Git) => &GIT,
        Some(Group::Workspace) => &WORKSPACE,
    }
}

pub(crate) fn lookup(group: Option<Group>, key: KeyEvent) -> Option<Step> {
    if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
        return None;
    }
    let pane = |place| Some(Step::Run(Action::Pane(place)));
    match (group, key.code) {
        (None, KeyCode::Left) => pane(Place::Left),
        (None, KeyCode::Right) => pane(Place::Right),
        (None, KeyCode::Up) => pane(Place::Above),
        (None, KeyCode::Down) => pane(Place::Below),
        (None, KeyCode::Char(c @ '1'..='9')) => Some(Step::Run(Action::Tab(usize::from(c as u8 - b'1')))),
        (_, KeyCode::Char(c)) => {
            let mut key = [0; 4];
            let key = c.encode_utf8(&mut key);
            items(group).iter().find(|item| item.key == key).and_then(|item| item.step)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
