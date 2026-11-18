use ratatui::layout::Rect;

use crate::app::App;
use crate::app::overlay::{Overlay, REMOVE_SUBMIT};
use crate::bots::routines::{Asked, Button, Dialog};
use crate::bots::secrets::{self, Entry, Kept};
use crate::editor::Editor;
use crate::queue::clock;
use crate::ui;

const NAME_HINT: &str = "a-z, 0-9, - and _, such as campus-password";
const SAVE_SUBMIT: &str = "save";
const NEXT_SUBMIT: &str = "next";
pub(super) const ONE_LINE: &str =
    "this field takes one line of text with no tab: pipe the value in with cornercase bot secret set";

#[derive(Debug)]
pub(in crate::app) enum Secrets {
    List(SecretsList),
    Entry(SecretEntry),
    Removal(SecretRemoval),
}

#[derive(Debug)]
pub(in crate::app) struct SecretsList {
    pub(in crate::app) dialog: Dialog,
    back: Option<Box<Overlay>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::app) enum Asking {
    Name,
    Value(String),
}

#[derive(Debug)]
pub(in crate::app) struct SecretEntry {
    bot: u64,
    asking: Asking,
    input: Editor,
    error: Option<String>,
    back: Box<Overlay>,
}

#[derive(Debug)]
pub(in crate::app) struct SecretRemoval {
    bot: u64,
    name: Option<String>,
    seen: Fingerprint,
    back: Box<Overlay>,
}

type Fingerprint = (bool, Vec<(String, i64)>);

fn fingerprint(kept: &Kept) -> Fingerprint {
    (kept.is_unreadable(), kept.entries().iter().map(|entry| (entry.name.clone(), entry.saved)).collect())
}

impl Secrets {
    pub(in crate::app) fn bot(&self) -> u64 {
        match self {
            Self::List(list) => list.dialog.bot,
            Self::Entry(entry) => entry.bot,
            Self::Removal(removal) => removal.bot,
        }
    }

    pub(in crate::app) fn list(&self) -> Option<&SecretsList> {
        if let Self::List(list) = self { Some(list) } else { None }
    }

    pub(in crate::app) fn list_mut(&mut self) -> Option<&mut SecretsList> {
        if let Self::List(list) = self { Some(list) } else { None }
    }

    pub(in crate::app) fn is_form(&self) -> bool {
        !matches!(self, Self::List(_))
    }

    pub(in crate::app) fn takes_input(&self) -> bool {
        matches!(self, Self::Entry(_))
    }

    pub(in crate::app) fn input(&mut self) -> Option<&mut Editor> {
        if let Self::Entry(entry) = self { Some(&mut entry.input) } else { None }
    }

    pub(in crate::app) fn clear_error(&mut self) {
        if let Self::Entry(entry) = self {
            entry.error = None;
        }
    }

    pub(in crate::app) fn submit_label(&self) -> &'static str {
        match self {
            Self::Entry(entry) if entry.asking == Asking::Name => NEXT_SUBMIT,
            Self::Entry(_) => SAVE_SUBMIT,
            _ => REMOVE_SUBMIT,
        }
    }

    fn hides_text(&self) -> bool {
        matches!(self, Self::Entry(SecretEntry { asking: Asking::Value(_), .. }))
    }
}

impl Overlay {
    pub(in crate::app) fn hides_text(&self) -> bool {
        matches!(self, Self::BotSecrets(secrets) if secrets.hides_text())
    }
}

fn summary(kept: &Kept) -> String {
    match kept {
        Kept::Unreadable => String::from("cannot be read"),
        Kept::Open(entries) if entries.is_empty() => String::from("none"),
        Kept::Open(entries) => {
            let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
            format!("{} saved: {}", entries.len(), names.join(", "))
        }
    }
}

impl App {
    pub(in crate::app) fn secrets_row(kept: &Kept) -> Option<ui::CardRow> {
        match kept {
            Kept::Unreadable => Some(ui::CardRow { red: true, ..ui::CardRow::new("secrets", "cannot be read") }),
            Kept::Open(entries) if entries.is_empty() => None,
            Kept::Open(entries) => {
                let text = crate::text::plural(entries.len(), "secret", "secrets");
                Some(ui::CardRow::new("secrets", text))
            }
        }
    }

    pub(in crate::app) fn form_with_secrets(&self, form: &crate::bots::form::Form) -> Box<crate::bots::form::Form> {
        let mut shown = form.clone();
        let at = form.editing.as_deref().and_then(|id| self.panels.bots.by_id(id));
        shown.secrets = at.map(|at| summary(&self.panels.bots.live[at].secrets)).unwrap_or_default();
        Box::new(shown)
    }

    pub(in crate::app) fn open_bot_secrets(&mut self, key: u64, back: Option<Box<Overlay>>) {
        let list = SecretsList { dialog: Dialog::new(key, None), back };
        self.overlay = Some(Overlay::BotSecrets(Box::new(Secrets::List(list))));
    }

    pub(in crate::app) fn secrets_view(&self, secrets: &Secrets, area: Rect) -> Option<ui::Overlay> {
        match secrets {
            Secrets::List(list) => {
                let view = self.secrets_list_view(&list.dialog, area)?;
                Some(ui::Overlay::BotSecrets(Box::new(view)))
            }
            Secrets::Entry(entry) => Some(self.secret_entry_view(entry)),
            Secrets::Removal(removal) => self.remove_secret_view(removal),
        }
    }

    pub(in crate::app) fn secrets_list_view(&self, dialog: &Dialog, area: Rect) -> Option<ui::BotRoutines> {
        let at = self.panels.bots.by_key(dialog.bot)?;
        let live = &self.panels.bots.live[at];
        let name = live.bot.name.clone();
        let compact = area.width < ui::COMPACT_WIDTH;
        let rows = self.secret_rows(&live.secrets);
        let ids: Vec<u64> = (0..rows.len() as u64).collect();
        let mut fitted = dialog.clone();
        fitted.fit(&ids, ui::routines_room(area, compact).0);
        let hint = format!("key: file · {}", secrets::WHAT_IT_IS);
        let empty = format!("{name} has no secrets yet. + secret saves a password or a token only {name} can read.");
        Some(ui::BotRoutines {
            title: format!("Secrets › {name}"),
            selected: fitted.index(&ids),
            scroll: fitted.scroll,
            rows,
            hint,
            empty,
            compact,
            new: "+ secret",
        })
    }

    fn secret_rows(&self, kept: &Kept) -> Vec<ui::RoutineRow> {
        let row = |name: &str, when: String, detail: String, buttons: Vec<Button>| ui::RoutineRow {
            name: name.to_string(),
            when,
            state: None,
            colour: None,
            at: String::new(),
            paused: false,
            buttons,
            detail: Some(detail),
        };
        if kept.is_unreadable() {
            let detail = String::from("another key or a damaged file: remove all starts again");
            return vec![row("the secrets cannot be read", String::new(), detail, vec![Button::RemoveAll])];
        }
        let (zone, now) = (self.window_zone(), crate::time::now());
        let saved = |entry: &Entry| format!("saved {}", clock::when(entry.saved, now, &zone));
        let both = || vec![Button::Replace, Button::Remove];
        kept.entries().iter().map(|entry| row(&entry.name, saved(entry), String::new(), both())).collect()
    }

    pub(in crate::app) fn bot_secrets_do(&mut self, asked: Asked) {
        let Some(Overlay::BotSecrets(secrets)) = &self.overlay else { return };
        let key = secrets.bot();
        let Some(at) = self.panels.bots.by_key(key) else { return };
        let entries = self.panels.bots.live[at].secrets.entries();
        let name = |i: u64| entries.get(usize::try_from(i).ok()?).map(|entry| entry.name.clone());
        let next = match asked {
            Asked::None => return,
            Asked::Close => return self.secrets_back(),
            Asked::Open(i) | Asked::Press(i, Button::Edit | Button::Replace) => name(i).map(Asking::Value),
            Asked::Press(i, Button::Remove) => return self.ask_secret_removal(key, name(i)),
            Asked::Press(..) => return self.ask_secret_removal(key, None),
            Asked::New => Some(Asking::Name),
        };
        if let Some(asking) = next {
            self.ask_secret(key, asking);
        }
    }

    fn ask_secret(&mut self, key: u64, asking: Asking) {
        let Some(list) = self.overlay.take() else { return };
        let entry = SecretEntry { bot: key, asking, input: Editor::default(), error: None, back: Box::new(list) };
        self.overlay = Some(Overlay::BotSecrets(Box::new(Secrets::Entry(entry))));
    }

    fn ask_secret_removal(&mut self, key: u64, name: Option<String>) {
        let Some(list) = self.overlay.take() else { return };
        let seen = self.panels.bots.by_key(key).map(|at| fingerprint(&self.panels.bots.live[at].secrets));
        let removal = SecretRemoval { bot: key, name, seen: seen.unwrap_or_default(), back: Box::new(list) };
        self.overlay = Some(Overlay::BotSecrets(Box::new(Secrets::Removal(removal))));
    }

    pub(in crate::app) fn secrets_back(&mut self) {
        self.overlay = match self.overlay.take() {
            Some(Overlay::BotSecrets(secrets)) => match *secrets {
                Secrets::List(list) => list.back.map(|back| *back),
                Secrets::Entry(entry) => Some(*entry.back),
                Secrets::Removal(removal) => Some(*removal.back),
            },
            other => other,
        };
    }

    fn secret_entry_view(&self, entry: &SecretEntry) -> ui::Overlay {
        let name = self.panels.bots.by_key(entry.bot).map(|at| self.panels.bots.live[at].bot.name.clone());
        let hidden = matches!(entry.asking, Asking::Value(_));
        let (title, label, hint, value) = match &entry.asking {
            Asking::Name => ("New secret", "name", NAME_HINT.to_string(), entry.input.clone()),
            Asking::Value(secret) => {
                let hint = format!("{} · {secret}", name.unwrap_or_default());
                ("Secret value", "value", hint, entry.input.masked())
            }
        };
        let note = match (&entry.error, hidden) {
            (Some(error), _) => Some(ui::Note::Error(error.clone())),
            (None, true) => Some(ui::Note::Info(secrets::WHAT_IT_IS.to_string())),
            (None, false) => None,
        };
        let submit = if hidden { SAVE_SUBMIT } else { NEXT_SUBMIT };
        ui::Overlay::Form(ui::Form { title, label, value, hint, toggle: None, note, submit })
    }

    fn remove_secret_view(&self, removal: &SecretRemoval) -> Option<ui::Overlay> {
        let name = self.panels.bots.live[self.panels.bots.by_key(removal.bot)?].bot.name.clone();
        let message = match &removal.name {
            Some(secret) => format!("Remove {secret} from {name}? The bot can no longer read it."),
            None => format!("Remove every secret of {name}? They cannot be read; this starts again."),
        };
        Some(ui::Overlay::Confirm(ui::Confirm {
            title: "Remove secret",
            message,
            note: None,
            submit: REMOVE_SUBMIT,
            extra: None,
            dangerous: true,
        }))
    }

    pub(in crate::app) fn submit_secrets(&mut self, secrets: Secrets) -> Overlay {
        match secrets {
            Secrets::Entry(entry) => self.submit_secret_entry(entry),
            Secrets::Removal(removal) => *self.submit_secret_removal(removal),
            list @ Secrets::List(_) => Overlay::BotSecrets(Box::new(list)),
        }
    }

    fn submit_secret_entry(&mut self, mut entry: SecretEntry) -> Overlay {
        let text = entry.input.text();
        match entry.asking.clone() {
            Asking::Name => match secrets::name_refusal(&text) {
                None => {
                    entry.asking = Asking::Value(text);
                    entry.input = Editor::default();
                    entry.error = None;
                }
                Some(why) => entry.error = Some(why),
            },
            Asking::Value(name) => match self.save_secret(entry.bot, &name, &text) {
                None => return *entry.back,
                Some(why) => entry.error = Some(why),
            },
        }
        Overlay::BotSecrets(Box::new(Secrets::Entry(entry)))
    }

    fn save_secret(&mut self, key: u64, name: &str, value: &str) -> Option<String> {
        let at = self.panels.bots.by_key(key)?;
        let id = self.panels.bots.live[at].bot.id.clone();
        self.bot_secret_set(None, &id, (name, value)).err().map(|refusal| refusal.message)
    }

    fn submit_secret_removal(&mut self, removal: SecretRemoval) -> Box<Overlay> {
        let at = self.panels.bots.by_key(removal.bot);
        let stale = removal.name.is_none()
            && at.is_some_and(|at| fingerprint(&self.panels.bots.live[at].secrets) != removal.seen);
        let id = at.map(|at| self.panels.bots.live[at].bot.id.clone());
        let done = id.map(|id| {
            if stale {
                return Err(crate::control::contract::Refusal::new(
                    crate::control::contract::Code::Conflict,
                    "the secrets changed since you asked: look at the list and ask again",
                ));
            }
            self.bot_secret_remove(None, &id, (removal.name.as_deref(), removal.name.is_none()))
        });
        if let Some(Err(refusal)) = done {
            let toast = crate::app::toast::Toast::new(refusal.message, ui::ToastIcon::Bug);
            self.messages.toast = Some(toast);
        }
        removal.back
    }
}

impl Overlay {
    pub(in crate::app) fn refuses_paste(&mut self, text: &str) -> bool {
        let Self::BotSecrets(secrets) = self else { return false };
        let Secrets::Entry(entry) = &mut **secrets else { return false };
        if !matches!(entry.asking, Asking::Value(_)) || !pasted(text).chars().any(char::is_control) {
            return false;
        }
        entry.error = Some(String::from(ONE_LINE));
        true
    }
}

pub(in crate::app) fn pasted(text: &str) -> &str {
    text.strip_suffix("\r\n").or_else(|| text.strip_suffix('\n')).unwrap_or(text)
}
