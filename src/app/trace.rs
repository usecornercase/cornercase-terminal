use std::fmt::Display;
use std::time::SystemTime;

use super::{App, Toast};
use crate::control::{self, Ids, What};
use crate::log::{self, Level};
use crate::project::Phase;
use crate::term::Term;
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

    fn followed(self) -> Option<control::Kind> {
        match self {
            Self::Group => None,
            Self::Project => Some(control::Kind::Project),
            Self::Workspace => Some(control::Kind::Workspace),
            Self::Tab => Some(control::Kind::Tab),
            Self::Pane => Some(control::Kind::Pane),
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
    ids: Ids,
}

#[derive(Debug, Clone, Copy)]
struct PaneNow<'a> {
    agent: Option<&'a str>,
    state: Option<&'static str>,
    job: Option<(i32, &'a str)>,
}

impl<'a> PaneNow<'a> {
    fn of(term: &'a Term) -> Self {
        Self {
            agent: term.agent.agent(),
            state: term.agent.state(),
            job: term.job.as_ref().map(|job| (job.pid, job.program.as_str())),
        }
    }
}

#[derive(Debug, Default)]
struct PaneSeen {
    agent: Option<String>,
    state: Option<&'static str>,
    job: Option<(i32, String)>,
}

impl PaneSeen {
    fn follow(&mut self, now: PaneNow, place: Place, changes: &mut Vec<Change>) {
        let status = |agent: &str, from, to| Change::Status { place, agent: agent.to_string(), from, to };
        if self.agent.as_deref() == now.agent {
            if let Some(agent) = now.agent
                && self.state != now.state
            {
                changes.push(status(agent, self.state, now.state));
            }
        } else {
            if let Some(agent) = &self.agent
                && self.state.is_some()
            {
                changes.push(status(agent, self.state, None));
            }
            if let Some(agent) = now.agent
                && now.state.is_some()
            {
                changes.push(status(agent, None, now.state));
            }
            self.agent = now.agent.map(str::to_string);
        }
        self.state = now.state;
        if let Some((pid, program)) = &self.job
            && now.job.is_none_or(|(now, _)| now != *pid)
        {
            changes.push(Change::Exited { place, program: program.clone() });
        }
        if self.job.as_ref().map(|(pid, program)| (*pid, program.as_str())) != now.job {
            self.job = now.job.map(|(pid, program)| (pid, program.to_string()));
        }
    }
}

#[derive(Debug)]
struct Item {
    place: Place,
    fields: Vec<(&'static str, String)>,
    pane: PaneSeen,
}

impl Item {
    fn closed(self) -> Change {
        Change::Closed { place: self.place, fields: self.fields }
    }
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

#[derive(Debug)]
enum Change {
    Opened { place: Place, fields: Vec<(&'static str, String)> },
    Closed { place: Place, fields: Vec<(&'static str, String)> },
    Moved { place: Place, from: Option<u64> },
    Phase { place: Place },
    Status { place: Place, agent: String, from: Option<&'static str>, to: Option<&'static str> },
    Exited { place: Place, program: String },
}

impl Change {
    fn note(&self) -> Option<Note> {
        let item = |place: &Place, what: &str| {
            Note::new(Level::Info, "app", format!("{} {what}", place.key.kind.name())).with("id", place.key.id)
        };
        Some(match self {
            Self::Opened { place, fields } => {
                let mut note = item(place, "opened").maybe(place.key.kind.parent(), place.parent);
                note.fields.extend(fields.iter().cloned());
                note
            }
            Self::Closed { place, fields } => {
                let mut note = item(place, "closed");
                note.fields.extend(fields.iter().cloned());
                note
            }
            Self::Moved { place, from } => {
                item(place, "moved").maybe(place.key.kind.parent(), place.parent).maybe("from", *from)
            }
            Self::Phase { place } => item(place, place.phase),
            Self::Status { .. } | Self::Exited { .. } => return None,
        })
    }

    fn event(&self, time: &str) -> Option<control::Event> {
        let (place, what) = match self {
            Self::Opened { place, .. } => (place, What::Opened { kind: place.key.kind.followed()? }),
            Self::Closed { place, .. } => (place, What::Closed { kind: place.key.kind.followed()? }),
            Self::Status { place, agent, from, to } => {
                let (from, to) = (from.map(str::to_string), to.map(str::to_string));
                (place, What::Status { agent: agent.clone(), from, to })
            }
            Self::Exited { place, program } => (place, What::Exited { program: program.clone() }),
            Self::Moved { .. } | Self::Phase { .. } => return None,
        };
        Some(control::Event { time: time.to_string(), what, ids: place.ids })
    }
}

fn phase(phase: &Phase) -> &'static str {
    match phase {
        Phase::Open => "open",
        Phase::Removing(_) => "removing",
        Phase::Closing => "closing",
    }
}

fn changes(before: Place, now: Place) -> Vec<Change> {
    let mut changes = Vec::new();
    if before.parent != now.parent {
        changes.push(Change::Moved { place: now, from: before.parent });
    }
    if before.phase != now.phase {
        changes.push(Change::Phase { place: now });
    }
    changes
}

impl App {
    pub fn trace(&mut self) {
        let logged = log::enabled(Level::Info);
        if logged || self.events.listening() {
            self.publish(logged);
        }
    }

    pub(super) fn publish(&mut self, logged: bool) {
        let (notes, events) = self.observe();
        if logged {
            notes.iter().for_each(Note::write);
        }
        self.events.publish(&events);
    }

    pub(super) fn observe(&mut self) -> (Vec<Note>, Vec<control::Event>) {
        let changes = self.observe_items();
        let mut notes: Vec<Note> = changes.iter().filter_map(Change::note).collect();
        notes.extend(self.observe_ui());
        if changes.is_empty() || !self.events.listening() {
            return (notes, Vec::new());
        }
        let time = log::Stamp(SystemTime::now()).to_string();
        (notes, changes.iter().filter_map(|change| change.event(&time)).collect())
    }

    fn traced_places(&self) -> Vec<(Place, Option<PaneNow<'_>>)> {
        let open = "open";
        let place = |kind, id, parent, phase, ids| Place { key: Key { kind, id }, parent, phase, ids };
        let mut places: Vec<(Place, Option<PaneNow>)> =
            self.groups.iter().map(|g| (place(Kind::Group, g.id, None, open, Ids::default()), None)).collect();
        for project in &self.projects {
            let ids = Ids { project: Some(project.id), ..Ids::default() };
            let phase_now = if project.closing { "closing" } else { open };
            places.push((place(Kind::Project, project.id, project.group, phase_now, ids), None));
            for workspace in &project.workspaces {
                let ids = Ids { workspace: Some(workspace.id), ..ids };
                places
                    .push((place(Kind::Workspace, workspace.id, Some(project.id), phase(&workspace.phase), ids), None));
                for tab in &workspace.tabs {
                    let ids = Ids { tab: Some(tab.id), ..ids };
                    places.push((place(Kind::Tab, tab.id, Some(workspace.id), open, ids), None));
                    places.extend(tab.panes.iter().map(|term| {
                        let ids = Ids { pane: Some(term.id), ..ids };
                        (place(Kind::Pane, term.id, Some(tab.id), open, ids), Some(PaneNow::of(term)))
                    }));
                }
            }
        }
        places.sort_by_key(|(place, _)| place.key);
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

    fn observe_items(&mut self) -> Vec<Change> {
        let mut old = std::mem::take(&mut self.seen.items).into_iter().peekable();
        let (mut closed, mut changed, mut items) = (Vec::new(), Vec::new(), Vec::new());
        for (place, pane) in self.traced_places() {
            closed.extend(std::iter::from_fn(|| old.next_if(|item| item.place.key < place.key)).map(Item::closed));
            let mut item = if let Some(item) = old.next_if(|item| item.place.key == place.key) {
                changed.extend(changes(item.place, place));
                Item { place, ..item }
            } else {
                let fields = self.describe(place.key);
                changed.push(Change::Opened { place, fields: fields.clone() });
                Item { place, fields, pane: PaneSeen::default() }
            };
            if let Some(now) = pane {
                item.pane.follow(now, place, &mut changed);
            }
            items.push(item);
        }
        closed.extend(old.map(Item::closed));
        closed.reverse();
        closed.append(&mut changed);
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
