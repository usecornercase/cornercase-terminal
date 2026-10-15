use std::path::PathBuf;

use super::store::{self, Listing, Origin, Saved};
use super::{Flow, builtin, share};
use crate::agents::Known;

pub(crate) const DONE: &str = "done";
pub(crate) const EDIT: &str = "edit";
pub(crate) const DUPLICATE: &str = "duplicate";
pub(crate) const DELETE: &str = "delete";
pub(crate) const EXPORT: &str = "export";
pub(crate) const IMPORT: &str = "import";
pub(crate) const NEW: &str = "new";
pub(crate) const SAVE: &str = "save";
pub(crate) const CANCEL: &str = "cancel";
pub(crate) const DIAGRAM: &str = "diagram";
pub(crate) const FORM: &str = "form";
pub(crate) const LIST: &str = "list";
pub(crate) const DELETE_ROLE: &str = "delete role";
pub(crate) const DISCARD: &str = "discard";
pub(crate) const ADD_ROLE: &str = "+ add role";
pub(crate) const RESTORE: &str = "restore default";
const IMPORT_HINT: &str = "enter starts a new line · import opens the flow to check it · esc goes back";
const BUILT_IN_HINT: &str = "a built-in flow: edit saves your version in your folder, in its place";

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
    Export(Box<Flow>),
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
    Restore(usize),
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

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Preview<'a> {
    Flow(&'a Flow),
    Note(String),
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
    pub(crate) last: Option<Origin>,
    pub(crate) project: Option<u64>,
    pub(crate) builtins: Vec<String>,
    pub(crate) importing: Option<Importing>,
    pub(crate) preview: u16,
    pub(crate) diagram: bool,
}

impl Dialog {
    pub(crate) fn new(listing: Listing, agents: Agents) -> Self {
        Self {
            flows: listing.flows,
            broken: listing.broken,
            builtins: listing.builtins,
            cursor: 0,
            edit: None,
            confirm: None,
            notice: None,
            agents,
            last: None,
            project: None,
            importing: None,
            preview: 0,
            diagram: false,
        }
    }

    pub(crate) fn set_listing(&mut self, listing: Listing) {
        let before = self.selected().map(|saved| (saved.origin.clone(), saved.flow.name.clone()));
        self.flows = listing.flows;
        self.broken = listing.broken;
        self.builtins = listing.builtins;
        self.cursor = self.cursor.min(self.entries().saturating_sub(1));
        if before != self.selected().map(|saved| (saved.origin.clone(), saved.flow.name.clone())) {
            self.preview = 0;
        }
    }

    pub(crate) fn preview(&self) -> Option<Preview<'_>> {
        if self.importing.is_some() {
            return None;
        }
        if let Some(edit) = &self.edit {
            return edit.text.is_none().then_some(Preview::Flow(&edit.flow));
        }
        if let Some(saved) = self.selected() {
            return Some(Preview::Flow(&saved.flow));
        }
        match self.broken.get(self.cursor.saturating_sub(self.flows.len())) {
            Some((path, reason)) if self.cursor >= self.flows.len() => {
                let reason = reason.split_once("`: ").map_or(reason.as_str(), |(_, rest)| rest);
                let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                Some(Preview::Note(format!("{file} cannot be read as a flow, so it has no diagram: {reason}")))
            }
            _ => Some(Preview::Note("no flows yet: new makes one, import brings one in".into())),
        }
    }

    pub(crate) fn preview_scroll(&self) -> u16 {
        self.edit.as_ref().map_or(self.preview, |edit| edit.preview)
    }

    pub(crate) fn scroll_preview(&mut self, delta: isize, most: u16) {
        match &mut self.edit {
            Some(edit) => edit.scroll_preview(delta, most),
            None => self.preview = scrolled(self.preview, delta, most),
        }
    }

    pub(crate) fn diagram(&self) -> bool {
        self.edit.as_ref().map_or(self.diagram, |edit| edit.diagram)
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
            if i != self.cursor {
                self.preview = 0;
            }
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
            Origin::BuiltIn | Origin::User(_) => Place::Yours,
            Origin::Project(_) => Place::Project,
        };
        let path = saved.origin.path().map(PathBuf::from);
        let (origin, flow) = (saved.origin.clone(), saved.flow.clone());
        self.last = Some(origin);
        self.edit = Some(Box::new(Editing::new(flow, place, path)));
        self.notice = None;
    }

    pub(crate) fn open_new(&mut self) {
        let mut flow = builtin::starter(&self.agents.installed, self.agents.default);
        flow.name = self.unique(&flow.name, "");
        self.edit = Some(Box::new(Editing::new(flow.clone(), Place::Yours, None)));
        self.notice = None;
    }

    pub(crate) fn unique(&self, name: &str, suffix: &str) -> String {
        let taken = |candidate: &str| {
            self.flows.iter().any(|s| store::same_name(&s.flow.name, candidate))
                || self.builtins.iter().any(|built| store::same_name(built, candidate))
        };
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

    pub(crate) fn export(&self) -> Action {
        match self.selected() {
            Some(saved) => Action::Export(Box::new(saved.flow.clone())),
            None => Action::None,
        }
    }

    pub(crate) fn ask_delete(&mut self) {
        let deletable = self.selected().is_none_or(|saved| saved.origin.editable());
        if self.cursor < self.entries() && deletable {
            let edited = self.selected().is_some_and(|saved| store::edits_builtin(&self.builtins, saved));
            self.confirm = Some(if edited { Confirm::Restore(self.cursor) } else { Confirm::Delete(self.cursor) });
        }
    }

    pub(crate) fn saved(&mut self, path: PathBuf) {
        if let Some(edit) = &mut self.edit {
            self.last = Some(match edit.place {
                Place::Yours => Origin::User(path.clone()),
                Place::Project => Origin::Project(path.clone()),
            });
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
            Some(Confirm::Delete(i) | Confirm::Restore(i)) => self.path_at(i).map_or(Action::None, Action::Delete),
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
            Confirm::Restore(i) => {
                let name = self.flows.get(i).map(|saved| saved.flow.name.trim().to_string()).unwrap_or_default();
                let message = format!("Bring back the built-in {name}? Your version's file goes.");
                Some(("Restore default", message, RESTORE))
            }
            Confirm::Leave(_) => {
                let name = self.edit.as_ref().map(|e| e.flow.name.trim().to_string()).unwrap_or_default();
                Some(("Unsaved changes", format!("{name} has changes that are not saved. Discard them?"), DISCARD))
            }
        }
    }

    pub(crate) fn title(&self) -> String {
        if self.importing.is_some() {
            return "Flows › import".into();
        }
        match &self.edit {
            Some(edit) if edit.flow.name.trim().is_empty() => "Flows › untitled".into(),
            Some(edit) => format!("Flows › {}", edit.flow.name.trim()),
            None => "Flows".into(),
        }
    }

    pub(crate) fn list_rows(&self) -> Vec<RowText> {
        let flows = self.flows.iter().map(|saved| flow_row(&self.flows, &self.builtins, saved));
        let broken = self.broken.iter().map(|(path, reason)| RowText {
            label: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            value: reason.clone(),
            red: true,
            ..RowText::default()
        });
        flows.chain(broken).collect()
    }

    pub(crate) fn buttons(&self, room: Room) -> Vec<&'static str> {
        if self.importing.is_some() {
            return vec![IMPORT, CANCEL];
        }
        if let Some(edit) = &self.edit {
            return edit.buttons(room);
        }
        let mut buttons = Vec::new();
        match self.selected() {
            Some(saved) if store::edits_builtin(&self.builtins, saved) => {
                buttons.extend([EDIT, DUPLICATE, EXPORT, RESTORE]);
            }
            Some(saved) if saved.origin.editable() => buttons.extend([EDIT, DUPLICATE, EXPORT, DELETE]),
            Some(_) => buttons.extend([EDIT, DUPLICATE, EXPORT]),
            None if self.cursor < self.entries() => buttons.push(DELETE),
            None => {}
        }
        if !room.wide {
            buttons.push(if self.diagram { LIST } else { DIAGRAM });
        }
        buttons.extend([IMPORT, NEW, DONE]);
        for dropped in [IMPORT, EXPORT] {
            if room.width > 0 && row_width(&buttons) > room.width {
                buttons.retain(|label| *label != dropped);
            }
        }
        buttons
    }

    pub(crate) fn submit(&self) -> &'static str {
        if self.importing.is_some() {
            return IMPORT;
        }
        self.edit.as_ref().map_or(DONE, |edit| edit.submit())
    }

    pub(crate) fn hint(&self) -> String {
        if let Some(notice) = &self.notice {
            return notice.clone();
        }
        if self.importing.is_some() {
            return IMPORT_HINT.into();
        }
        match &self.edit {
            Some(edit) => edit.hint(),
            None if self.selected().is_some_and(|s| !s.origin.editable()) => BUILT_IN_HINT.into(),
            None => "enter or a click on the selected flow edits it · tab moves between flows".into(),
        }
    }

    pub(crate) fn error(&self) -> Option<String> {
        if let Some(importing) = &self.importing {
            return importing.error.clone();
        }
        self.edit.as_ref().and_then(|edit| edit.field.as_ref()).and_then(|field| field.error.clone())
    }

    pub(crate) fn warnings(&self) -> Vec<String> {
        let Some(edit) = &self.edit else { return Vec::new() };
        let risks = edit.is_project_flow().then(|| project_risks(&edit.flow)).flatten();
        let imported = edit.imported.then(|| share::risks(&edit.flow, "this imported flow")).flatten();
        risks.into_iter().chain(imported).chain(edit.flow.problems().iter().map(|p| share::visible(p))).collect()
    }

    pub(crate) fn press(&mut self, label: &str) -> Action {
        self.notice = None;
        if self.importing.is_some() {
            return self.press_import(label);
        }
        if self.edit.is_some() {
            return self.press_in_edit(label);
        }
        match label {
            EXPORT => return self.export(),
            IMPORT => self.importing = Some(Importing::new("")),
            EDIT => self.open(self.cursor),
            DIAGRAM | LIST => self.diagram = !self.diagram,
            DUPLICATE => return self.duplicate(),
            DELETE | RESTORE => self.ask_delete(),
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

pub(crate) fn flow_row(flows: &[Saved], builtins: &[String], saved: &Saved) -> RowText {
    let problems: Vec<String> = store::conflict(flows, saved).into_iter().chain(saved.flow.problems()).collect();
    let (value, note) = match problems.as_slice() {
        [] => {
            let tag = store::tag(builtins, saved).map(|tag| format!("{tag} · ")).unwrap_or_default();
            (agents_count(&saved.flow), format!("{tag}{}", plan_summary(&saved.flow)))
        }
        [only] => (only.clone(), String::new()),
        [first, rest @ ..] => (format!("{first} (+{} more)", rest.len()), String::new()),
    };
    RowText { label: saved.flow.name.clone(), value, note, red: !problems.is_empty(), ..RowText::default() }
}

pub(crate) fn project_risks(flow: &Flow) -> Option<String> {
    share::key_risks(flow, "this project's flow")
}

fn row_width(labels: &[&str]) -> usize {
    labels.iter().map(|label| label.chars().count() + 2).sum::<usize>() + labels.len().saturating_sub(1)
}

pub(crate) fn scrolled(scroll: u16, delta: isize, most: u16) -> u16 {
    let scroll = i32::from(scroll).saturating_add(i32::try_from(delta).unwrap_or(0)).max(0);
    u16::try_from(scroll).unwrap_or(u16::MAX).min(most)
}

pub(crate) fn agents_count(flow: &Flow) -> String {
    format!("{} agents", flow.agents())
}

pub(crate) fn plan_summary(flow: &Flow) -> String {
    flow.stages.iter().map(|stage| stage.roles.join(" + ")).collect::<Vec<_>>().join(" → ")
}

mod edit;
mod import;
mod input;
mod rows;

pub(crate) use edit::Editing;
#[cfg(test)]
pub(crate) use edit::{Row, Tab};
pub(crate) use import::Importing;

#[cfg(test)]
mod tests;
