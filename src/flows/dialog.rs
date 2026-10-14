use std::path::PathBuf;

use super::store::{Listing, Origin, Saved};
use super::{Flow, builtin};
use crate::agents::Known;

pub(crate) const DONE: &str = "done";
pub(crate) const EDIT: &str = "edit";
pub(crate) const DUPLICATE: &str = "duplicate";
pub(crate) const DELETE: &str = "delete";
pub(crate) const NEW: &str = "new";
pub(crate) const SAVE: &str = "save";
pub(crate) const CANCEL: &str = "cancel";
pub(crate) const DIAGRAM: &str = "diagram";
pub(crate) const FORM: &str = "form";
pub(crate) const DELETE_ROLE: &str = "delete role";
pub(crate) const DISCARD: &str = "discard";
pub(crate) const ADD_ROLE: &str = "+ role";
const BUILT_IN_HINT: &str = "built-in flows are read only: duplicate one to change it";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    Yours,
    Project,
}

impl Place {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Yours => "yours",
            Self::Project => "this project",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Action {
    None,
    Close,
    Save { flow: Box<Flow>, place: Place, previous: Option<PathBuf> },
    Duplicate(Box<Flow>),
    Delete(PathBuf),
    ReadModels(Known),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Then {
    List,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Confirm {
    Delete(usize),
    Leave(Then),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Tab(usize),
    Row(usize),
    Remove(usize),
    MoveUp(usize),
    MoveDown(usize),
    Pick(usize),
    Button(&'static str),
    Text { line: usize, col: usize },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Room {
    pub(crate) pick: usize,
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) wide: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RowText {
    pub(crate) section: &'static str,
    pub(crate) label: String,
    pub(crate) value: String,
    pub(crate) note: String,
    pub(crate) red: bool,
    pub(crate) removable: bool,
    pub(crate) movable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Agents {
    pub(crate) installed: Vec<Known>,
    pub(crate) default: Option<Known>,
    pub(crate) project: bool,
    models: Vec<(Known, Vec<String>)>,
    reading: Vec<Known>,
}

impl Agents {
    pub(crate) fn new(installed: Vec<Known>, default: Option<Known>, project: bool) -> Self {
        Self { installed, default, project, models: Vec::new(), reading: Vec::new() }
    }

    pub(crate) fn models(&self, kind: Known) -> Vec<String> {
        let read = self.models.iter().find(|(k, _)| *k == kind).map(|(_, ids)| ids.clone()).unwrap_or_default();
        kind.models().iter().map(|m| (*m).to_string()).chain(read).collect()
    }

    pub(crate) fn reading(&self, kind: Known) -> bool {
        self.reading.contains(&kind)
    }

    fn wants(&mut self, kind: Known) -> bool {
        let unread = kind.catalogue().is_some() && self.models.iter().all(|(k, _)| *k != kind);
        if unread && !self.reading.contains(&kind) {
            self.reading.push(kind);
            return true;
        }
        false
    }
}

#[derive(Debug)]
pub(crate) struct Dialog {
    pub(crate) flows: Vec<Saved>,
    pub(crate) broken: Vec<(PathBuf, String)>,
    pub(crate) cursor: usize,
    pub(crate) edit: Option<Box<Editing>>,
    pub(crate) confirm: Option<Confirm>,
    pub(crate) notice: Option<String>,
    pub(crate) agents: Agents,
}

impl Dialog {
    pub(crate) fn new(listing: Listing, agents: Agents) -> Self {
        Self {
            flows: listing.flows,
            broken: listing.broken,
            cursor: 0,
            edit: None,
            confirm: None,
            notice: None,
            agents,
        }
    }

    pub(crate) fn set_listing(&mut self, listing: Listing) {
        self.flows = listing.flows;
        self.broken = listing.broken;
        self.cursor = self.cursor.min(self.entries().saturating_sub(1));
    }

    pub(crate) fn entries(&self) -> usize {
        self.flows.len() + self.broken.len()
    }

    pub(crate) fn find(&self, origin: &Origin, name: &str) -> Option<usize> {
        self.flows.iter().position(|s| s.origin == *origin && (origin.path().is_some() || s.flow.name == name))
    }

    pub(crate) fn summary(&self) -> String {
        let count = |builtin: bool| self.flows.iter().filter(|s| s.origin.editable() != builtin).count();
        let mut parts = vec![format!("{} built in", count(true))];
        let yours = count(false);
        if yours > 0 {
            parts.push(format!("{yours} yours"));
        }
        parts.join(", ")
    }

    pub(crate) fn select(&mut self, i: usize) {
        if i < self.entries() {
            self.cursor = i;
        }
    }

    fn selected(&self) -> Option<&Saved> {
        self.flows.get(self.cursor)
    }

    fn path_at(&self, i: usize) -> Option<PathBuf> {
        match self.flows.get(i) {
            Some(saved) => saved.origin.path().map(PathBuf::from),
            None => self.broken.get(i - self.flows.len()).map(|(path, _)| path.clone()),
        }
    }

    pub(crate) fn open(&mut self, i: usize) {
        self.select(i);
        let Some(saved) = self.selected() else { return };
        let place = match &saved.origin {
            Origin::BuiltIn => {
                self.notice = Some(BUILT_IN_HINT.into());
                return;
            }
            Origin::User(_) => Place::Yours,
            Origin::Project(_) => Place::Project,
        };
        let path = saved.origin.path().map(PathBuf::from);
        self.edit = Some(Box::new(Editing::new(saved.flow.clone(), place, path)));
        self.notice = None;
    }

    pub(crate) fn open_new(&mut self) {
        let mut flow = builtin::starter(&self.agents.installed, self.agents.default);
        flow.name = self.unique(&flow.name, "");
        self.edit = Some(Box::new(Editing::new(flow.clone(), Place::Yours, None)));
        self.notice = None;
    }

    fn unique(&self, name: &str, suffix: &str) -> String {
        let taken = |candidate: &str| self.flows.iter().any(|s| s.flow.name.trim() == candidate);
        let base = format!("{name}{suffix}");
        (1..=self.flows.len() + 1)
            .map(|n| if n == 1 { base.clone() } else { format!("{base} {n}") })
            .find(|candidate| !taken(candidate))
            .unwrap_or(base)
    }

    pub(crate) fn duplicate(&self) -> Action {
        let Some(saved) = self.selected() else { return Action::None };
        let mut flow = saved.flow.clone();
        flow.name = self.unique(saved.flow.name.trim(), " copy");
        Action::Duplicate(Box::new(flow))
    }

    pub(crate) fn ask_delete(&mut self) {
        let deletable = self.selected().is_none_or(|saved| saved.origin.editable());
        if self.cursor < self.entries() && deletable {
            self.confirm = Some(Confirm::Delete(self.cursor));
        }
    }

    pub(crate) fn saved(&mut self, path: PathBuf) {
        if let Some(edit) = &mut self.edit {
            edit.saved(path);
            self.notice = Some(format!("saved {}", edit.flow.name.trim()));
        }
    }

    pub(crate) fn models_read(&mut self, kind: Known, ids: Vec<String>) {
        self.agents.reading.retain(|k| *k != kind);
        self.agents.models.retain(|(k, _)| *k != kind);
        self.agents.models.push((kind, ids));
        if let Some(edit) = &mut self.edit {
            edit.models_read(kind, &self.agents);
        }
    }

    pub(crate) fn dirty(&self) -> bool {
        self.edit.as_ref().is_some_and(|edit| edit.dirty())
    }

    pub(crate) fn back(&mut self) -> Action {
        if self.edit.is_none() {
            return Action::Close;
        }
        if self.dirty() {
            self.confirm = Some(Confirm::Leave(Then::List));
        } else {
            self.edit = None;
        }
        Action::None
    }

    pub(crate) fn close(&mut self) -> Action {
        if self.confirm.take().is_some() {
            return Action::None;
        }
        if self.dirty() {
            self.confirm = Some(Confirm::Leave(Then::Close));
            return Action::None;
        }
        Action::Close
    }

    pub(crate) fn confirmed(&mut self) -> Action {
        match self.confirm.take() {
            Some(Confirm::Delete(i)) => self.path_at(i).map_or(Action::None, Action::Delete),
            Some(Confirm::Leave(Then::List)) => {
                self.edit = None;
                Action::None
            }
            Some(Confirm::Leave(Then::Close)) => Action::Close,
            None => Action::None,
        }
    }

    pub(crate) fn confirm_text(&self) -> Option<(&'static str, String, &'static str)> {
        match self.confirm? {
            Confirm::Delete(i) => {
                let message = match self.flows.get(i) {
                    Some(saved) => format!("Delete the flow {}? Its file goes too.", saved.flow.name.trim()),
                    None => "Delete this file, which cannot be read as a flow?".to_string(),
                };
                Some(("Delete flow", message, DELETE))
            }
            Confirm::Leave(_) => {
                let name = self.edit.as_ref().map(|e| e.flow.name.trim().to_string()).unwrap_or_default();
                Some(("Unsaved changes", format!("{name} has changes that are not saved. Discard them?"), DISCARD))
            }
        }
    }

    pub(crate) fn title(&self) -> String {
        match &self.edit {
            Some(edit) if edit.flow.name.trim().is_empty() => "Flows › untitled".into(),
            Some(edit) => format!("Flows › {}", edit.flow.name.trim()),
            None => "Flows".into(),
        }
    }

    pub(crate) fn list_rows(&self) -> Vec<RowText> {
        let flows = self.flows.iter().map(|saved| {
            let problems = saved.flow.problems();
            let section = match saved.origin {
                Origin::BuiltIn => "Built in",
                Origin::User(_) => "Yours",
                Origin::Project(_) => "This project",
            };
            let (value, note) = match problems.as_slice() {
                [] => (agents_count(&saved.flow), plan_summary(&saved.flow)),
                [only] => (only.clone(), String::new()),
                [first, rest @ ..] => (format!("{first} (+{} more)", rest.len()), String::new()),
            };
            RowText {
                section,
                label: saved.flow.name.clone(),
                value,
                note,
                red: !problems.is_empty(),
                ..RowText::default()
            }
        });
        let broken = self.broken.iter().map(|(path, reason)| RowText {
            section: "Unreadable files",
            label: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            value: reason.clone(),
            red: true,
            ..RowText::default()
        });
        flows.chain(broken).collect()
    }

    pub(crate) fn buttons(&self, room: Room) -> Vec<&'static str> {
        if let Some(edit) = &self.edit {
            return edit.buttons(room);
        }
        let mut buttons = Vec::new();
        match self.selected() {
            Some(saved) if saved.origin.editable() => buttons.extend([EDIT, DUPLICATE, DELETE]),
            Some(_) => buttons.push(DUPLICATE),
            None if self.cursor < self.entries() => buttons.push(DELETE),
            None => {}
        }
        buttons.extend([NEW, DONE]);
        buttons
    }

    pub(crate) fn submit(&self) -> &'static str {
        self.edit.as_ref().map_or(DONE, |edit| edit.submit())
    }

    pub(crate) fn hint(&self) -> String {
        if let Some(notice) = &self.notice {
            return notice.clone();
        }
        match &self.edit {
            Some(edit) => edit.hint(),
            None if self.selected().is_some_and(|s| !s.origin.editable()) => BUILT_IN_HINT.into(),
            None => "enter or a click on the selected flow edits it · tab moves between flows".into(),
        }
    }

    pub(crate) fn error(&self) -> Option<String> {
        self.edit.as_ref().and_then(|edit| edit.field.as_ref()).and_then(|field| field.error.clone())
    }

    pub(crate) fn warnings(&self) -> Vec<String> {
        let Some(edit) = &self.edit else { return Vec::new() };
        let agents = edit.flow.agents();
        let run = if agents == 1 { "agent runs" } else { "agents run" };
        std::iter::once(format!("{agents} {run} without asking for permission")).chain(edit.flow.problems()).collect()
    }

    pub(crate) fn press(&mut self, label: &str) -> Action {
        self.notice = None;
        if self.edit.is_some() {
            return self.press_in_edit(label);
        }
        match label {
            EDIT => self.open(self.cursor),
            DUPLICATE => return self.duplicate(),
            DELETE => self.ask_delete(),
            NEW => self.open_new(),
            DONE => return Action::Close,
            _ => {}
        }
        Action::None
    }

    fn press_in_edit(&mut self, label: &str) -> Action {
        let Some(edit) = &mut self.edit else { return Action::None };
        let busy = edit.field.is_some() || edit.pick.is_some() || edit.text.is_some();
        match label {
            SAVE if busy => edit.keep(),
            CANCEL | DONE if busy => edit.drop_open(),
            SAVE => return self.save(),
            DONE => return self.back(),
            DIAGRAM | FORM => edit.diagram = !edit.diagram,
            DELETE_ROLE => edit.delete_role(),
            _ => {}
        }
        Action::None
    }

    pub(crate) fn save(&mut self) -> Action {
        let Some(edit) = &mut self.edit else { return Action::None };
        if !edit.flow.problems().is_empty() {
            self.notice = Some("fix the problems in red to save the flow".into());
            return Action::None;
        }
        Action::Save { flow: Box::new(edit.flow.clone()), place: edit.place, previous: edit.path.clone() }
    }

    pub(crate) fn wants_models(&mut self) -> Action {
        let Some(kind) = self.edit.as_ref().and_then(|edit| edit.model_pick()) else { return Action::None };
        if self.agents.wants(kind) { Action::ReadModels(kind) } else { Action::None }
    }
}

pub(crate) fn agents_count(flow: &Flow) -> String {
    format!("{} agents", flow.agents())
}

pub(crate) fn plan_summary(flow: &Flow) -> String {
    flow.stages.iter().map(|stage| stage.roles.join(" + ")).collect::<Vec<_>>().join(" → ")
}

mod edit;
mod input;
mod rows;

pub(crate) use edit::Editing;
#[cfg(test)]
pub(crate) use edit::{Row, Tab};

#[cfg(test)]
mod tests;
