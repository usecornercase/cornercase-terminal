use std::fmt::Display;

use super::{App, Toast};
use crate::log::{self, Level};
use crate::project::Phase;
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Group,
    Project,
    Workspace,
    Tab,
    Pane,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Group => "group",
            Self::Project => "project",
            Self::Workspace => "workspace",
            Self::Tab => "tab",
            Self::Pane => "pane",
        }
    }

    fn parent(self) -> &'static str {
        match self {
            Self::Group | Self::Project => "group",
            Self::Workspace => "project",
            Self::Tab => "workspace",
            Self::Pane => "tab",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    kind: Kind,
    id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Place {
    key: Key,
    parent: Option<u64>,
    phase: &'static str,
}

#[derive(Debug)]
struct Item {
    place: Place,
    fields: Vec<(&'static str, String)>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Focus {
    project: Option<u64>,
    workspace: Option<u64>,
    tab: Option<u64>,
    pane: Option<u64>,
}

#[derive(Debug, Default)]
pub(super) struct Seen {
    items: Vec<Item>,
    focus: Focus,
    overlay: Option<&'static str>,
    panel: Option<&'static str>,
    menu: bool,
    toast: Option<Toast>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Note {
    pub(super) level: Level,
    pub(super) target: &'static str,
    pub(super) message: String,
    pub(super) fields: Vec<(&'static str, String)>,
}

impl Note {
    fn new(level: Level, target: &'static str, message: impl Into<String>) -> Self {
        Self { level, target, message: message.into(), fields: Vec::new() }
    }

    fn with(mut self, key: &'static str, value: impl Display) -> Self {
        self.fields.push((key, value.to_string()));
        self
    }

    fn maybe(self, key: &'static str, value: Option<impl Display>) -> Self {
        match value {
            Some(value) => self.with(key, value),
            None => self,
        }
    }

    fn write(&self) {
        log::write_fields(self.level, self.target, &self.message, &self.fields);
    }
}

fn phase(phase: &Phase) -> &'static str {
    match phase {
        Phase::Open => "open",
        Phase::Removing(_) => "removing",
        Phase::Closing => "closing",
    }
}

fn closed_note(item: Item) -> Note {
    let mut note =
        Note::new(Level::Info, "app", format!("{} closed", item.place.key.kind.name())).with("id", item.place.key.id);
    note.fields.extend(item.fields);
    note
}

fn changes(before: Place, now: Place) -> Vec<Note> {
    let (kind, id) = (now.key.kind, now.key.id);
    let mut notes = Vec::new();
    if before.parent != now.parent {
        let note = Note::new(Level::Info, "app", format!("{} moved", kind.name())).with("id", id);
        notes.push(note.maybe(kind.parent(), now.parent).maybe("from", before.parent));
    }
    if before.phase != now.phase {
        notes.push(Note::new(Level::Info, "app", format!("{} {}", kind.name(), now.phase)).with("id", id));
    }
    notes
}

impl App {
    pub fn trace(&mut self) {
        if log::enabled(Level::Info) {
            for note in self.observe() {
                note.write();
            }
        }
    }

    pub(super) fn observe(&mut self) -> Vec<Note> {
        let mut notes = self.observe_items();
        notes.extend(self.observe_ui());
        notes
    }

    fn traced_places(&self) -> Vec<Place> {
        let open = "open";
        let mut places: Vec<Place> = self
            .groups
            .iter()
            .map(|g| Place { key: Key { kind: Kind::Group, id: g.id }, parent: None, phase: open })
            .collect();
        for project in &self.projects {
            let key = Key { kind: Kind::Project, id: project.id };
            places.push(Place { key, parent: project.group, phase: if project.closing { "closing" } else { open } });
            for workspace in &project.workspaces {
                let key = Key { kind: Kind::Workspace, id: workspace.id };
                places.push(Place { key, parent: Some(project.id), phase: phase(&workspace.phase) });
                for tab in &workspace.tabs {
                    places.push(Place {
                        key: Key { kind: Kind::Tab, id: tab.id },
                        parent: Some(workspace.id),
                        phase: open,
                    });
                    places.extend(tab.panes.iter().map(|pane| Place {
                        key: Key { kind: Kind::Pane, id: pane.id },
                        parent: Some(tab.id),
                        phase: open,
                    }));
                }
            }
        }
        places.sort_by_key(|place| place.key);
        places
    }

    fn describe(&self, key: Key) -> Vec<(&'static str, String)> {
        let mut fields = Vec::new();
        match key.kind {
            Kind::Group => {
                if let Some(group) = self.groups.iter().find(|g| g.id == key.id) {
                    fields.push(("name", group.entry.name.clone()));
                }
            }
            Kind::Project => {
                if let Some(project) = self.projects.iter().find(|p| p.id == key.id) {
                    fields.push(("path", project.path.display().to_string()));
                    fields.extend(project.name.clone().map(|name| ("name", name)));
                }
            }
            Kind::Workspace => {
                if let Some(workspace) = self.projects.iter().flat_map(|p| &p.workspaces).find(|w| w.id == key.id) {
                    fields.push(("path", workspace.path.display().to_string()));
                    fields.push(("label", workspace.label()));
                    fields.push(("worktree", workspace.worktree.to_string()));
                }
            }
            Kind::Tab | Kind::Pane => {}
        }
        fields
    }

    fn observe_items(&mut self) -> Vec<Note> {
        let mut old = std::mem::take(&mut self.seen.items).into_iter().peekable();
        let (mut closed, mut notes) = (Vec::new(), Vec::new());
        let mut items = Vec::new();
        for place in self.traced_places() {
            closed.extend(std::iter::from_fn(|| old.next_if(|item| item.place.key < place.key)).map(closed_note));
            let fields = if let Some(item) = old.next_if(|item| item.place.key == place.key) {
                notes.extend(changes(item.place, place));
                item.fields
            } else {
                let fields = self.describe(place.key);
                let note = Note::new(Level::Info, "app", format!("{} opened", place.key.kind.name()));
                let mut note = note.with("id", place.key.id).maybe(place.key.kind.parent(), place.parent);
                note.fields.extend(fields.iter().cloned());
                notes.push(note);
                fields
            };
            items.push(Item { place, fields });
        }
        closed.extend(old.map(closed_note));
        closed.reverse();
        closed.append(&mut notes);
        self.seen.items = items;
        closed
    }

    fn observe_ui(&mut self) -> Vec<Note> {
        let mut notes = Vec::new();
        let focus = self.focus();
        let pane = self.tab().and_then(|tab| tab.pane()).map(|term| term.id);
        let focus = Focus { project: focus.project, workspace: focus.workspace, tab: focus.tab, pane };
        if focus != self.seen.focus {
            self.seen.focus = focus;
            let note = Note::new(Level::Info, "ui", "focus").maybe("project", focus.project);
            let note = note.maybe("workspace", focus.workspace).maybe("tab", focus.tab);
            notes.push(note.maybe("pane", focus.pane));
        }
        let overlay = self.overlay.as_ref().map(super::Overlay::name);
        if overlay != self.seen.overlay {
            if let Some(closed) = self.seen.overlay {
                notes.push(Note::new(Level::Info, "ui", "overlay closed").with("kind", closed));
            }
            if let Some(opened) = overlay {
                notes.push(Note::new(Level::Info, "ui", "overlay opened").with("kind", opened));
            }
            self.seen.overlay = overlay;
        }
        let panel = if self.changes.open {
            Some("changes")
        } else if self.todo.open {
            Some("todo")
        } else if self.files.open {
            Some("files")
        } else {
            None
        };
        if panel != self.seen.panel {
            let note = Note::new(Level::Info, "ui", "panel").with("shown", panel.unwrap_or("none"));
            notes.push(note);
            self.seen.panel = panel;
        }
        let menu = self.nav.is_some();
        if menu != self.seen.menu {
            notes.push(Note::new(Level::Info, "ui", if menu { "compact menu opened" } else { "compact menu closed" }));
            self.seen.menu = menu;
        }
        if self.toast != self.seen.toast {
            self.seen.toast.clone_from(&self.toast);
            if let Some(toast) = self.toast.as_ref().filter(|toast| toast.icon == ui::ToastIcon::Bug) {
                notes.push(Note::new(Level::Warn, "ui", "error shown").with("text", &toast.message));
            }
        }
        notes
    }
}
