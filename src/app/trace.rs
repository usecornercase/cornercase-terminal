use std::collections::{BTreeMap, HashSet};
use std::fmt::Display;
use std::time::{Instant, SystemTime};

use super::layout::Side;
use super::{App, Toast};
use crate::control::{self, Ids, What};
use crate::log::{self, Level};
use crate::project::{Hierarchy, Phase, Workspace};
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
    opener: Option<u64>,
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
    full: bool,
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
            Self::Opened { place, .. } => (
                place,
                What::Opened { kind: place.key.kind.followed()?, parent: place.opener, accounts: BTreeMap::new() },
            ),
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
    pub(crate) fn trace(&mut self) {
        self.clients.events.snapshots.mark_changed();
        self.clients.change_streams.mark_changed();
        self.trace_refresh();
    }

    pub(crate) fn trace_refresh(&mut self) {
        let logged = log::enabled(Level::Info);
        if logged || self.following() {
            self.publish(logged);
        }
        self.settle_views();
        let now = Instant::now();
        self.publish_snapshot(now);
        self.publish_app(now);
        self.publish_flow_runs(now);
    }

    pub(super) fn publish(&mut self, logged: bool) {
        let (notes, events) = self.observe();
        if logged {
            notes.iter().for_each(Note::write);
        }
        self.clients.events.publish(&events);
        self.run_hooks(&events);
    }

    pub(super) fn observe(&mut self) -> (Vec<Note>, Vec<control::Event>) {
        let changes = self.observe_items();
        let mut notes: Vec<Note> = changes.iter().filter_map(Change::note).collect();
        notes.extend(self.observe_ui());
        if changes.is_empty() || !self.following() {
            return (notes, Vec::new());
        }
        let time = crate::time::Stamp(SystemTime::now()).to_string();
        let mut events: Vec<control::Event> = changes.iter().filter_map(|change| change.event(&time)).collect();
        for event in &mut events {
            if let What::Opened { kind: control::Kind::Pane, accounts, .. } = &mut event.what
                && let Some(term) = event.ids.pane.and_then(|pane| self.projects.pane_by(pane))
            {
                accounts.extend(term.accounts().iter().map(|a| (a.kind.name().to_string(), a.name.clone())));
            }
        }
        (notes, events)
    }

    fn traced_places(&self) -> Vec<(Place, Option<PaneNow<'_>>)> {
        let live = self.pane_ids();
        let mut places: Vec<(Place, Option<PaneNow>)> =
            self.groups.iter().map(|g| (place_of(Kind::Group, g.id, None, OPEN, Ids::default()), None)).collect();
        for project in &self.projects {
            let ids = Ids { project: Some(project.id), ..Ids::default() };
            let phase_now = if project.closing { "closing" } else { OPEN };
            places.push((place_of(Kind::Project, project.id, project.group, phase_now, ids), None));
            for workspace in &project.workspaces {
                workspace_places(
                    workspace,
                    (project.id, Ids { workspace: Some(workspace.id), ..ids }),
                    &live,
                    &mut places,
                );
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
                if let Some(p) = self.projects.project_index(key.id) {
                    let project = &self.projects[p];
                    fields.push(("path", project.path.display().to_string()));
                    fields.extend(project.name.clone().map(|name| ("name", name)));
                }
            }
            Kind::Workspace => {
                if let Some(workspace) = self.projects.workspace_by(key.id) {
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
        self.observe_focus(&mut notes);
        self.observe_overlay(&mut notes);
        self.observe_panel(&mut notes);
        let full = self.file_full_screen();
        if full != self.seen.full {
            let text = if full { "file shown full screen" } else { "file back in its panel" };
            notes.push(Note::new(Level::Info, "ui", text));
            self.seen.full = full;
        }
        let menu = self.screen.nav.is_some();
        if menu != self.seen.menu {
            notes.push(Note::new(Level::Info, "ui", if menu { "compact menu opened" } else { "compact menu closed" }));
            self.seen.menu = menu;
        }
        if self.messages.toast != self.seen.toast {
            self.seen.toast.clone_from(&self.messages.toast);
            if let Some(toast) = self.messages.toast.as_ref().filter(|toast| toast.icon == ui::ToastIcon::Bug) {
                notes.push(Note::new(Level::Warn, "ui", "error shown").with("text", &toast.message));
            }
        }
        notes
    }

    fn observe_focus(&mut self, notes: &mut Vec<Note>) {
        let focus = self.focus();
        let pane = self.tab().and_then(|tab| tab.pane()).map(|term| term.id);
        let focus = Focus { project: focus.project, workspace: focus.workspace, tab: focus.tab, pane };
        if focus != self.seen.focus {
            self.seen.focus = focus;
            let note = Note::new(Level::Info, "ui", "focus").maybe("project", focus.project);
            let note = note.maybe("workspace", focus.workspace).maybe("tab", focus.tab);
            notes.push(note.maybe("pane", focus.pane));
        }
    }

    fn observe_overlay(&mut self, notes: &mut Vec<Note>) {
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
    }

    fn observe_panel(&mut self, notes: &mut Vec<Note>) {
        let panel = match self.screen.side {
            Some(Side::Changes) => Some("changes"),
            Some(Side::Todo) => Some("todo"),
            Some(Side::Files) => Some("files"),
            Some(Side::Flow) => Some("flow"),
            None => None,
        };
        if panel != self.seen.panel {
            let note = Note::new(Level::Info, "ui", "panel").with("shown", panel.unwrap_or("none"));
            notes.push(note);
            self.seen.panel = panel;
        }
    }
}

const OPEN: &str = "open";

type Placed<'a> = Vec<(Place, Option<PaneNow<'a>>)>;

fn place_of(kind: Kind, id: u64, parent: Option<u64>, phase: &'static str, ids: Ids) -> Place {
    Place { key: Key { kind, id }, parent, phase, ids, opener: None }
}

fn workspace_places<'a>(
    workspace: &'a Workspace,
    (project, ids): (u64, Ids),
    live: &HashSet<u64>,
    places: &mut Placed<'a>,
) {
    let found = place_of(Kind::Workspace, workspace.id, Some(project), phase(&workspace.phase), ids);
    places.push((found, None));
    for tab in &workspace.tabs {
        let ids = Ids { tab: Some(tab.id), ..ids };
        places.push((place_of(Kind::Tab, tab.id, Some(workspace.id), OPEN, ids), None));
        places.extend(tab.panes.iter().map(|term| {
            let ids = Ids { pane: Some(term.id), ..ids };
            let opener = App::live_parent(term, live);
            (Place { opener, ..place_of(Kind::Pane, term.id, Some(tab.id), OPEN, ids) }, Some(PaneNow::of(term)))
        }));
    }
}
