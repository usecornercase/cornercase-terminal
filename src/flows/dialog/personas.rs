use std::collections::BTreeMap;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};

use super::{CANCEL, DONE, EDIT, IMPORT, NEW, Room, RowText, SAVE};
use crate::flows::Flow;
use crate::flows::persona::{self, Listing, Persona};

pub(crate) const PERSONAS: &str = "personas";
pub(crate) const REMOVE: &str = "remove";
const LIST_HINT: &str =
    "enter or a click on the selected persona edits it · new writes one of yours · import brings some in";
const BROKEN_HINT: &str = "this file cannot be read as a persona: remove deletes it, or fix it in an editor";
const EMPTY: &str = "no personas yet: new writes one of yours, import brings them from a repository, an address or a \
                     folder, such as msitarzewski/agency-agents";

pub(crate) mod catalogue;
mod form;

pub(crate) use catalogue::{Catalogue, Division, Fetched, Offered};
pub(crate) use form::Form;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Saving {
    pub(crate) id: Option<String>,
    pub(crate) text: String,
    pub(crate) replace: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PageAction {
    None,
    Back,
    Save(Saving),
    Remove(String),
    Fetch(String),
    Import(Vec<Offered>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ask {
    Remove(String),
    Leave,
    Import,
}

#[derive(Debug)]
pub(crate) struct Page {
    pub(crate) library: Vec<Persona>,
    pub(crate) broken: Vec<(PathBuf, String)>,
    pub(crate) followers: BTreeMap<String, Vec<String>>,
    pub(crate) cursor: usize,
    pub(crate) form: Option<Box<Form>>,
    pub(crate) catalogue: Option<Box<Catalogue>>,
    pub(crate) ask: Option<Ask>,
    pub(crate) notice: Option<String>,
}

pub(crate) fn followers(flows: &[Flow]) -> BTreeMap<String, Vec<String>> {
    let mut by_id: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for flow in flows {
        let roles = std::iter::once(&flow.orchestrator).chain(&flow.roles);
        for id in roles.filter_map(|role| role.persona()) {
            let names = by_id.entry(id.to_string()).or_default();
            if !names.iter().any(|name| name == flow.name.trim()) {
                names.push(flow.name.trim().to_string());
            }
        }
    }
    by_id
}

impl Page {
    pub(crate) fn new(listing: Listing, followers: BTreeMap<String, Vec<String>>) -> Self {
        Self {
            library: listing.personas,
            broken: listing.broken,
            followers,
            cursor: 0,
            form: None,
            catalogue: None,
            ask: None,
            notice: None,
        }
    }

    pub(crate) fn set_listing(&mut self, listing: Listing, followers: BTreeMap<String, Vec<String>>) {
        let held = self.selected().map(|persona| persona.id.clone());
        self.library = listing.personas;
        self.broken = listing.broken;
        self.followers = followers;
        let found = held.and_then(|id| self.library.iter().position(|persona| persona.id == id));
        self.cursor = found.unwrap_or_else(|| self.cursor.min(self.entries().saturating_sub(1)));
    }

    pub(crate) fn entries(&self) -> usize {
        self.library.len() + self.broken.len()
    }

    pub(crate) fn selected(&self) -> Option<&Persona> {
        self.library.get(self.cursor)
    }

    fn broken_at(&self, i: usize) -> Option<&(PathBuf, String)> {
        i.checked_sub(self.library.len()).and_then(|i| self.broken.get(i))
    }

    pub(crate) fn select(&mut self, i: usize) {
        if i < self.entries() {
            self.cursor = i;
        }
    }

    pub(crate) fn title(&self) -> String {
        if let Some(form) = &self.form {
            return format!("Flows › personas › {}", form.title());
        }
        if self.catalogue.is_some() {
            return "Flows › personas › import".into();
        }
        "Flows › personas".into()
    }

    pub(crate) fn rows(&self) -> Vec<RowText> {
        if let Some(form) = &self.form {
            return form.rows();
        }
        if let Some(catalogue) = &self.catalogue {
            return catalogue.rows();
        }
        let personas = self.library.iter().map(|persona| RowText {
            label: label(persona),
            value: persona.description.clone(),
            note: persona.source.as_deref().map_or_else(|| "yours".to_string(), short_source),
            ..RowText::default()
        });
        let broken = self.broken.iter().map(|(path, reason)| RowText {
            label: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            value: reason.clone(),
            red: true,
            ..RowText::default()
        });
        personas.chain(broken).collect()
    }

    pub(crate) fn cursor(&self) -> usize {
        match (&self.form, &self.catalogue) {
            (Some(form), _) => form.cursor,
            (None, Some(catalogue)) => catalogue.cursor,
            (None, None) => self.cursor,
        }
    }

    pub(crate) fn note(&self) -> Option<String> {
        if self.form.is_some() || self.catalogue.is_some() {
            return None;
        }
        if let Some((path, reason)) = self.broken_at(self.cursor) {
            return Some(format!("{} cannot be read as a persona: {reason}", path.display()));
        }
        let Some(persona) = self.selected() else { return Some(EMPTY.into()) };
        Some(self.about(persona))
    }

    fn about(&self, persona: &Persona) -> String {
        let mut lines = vec![label(persona)];
        if !persona.description.is_empty() {
            lines.push(persona.description.clone());
        }
        lines.push(format!("id {} · {} KiB", persona.id, persona.text.len().div_ceil(1024)));
        lines.push(persona.source.as_ref().map_or_else(|| "yours".to_string(), |source| format!("from {source}")));
        lines.push(match self.followers.get(&persona.id) {
            Some(flows) => format!("followed in {}", flows.join(", ")),
            None => "no flow follows it yet: pick it in a role's persona row".into(),
        });
        lines.join("\n\n")
    }

    pub(crate) fn buttons(&self) -> Vec<&'static str> {
        if self.form.is_some() {
            return vec![SAVE, CANCEL];
        }
        if let Some(catalogue) = &self.catalogue {
            return catalogue.buttons();
        }
        let mut buttons = Vec::new();
        if self.selected().is_some() {
            buttons.push(EDIT);
        }
        if self.cursor < self.entries() {
            buttons.push(REMOVE);
        }
        buttons.extend([IMPORT, NEW, DONE]);
        buttons
    }

    pub(crate) fn submit(&self) -> &'static str {
        match (&self.form, &self.catalogue) {
            (Some(_), _) => SAVE,
            (None, Some(catalogue)) => catalogue.submit(),
            (None, None) => DONE,
        }
    }

    pub(crate) fn hint(&self) -> String {
        if let Some(notice) = &self.notice {
            return notice.clone();
        }
        match (&self.form, &self.catalogue) {
            (Some(form), _) => form.hint(),
            (None, Some(catalogue)) => catalogue.hint(),
            (None, None) if self.broken_at(self.cursor).is_some() => BROKEN_HINT.into(),
            (None, None) => LIST_HINT.into(),
        }
    }

    pub(crate) fn error(&self) -> Option<String> {
        match (&self.form, &self.catalogue) {
            (Some(form), _) => form.error.clone(),
            (None, Some(catalogue)) => catalogue.error.clone(),
            (None, None) => None,
        }
    }

    pub(crate) fn warnings(&self) -> Vec<String> {
        self.catalogue.as_ref().map(|catalogue| catalogue.warnings()).unwrap_or_default()
    }

    pub(crate) fn ask_text(&self) -> Option<(&'static str, String, &'static str)> {
        match self.ask.as_ref()? {
            Ask::Remove(id) => Some(("Remove persona", self.remove_text(id), REMOVE)),
            Ask::Leave => {
                let name = self.form.as_ref().map(|form| form.title()).unwrap_or_default();
                Some(("Unsaved changes", format!("{name} has changes that are not saved. Discard them?"), "discard"))
            }
            Ask::Import => self.catalogue.as_ref().map(|catalogue| ("Import personas", catalogue.question(), IMPORT)),
        }
    }

    fn remove_text(&self, id: &str) -> String {
        let name = self.library.iter().find(|persona| persona.id == id).map_or(id, |persona| persona.name.as_str());
        match self.followers.get(id) {
            Some(flows) => format!(
                "Remove the persona {name}? {} follow it, and cannot start until you pick another one or import it \
                 again. Running flows keep their copy.",
                flows.join(", ")
            ),
            None => format!("Remove the persona {name}? Its file goes; no flow follows it."),
        }
    }

    pub(crate) fn answered(&mut self, yes: bool) -> PageAction {
        let Some(ask) = self.ask.take() else { return PageAction::None };
        if !yes {
            return PageAction::None;
        }
        match ask {
            Ask::Remove(id) => PageAction::Remove(id),
            Ask::Leave => {
                self.form = None;
                PageAction::None
            }
            Ask::Import => self.catalogue.as_ref().map_or(PageAction::None, |catalogue| catalogue.chosen()),
        }
    }

    pub(crate) fn press(&mut self, label: &str) -> PageAction {
        self.notice = None;
        if let Some(form) = &mut self.form {
            return match form.press(label) {
                form::Pressed::Save(saving) => PageAction::Save(saving),
                form::Pressed::Leave if form.dirty() => {
                    self.ask = Some(Ask::Leave);
                    PageAction::None
                }
                form::Pressed::Leave => {
                    self.form = None;
                    PageAction::None
                }
                form::Pressed::Stay => PageAction::None,
            };
        }
        if self.catalogue.is_some() {
            return self.press_catalogue(label);
        }
        self.press_list(label)
    }

    fn press_list(&mut self, label: &str) -> PageAction {
        match label {
            EDIT => self.edit(),
            REMOVE => self.ask_remove(),
            NEW => self.form = Some(Box::new(Form::new(None))),
            IMPORT => self.catalogue = Some(Box::default()),
            DONE => return PageAction::Back,
            _ => {}
        }
        PageAction::None
    }

    fn press_catalogue(&mut self, label: &str) -> PageAction {
        let Some(catalogue) = &mut self.catalogue else { return PageAction::None };
        let pressed = catalogue.press(label);
        self.after_catalogue(pressed)
    }

    fn edit(&mut self) {
        if let Some(persona) = self.selected() {
            self.form = Some(Box::new(Form::new(Some(persona))));
        }
    }

    fn ask_remove(&mut self) {
        if let Some(persona) = self.selected() {
            self.ask = Some(Ask::Remove(persona.id.clone()));
        } else if let Some((path, _)) = self.broken_at(self.cursor) {
            let id = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
            self.ask = Some(Ask::Remove(id));
        }
    }

    pub(crate) fn activate(&mut self, i: usize) -> PageAction {
        if let Some(form) = &mut self.form {
            form.select(i);
            form.activate();
            return PageAction::None;
        }
        if let Some(catalogue) = &mut self.catalogue {
            catalogue.toggle(i);
            return PageAction::None;
        }
        if i == self.cursor {
            self.edit();
        } else {
            self.select(i);
        }
        PageAction::None
    }

    pub(crate) fn key(&mut self, key: KeyEvent, room: Room) -> PageAction {
        self.notice = None;
        if self.ask.is_some() {
            return match key.code {
                KeyCode::Enter => self.answered(true),
                KeyCode::Esc => self.answered(false),
                _ => PageAction::None,
            };
        }
        if self.form.is_some() {
            return self.form_key(key, room);
        }
        if let Some(catalogue) = &mut self.catalogue {
            let pressed = catalogue.key(key, room);
            return self.after_catalogue(pressed);
        }
        self.list_key(key)
    }

    fn after_catalogue(&mut self, pressed: catalogue::Pressed) -> PageAction {
        match pressed {
            catalogue::Pressed::Fetch(source) => PageAction::Fetch(source),
            catalogue::Pressed::Ask => {
                self.ask = Some(Ask::Import);
                PageAction::None
            }
            catalogue::Pressed::Close => {
                self.catalogue = None;
                PageAction::None
            }
            catalogue::Pressed::Stay => PageAction::None,
        }
    }

    fn form_key(&mut self, key: KeyEvent, room: Room) -> PageAction {
        let Some(form) = &mut self.form else { return PageAction::None };
        match form.key(key, room) {
            form::Pressed::Save(saving) => PageAction::Save(saving),
            form::Pressed::Leave => self.press(CANCEL),
            form::Pressed::Stay => PageAction::None,
        }
    }

    fn list_key(&mut self, key: KeyEvent) -> PageAction {
        match key.code {
            KeyCode::Esc => return PageAction::Back,
            KeyCode::Enter => self.edit(),
            KeyCode::Up | KeyCode::BackTab => self.select(self.cursor.saturating_sub(1)),
            KeyCode::Down | KeyCode::Tab => self.select(self.cursor + 1),
            KeyCode::Delete | KeyCode::Backspace => self.ask_remove(),
            _ => {}
        }
        PageAction::None
    }

    pub(crate) fn wheel(&mut self, delta: isize, room: Room) {
        if let Some(form) = &mut self.form {
            form.wheel(delta, room);
        } else if let Some(catalogue) = &mut self.catalogue {
            catalogue.wheel(delta);
        } else {
            self.select(self.cursor.saturating_add_signed(delta).min(self.entries().saturating_sub(1)));
        }
    }

    pub(crate) fn saved(&mut self, persona: &str) {
        self.form = None;
        self.notice = Some(format!("saved {persona}"));
    }

    pub(crate) fn refused(&mut self, error: String) {
        match (&mut self.form, &mut self.catalogue) {
            (Some(form), _) => form.error = Some(error),
            (None, Some(catalogue)) => catalogue.error = Some(error),
            (None, None) => self.notice = Some(error),
        }
    }

    pub(crate) fn imported(&mut self, count: usize) {
        self.catalogue = None;
        self.notice = Some(if count == 1 { "imported 1 persona".into() } else { format!("imported {count} personas") });
    }
}

pub(crate) fn label(persona: &Persona) -> String {
    if persona.emoji.is_empty() { persona.name.clone() } else { format!("{} {}", persona.emoji, persona.name) }
}

fn short_source(source: &str) -> String {
    let file = source.rsplit(['/', '\\']).next().unwrap_or(source);
    let repo: Vec<&str> = source.split('/').take(2).collect();
    match repo.as_slice() {
        [owner, name] if !source.contains("://") && persona::source::repo_path(source).is_some() => {
            format!("{owner}/{name}")
        }
        _ => file.to_string(),
    }
}

#[cfg(test)]
mod tests;
