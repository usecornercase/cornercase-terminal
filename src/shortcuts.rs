use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::split::{Dir, Place};

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
pub struct Prefix {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl Prefix {
    pub fn parse(text: &str) -> Option<Self> {
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
        Self::from_key(KeyEvent::new(code, modifiers)).ok()
    }

    pub fn from_key(key: KeyEvent) -> Result<Self, String> {
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

    pub fn matches(self, key: KeyEvent) -> bool {
        Self::normal(key) == self
    }

    pub fn event(self) -> KeyEvent {
        KeyEvent::new(self.code, self.modifiers)
    }

    pub fn clash(self) -> Option<&'static str> {
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
pub enum Action {
    NextTab,
    PreviousTab,
    Tab(usize),
    NextWorkspace,
    PreviousWorkspace,
    NextProject,
    PreviousProject,
    Pane(Place),
    Agent,
    Search,
    NewTab,
    Split(Dir),
    ClosePane,
    RenameTab,
    FindNames,
    FindText,
    Files,
    Changes,
    Base,
    NewWorkspace,
    RenameWorkspace,
    CloseWorkspace,
    Issues,
    Todo,
    Settings,
    Usage,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Find,
    Git,
    Workspace,
}

impl Group {
    pub fn name(self) -> &'static str {
        match self {
            Self::Find => "find",
            Self::Git => "git",
            Self::Workspace => "workspace",
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
pub enum Step {
    Run(Action),
    Open(Group),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    pub key: &'static str,
    pub label: &'static str,
    pub step: Option<Step>,
}

const fn run(key: &'static str, label: &'static str, action: Action) -> Item {
    Item { key, label, step: Some(Step::Run(action)) }
}

const fn open(key: &'static str, group: Group) -> Item {
    Item { key, label: group.label(), step: Some(Step::Open(group)) }
}

const ROOT: [Item; 23] = [
    run("n", "next tab", Action::NextTab),
    run("p", "previous tab", Action::PreviousTab),
    Item { key: "1-9", label: "go to tab", step: None },
    run("]", "next workspace", Action::NextWorkspace),
    run("[", "previous workspace", Action::PreviousWorkspace),
    run("}", "next project", Action::NextProject),
    run("{", "previous project", Action::PreviousProject),
    Item { key: "←↑↓→", label: "pane on that side", step: None },
    run("a", "agent that needs you", Action::Agent),
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
    run("q", "quit", Action::Quit),
];

const FIND: [Item; 4] = [
    run("f", "files by name", Action::FindNames),
    run("w", "text in files", Action::FindText),
    run("e", "file tree", Action::Files),
    run("p", "projects, workspaces, tabs", Action::Search),
];

const GIT: [Item; 2] = [run("d", "changes", Action::Changes), run("b", "compare against a branch", Action::Base)];

const WORKSPACE: [Item; 3] = [
    run("n", "new workspace", Action::NewWorkspace),
    run("r", "rename workspace", Action::RenameWorkspace),
    run("x", "close workspace", Action::CloseWorkspace),
];

pub fn items(group: Option<Group>) -> &'static [Item] {
    match group {
        None => &ROOT,
        Some(Group::Find) => &FIND,
        Some(Group::Git) => &GIT,
        Some(Group::Workspace) => &WORKSPACE,
    }
}

pub fn lookup(group: Option<Group>, key: KeyEvent) -> Option<Step> {
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
mod tests {
    use rstest::rstest;

    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    mod prefix {
        use super::*;

        #[rstest]
        #[case::ctrl_bracket("ctrl+]")]
        #[case::ctrl_space("ctrl+space")]
        #[case::alt_letter("alt+k")]
        #[case::ctrl_shift_letter("ctrl+shift+k")]
        #[case::function_key("f12")]
        #[case::ctrl_function_key("ctrl+f5")]
        fn reads_back_what_it_writes(#[case] text: &str) {
            assert_eq!(Prefix::parse(text).map(|p| p.to_string()).as_deref(), Some(text));
        }

        #[rstest]
        #[case::empty("")]
        #[case::plain_letter("k")]
        #[case::unknown_modifier("hyper+k")]
        #[case::two_keys("ctrl+ab")]
        #[case::ctrl_c("ctrl+c")]
        #[case::shift_only("shift+k")]
        fn refuses_what_cannot_be_one(#[case] text: &str) {
            assert_eq!(Prefix::parse(text), None);
        }

        #[test]
        fn is_read_ignoring_case_and_spaces() {
            assert_eq!(Prefix::parse(" Ctrl+Space ").map(|p| p.to_string()).as_deref(), Some("ctrl+space"));
        }

        #[rstest]
        #[case::legacy_byte(KeyCode::Char('5'), KeyModifiers::CONTROL)]
        #[case::kitty(KeyCode::Char(']'), KeyModifiers::CONTROL)]
        fn ctrl_bracket_matches_however_the_terminal_sends_it(#[case] code: KeyCode, #[case] mods: KeyModifiers) {
            let prefix = Prefix::parse("ctrl+]").expect("a prefix");
            assert!(prefix.matches(key(code, mods)));
        }

        #[test]
        fn an_upper_case_letter_is_that_letter_with_shift() {
            let prefix = Prefix::parse("ctrl+shift+k").expect("a prefix");
            assert!(prefix.matches(key(KeyCode::Char('K'), KeyModifiers::CONTROL)));
        }

        #[test]
        fn shift_on_a_symbol_is_part_of_the_symbol() {
            let prefix = Prefix::from_key(key(KeyCode::Char('}'), KeyModifiers::CONTROL | KeyModifiers::SHIFT));
            assert_eq!(prefix.map(|p| p.to_string()), Ok("ctrl+}".into()));
        }

        #[test]
        fn another_modifier_does_not_match() {
            let prefix = Prefix::parse("ctrl+]").expect("a prefix");
            assert!(!prefix.matches(key(KeyCode::Char(']'), KeyModifiers::CONTROL | KeyModifiers::ALT)));
        }

        #[rstest]
        #[case::enter(KeyCode::Enter, KeyModifiers::NONE)]
        #[case::letter(KeyCode::Char('k'), KeyModifiers::NONE)]
        #[case::arrow(KeyCode::Left, KeyModifiers::CONTROL)]
        fn a_pressed_key_that_cannot_be_one_says_why(#[case] code: KeyCode, #[case] mods: KeyModifiers) {
            assert_eq!(Prefix::from_key(key(code, mods)), Err("use ctrl or alt with a key, or a function key".into()));
        }

        #[test]
        fn ctrl_c_says_what_it_does() {
            let refused = Prefix::from_key(key(KeyCode::Char('c'), KeyModifiers::CONTROL));
            assert_eq!(refused, Err("ctrl+c cannot be the prefix: it stops programs".into()));
        }

        #[test]
        fn known_clashes_are_named() {
            let clash = Prefix::parse("ctrl+b").and_then(Prefix::clash);
            assert_eq!(clash, Some("tmux's prefix, so it never reaches a tmux inside a pane"));
        }
    }

    mod keys {
        use super::*;

        fn press(group: Option<Group>, c: char) -> Option<Step> {
            lookup(group, key(KeyCode::Char(c), KeyModifiers::NONE))
        }

        #[test]
        fn a_letter_runs_its_action() {
            assert_eq!(press(None, 'n'), Some(Step::Run(Action::NextTab)));
        }

        #[test]
        fn a_group_letter_opens_the_group() {
            assert_eq!(press(None, 'f'), Some(Step::Open(Group::Find)));
        }

        #[test]
        fn a_letter_means_what_its_group_says() {
            assert_eq!(press(Some(Group::Find), 'f'), Some(Step::Run(Action::FindNames)));
        }

        #[test]
        fn digits_go_to_a_tab_from_one() {
            assert_eq!(press(None, '1'), Some(Step::Run(Action::Tab(0))));
        }

        #[test]
        fn arrows_move_between_panes() {
            assert_eq!(lookup(None, key(KeyCode::Up, KeyModifiers::NONE)), Some(Step::Run(Action::Pane(Place::Above))));
        }

        #[test]
        fn an_unknown_key_does_nothing() {
            assert_eq!(press(Some(Group::Git), 'z'), None);
        }

        #[test]
        fn a_key_with_ctrl_does_nothing() {
            assert_eq!(lookup(None, key(KeyCode::Char('n'), KeyModifiers::CONTROL)), None);
        }

        #[test]
        fn no_key_is_listed_twice_in_a_menu() {
            for group in [None, Some(Group::Find), Some(Group::Git), Some(Group::Workspace)] {
                let mut keys: Vec<&str> = items(group).iter().map(|i| i.key).collect();
                keys.sort_unstable();
                keys.dedup();
                assert_eq!(keys.len(), items(group).len(), "{group:?}");
            }
        }
    }
}
