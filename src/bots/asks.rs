use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crossterm::event::{KeyCode, KeyEvent};

use super::secrets;
use crate::editor::Editor;
use crate::issues::Secret;

pub(crate) const NAMES_MAX: usize = 4;
pub(crate) const ABOUT_MAX: usize = 200;
pub(crate) const ASKS_MAX: usize = 4;
pub(crate) const ASKS: &str = "asks for a secret: ";
pub(crate) const SAVED: &str = "secret saved: ";
pub(crate) const NOT_GIVEN: &str = "secret not given: ";
pub(crate) const ONLY_ITSELF: &str = "only the bot itself asks for a secret, from its own turn";
pub(crate) const NAME_TWICE: &str = "a secret's name is there twice";
pub(crate) const NAME_HOLDS_VALUE: &str = "a secret's name holds the value of a saved secret: ask by its name";
pub(crate) const ABOUT_HOLDS_VALUE: &str =
    "--about holds the value of a saved secret: say why without a secret's value";
pub(crate) const NO_ANSWER: &str = "a bot cannot give or decline what a bot asks for";
pub(crate) const WHAT_HAPPENS: &str =
    "The bot will read these values. They are kept encrypted on this computer, for this bot only.";
pub(crate) const SAVE: &str = "save";
pub(crate) const LATER: &str = "not now";
pub(crate) const NEVER: &str = "never";
const DIRECTION_MARKS: [char; 3] = ['\u{200E}', '\u{200F}', '\u{061C}'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Ask {
    pub(crate) id: u64,
    pub(crate) names: Vec<String>,
    pub(crate) about: String,
    pub(crate) replace: bool,
    pub(crate) run: Option<u64>,
    pub(crate) given: Vec<String>,
}

impl Ask {
    pub(crate) fn missing(&self) -> Vec<&str> {
        let given = |name: &&String| self.given.iter().any(|given| secrets::same_name(given, name));
        self.names.iter().filter(|name| !given(name)).map(String::as_str).collect()
    }

    pub(crate) fn done(&self) -> bool {
        self.missing().is_empty()
    }

    pub(crate) fn same(&self, names: &[String]) -> bool {
        let keys = |names: &[String]| names.iter().map(|name| name.to_ascii_lowercase()).collect::<BTreeSet<String>>();
        keys(&self.names) == keys(names)
    }

    pub(crate) fn joined(&self) -> String {
        self.names.join(", ")
    }
}

pub(crate) fn names_refusal(names: &[String]) -> Option<String> {
    if names.is_empty() || names.len() > NAMES_MAX {
        return Some(format!("ask for 1 to {NAMES_MAX} secrets, not {}", names.len()));
    }
    if let Some(why) = names.iter().find_map(|name| secrets::name_refusal(name)) {
        return Some(why);
    }
    twice(names).then(|| String::from(NAME_TWICE))
}

pub(crate) fn twice<'a>(names: impl IntoIterator<Item = &'a String>) -> bool {
    let names: Vec<&String> = names.into_iter().collect();
    let unique: BTreeSet<String> = names.iter().map(|name| name.to_ascii_lowercase()).collect();
    unique.len() != names.len()
}

fn is_direction_mark(c: char) -> bool {
    DIRECTION_MARKS.contains(&c) || ('\u{202A}'..='\u{202E}').contains(&c) || ('\u{2066}'..='\u{2069}').contains(&c)
}

pub(crate) fn about_refusal(about: &str) -> Option<String> {
    let length = about.chars().count();
    if length == 0 || length > ABOUT_MAX {
        return Some(format!("--about takes 1 to {ABOUT_MAX} characters, not {length}"));
    }
    about
        .chars()
        .any(|c| c.is_control() || is_direction_mark(c))
        .then(|| String::from("--about takes one line of plain text, with no tab, escape or direction mark"))
}

pub(crate) fn asks_text(joined: &str) -> String {
    format!("{ASKS}{joined}")
}

pub(crate) fn saved_text(joined: &str) -> String {
    format!("{SAVED}{joined}")
}

pub(crate) fn not_given_text(joined: &str) -> String {
    format!("{NOT_GIVEN}{joined}")
}

pub(crate) fn told_saved(joined: &str) -> String {
    format!("The user saved the secrets you asked for: {joined}.")
}

pub(crate) fn told_declined(joined: &str) -> String {
    format!("The user did not give: {joined}.")
}

pub(crate) fn toast(bot: &str, joined: &str) -> String {
    format!("{bot} {}", asks_text(joined))
}

pub(crate) fn announced(bot: &str, joined: &str) -> String {
    format!("{}; open cornercase to give it", toast(bot, joined))
}

#[derive(Clone)]
pub(crate) struct Field {
    pub(crate) name: String,
    pub(crate) value: Editor,
}

impl fmt::Debug for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Field({}, ..)", self.name)
    }
}

#[derive(Clone)]
pub(crate) struct Form {
    pub(crate) bot: u64,
    pub(crate) ask: u64,
    pub(crate) fields: Vec<Field>,
    pub(crate) focus: usize,
    pub(crate) error: Option<String>,
}

impl fmt::Debug for Form {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = self.fields.iter().map(|field| field.name.as_str()).collect();
        write!(f, "Form(bot {}, ask {}, {names:?})", self.bot, self.ask)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    None,
    Save,
    Later,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Field { at: usize, col: usize },
    Button(&'static str),
}

impl Form {
    pub(crate) fn new(bot: u64, ask: &Ask) -> Self {
        let fields =
            ask.missing().into_iter().map(|name| Field { name: name.to_string(), value: Editor::default() }).collect();
        Self { bot, ask: ask.id, fields, focus: 0, error: None }
    }

    pub(crate) fn refresh(&mut self, ask: &Ask) {
        let focused = self.fields.get(self.focus).map(|field| field.name.clone());
        let mut old = std::mem::take(&mut self.fields);
        let missing = ask.missing();
        for name in &ask.names {
            match old.iter().position(|field| &field.name == name) {
                Some(at) => self.fields.push(old.swap_remove(at)),
                None if missing.contains(&name.as_str()) => {
                    self.fields.push(Field { name: name.clone(), value: Editor::default() });
                }
                None => {}
            }
        }
        let at = self.fields.iter().position(|field| Some(&field.name) == focused.as_ref());
        self.focus = at.unwrap_or(0);
    }

    pub(crate) fn buttons() -> [&'static str; 3] {
        [NEVER, LATER, SAVE]
    }

    fn step_focus(&mut self, forward: bool) {
        let count = self.fields.len().max(1);
        self.focus = if forward { (self.focus + 1) % count } else { (self.focus + count - 1) % count };
    }

    pub(crate) fn key(&mut self, key: KeyEvent, width: usize) -> Step {
        self.error = None;
        match key.code {
            KeyCode::Esc => return Step::Later,
            KeyCode::Tab | KeyCode::Down => self.step_focus(true),
            KeyCode::BackTab | KeyCode::Up => self.step_focus(false),
            KeyCode::Enter => match self.fields.iter().position(|field| field.value.is_empty()) {
                Some(empty) => self.focus = empty,
                None => return Step::Save,
            },
            _ => {
                if let Some(field) = self.fields.get_mut(self.focus) {
                    field.value.one_line_key(key, width);
                    field.value.reveal(width, 1);
                }
            }
        }
        Step::None
    }

    pub(crate) fn hit(&mut self, hit: Hit, width: usize) -> Step {
        match hit {
            Hit::Button(SAVE) => Step::Save,
            Hit::Button(LATER) => Step::Later,
            Hit::Button(_) => Step::Never,
            Hit::Field { at, col } => {
                if let Some(field) = self.fields.get_mut(at) {
                    self.focus = at;
                    field.value.place(width, 0, col);
                }
                Step::None
            }
        }
    }

    pub(crate) fn insert(&mut self, text: &str, width: usize) {
        self.error = None;
        if let Some(field) = self.fields.get_mut(self.focus) {
            field.value.insert(text);
            field.value.reveal(width, 1);
        }
    }

    pub(crate) fn values(&self) -> BTreeMap<String, Secret> {
        self.fields.iter().map(|field| (field.name.clone(), Secret(field.value.text()))).collect()
    }
}

#[cfg(test)]
mod tests;
