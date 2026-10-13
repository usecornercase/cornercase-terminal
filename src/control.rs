use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PANE_ENV: &str = "CORNERCASE_PANE";
pub(crate) const SERVER_ENV: &str = "CORNERCASE_SERVER";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Request {
    #[serde(default)]
    pub(crate) caller: Option<u64>,
    #[serde(default)]
    pub(crate) server: Option<String>,
    #[serde(flatten)]
    pub(crate) command: Command,
}

impl std::fmt::Display for Item {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Group(id) => write!(f, "group-{id}"),
            Self::Project(id) => write!(f, "project-{id}"),
            Self::Workspace(id) => write!(f, "workspace-{id}"),
            Self::Tab(id) => write!(f, "tab-{id}"),
            Self::Pane(id) => write!(f, "pane-{id}"),
        }
    }
}

pub(crate) fn server_token() -> &'static str {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(|| {
        let started = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_nanos());
        format!("{}-{started}", std::process::id())
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", content = "args", rename_all = "kebab-case")]
pub(crate) enum Command {
    Status(Status),
    Open(Open),
    NewWorkspace(NewWorkspace),
    NewTab(NewTab),
    Split(Split),
    Start(Start),
    Send(SendText),
    Keys(Keys),
    Read(Read),
    Wait(Wait),
    WaitSeveral(Wait),
    Close(Close),
    Rename(Rename),
    Focus(Focus),
    Notify(Notify),
    Todo(Todo),
    Events(Events),
    LastMessage(LastMessage),
    RestartWhenIdle(RestartWhenIdle),
    Stats(Stats),
}

impl Command {
    pub(crate) fn name(&self) -> &'static str {
        Self::NAMES[match self {
            Self::Status(_) => 0,
            Self::Open(_) => 1,
            Self::NewWorkspace(_) => 2,
            Self::NewTab(_) => 3,
            Self::Split(_) => 4,
            Self::Start(_) => 5,
            Self::Send(_) => 6,
            Self::Keys(_) => 7,
            Self::Read(_) => 8,
            Self::Wait(_) => 9,
            Self::Close(_) => 10,
            Self::Rename(_) => 11,
            Self::Focus(_) => 12,
            Self::Notify(_) => 13,
            Self::Todo(_) => 14,
            Self::WaitSeveral(_) => 15,
            Self::Events(_) => 16,
            Self::LastMessage(_) => 17,
            Self::RestartWhenIdle(_) => 18,
            Self::Stats(_) => 19,
        }]
    }

    pub(crate) fn fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = Fields::default();
        match self {
            Self::Status(_) | Self::Stats(_) => {}
            Self::Open(open) => {
                fields.add("path", open.path.display()).add("focus", open.focus);
            }
            Self::NewWorkspace(new) => {
                fields.add("name", &new.name).maybe("project", new.project).add("worktree", new.worktree);
                fields.add("focus", new.focus);
            }
            Self::NewTab(new) => {
                fields.maybe("workspace", new.workspace).maybe("name", new.name.as_ref());
                fields.add("command", new.command.is_some()).add("focus", new.focus);
            }
            Self::Split(split) => {
                fields.maybe("pane", split.pane).add("down", split.down).add("command", split.command.is_some());
                fields.add("focus", split.focus);
            }
            Self::Start(start) => {
                fields.maybe("agent", start.agent.as_ref()).maybe("worktree", start.worktree.as_ref());
                fields.maybe("workspace", start.workspace).add("prompt", start.prompt.is_some());
                fields.add("wait", start.wait).maybe("until", start.wait.then(|| start.until.name()));
                fields.maybe("timeout", start.timeout).add("focus", start.focus);
            }
            Self::Send(send) => {
                fields.maybe("pane", send.pane).maybe("tab", send.tab);
                fields.add("bytes", send.text.as_ref().map_or(0, String::len)).add("enter", send.enter);
                fields.add("wait", send.wait).maybe("until", send.wait.then(|| send.until.name()));
                fields.maybe("timeout", send.timeout).add("force", send.force);
            }
            Self::Keys(keys) => {
                fields.maybe("pane", keys.pane).maybe("tab", keys.tab).add("keys", keys.keys.len());
            }
            Self::Read(read) => {
                fields.maybe("pane", read.pane).maybe("tab", read.tab).maybe("lines", read.lines);
            }
            Self::LastMessage(last) => {
                fields.maybe("pane", last.pane).maybe("tab", last.tab);
            }
            Self::Wait(wait) | Self::WaitSeveral(wait) => {
                fields.maybe("pane", wait.pane).maybe("tab", wait.tab).add("until", wait.until.name());
                fields.maybe("timeout", wait.timeout);
                if let Self::WaitSeveral(_) = self {
                    fields.add("panes", listed(&wait.panes)).add("tabs", listed(&wait.tabs));
                    fields.add("mode", if wait.all { "all" } else { "any" });
                }
            }
            Self::Close(close) => {
                fields.add("item", close.item).add("remove_worktree", close.remove_worktree).add("force", close.force);
            }
            Self::Rename(rename) => {
                fields.maybe("item", rename.item).add("name", &rename.name);
            }
            Self::Focus(focus) => {
                fields.add("item", focus.item);
            }
            Self::Notify(notify) => {
                fields.add("bytes", notify.text.len());
            }
            Self::Todo(todo) => {
                match todo {
                    Todo::Add(_) => fields.add("todo", "add"),
                    Todo::List => fields.add("todo", "list"),
                    Todo::Done(id) => fields.add("todo", "done").add("id", id),
                    Todo::Rm(id) => fields.add("todo", "rm").add("id", id),
                };
            }
            Self::Events(events) => {
                let panes: Vec<String> = events.panes.iter().map(u64::to_string).collect();
                fields.maybe("panes", (!panes.is_empty()).then(|| panes.join(",")));
            }
            Self::RestartWhenIdle(restart) => {
                fields.maybe("timeout", restart.timeout);
            }
        }
        fields.0
    }

    pub(crate) const NAMES: [&str; 20] = [
        "status",
        "open",
        "new-workspace",
        "new-tab",
        "split",
        "start",
        "send",
        "keys",
        "read",
        "wait",
        "close",
        "rename",
        "focus",
        "notify",
        "todo",
        "wait-several",
        "events",
        "last-message",
        "restart-when-idle",
        "stats",
    ];
}

fn listed(ids: &[u64]) -> String {
    ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",")
}

#[derive(Default)]
struct Fields(Vec<(&'static str, String)>);

impl Fields {
    fn add(&mut self, key: &'static str, value: impl std::fmt::Display) -> &mut Self {
        self.0.push((key, value.to_string()));
        self
    }

    fn maybe(&mut self, key: &'static str, value: Option<impl std::fmt::Display>) -> &mut Self {
        if let Some(value) = value {
            self.add(key, value);
        }
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Item {
    Group(u64),
    Project(u64),
    Workspace(u64),
    Tab(u64),
    Pane(u64),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Status {}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Open {
    pub(crate) path: PathBuf,
    pub(crate) focus: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct NewWorkspace {
    pub(crate) name: String,
    pub(crate) project: Option<u64>,
    pub(crate) worktree: bool,
    pub(crate) focus: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct NewTab {
    pub(crate) workspace: Option<u64>,
    pub(crate) name: Option<String>,
    pub(crate) command: Option<String>,
    pub(crate) focus: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Split {
    pub(crate) pane: Option<u64>,
    pub(crate) down: bool,
    pub(crate) command: Option<String>,
    pub(crate) focus: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Start {
    pub(crate) agent: Option<String>,
    pub(crate) worktree: Option<String>,
    pub(crate) workspace: Option<u64>,
    pub(crate) name: Option<String>,
    pub(crate) prompt: Option<String>,
    pub(crate) wait: bool,
    pub(crate) until: Until,
    pub(crate) timeout: Option<f64>,
    pub(crate) focus: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SendText {
    pub(crate) pane: Option<u64>,
    pub(crate) tab: Option<u64>,
    pub(crate) text: Option<String>,
    pub(crate) enter: bool,
    pub(crate) wait: bool,
    pub(crate) until: Until,
    pub(crate) timeout: Option<f64>,
    pub(crate) force: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Keys {
    pub(crate) pane: Option<u64>,
    pub(crate) tab: Option<u64>,
    pub(crate) keys: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Read {
    pub(crate) pane: Option<u64>,
    pub(crate) tab: Option<u64>,
    pub(crate) lines: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct LastMessage {
    pub(crate) pane: Option<u64>,
    pub(crate) tab: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Wait {
    pub(crate) pane: Option<u64>,
    pub(crate) tab: Option<u64>,
    pub(crate) panes: Vec<u64>,
    pub(crate) tabs: Vec<u64>,
    pub(crate) all: bool,
    pub(crate) until: Until,
    pub(crate) timeout: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Until {
    #[default]
    Stops,
    Idle,
    Working,
    Waiting,
    Shell,
    #[serde(rename = "turn-over")]
    TurnOver,
    Text(String),
    Quiet(f64),
}

impl Until {
    pub(crate) fn name(&self) -> String {
        match self {
            Self::Stops => "stops".into(),
            Self::Idle => "idle".into(),
            Self::Working => "working".into(),
            Self::Waiting => "waiting".into(),
            Self::Shell => "shell".into(),
            Self::TurnOver => "turn-over".into(),
            Self::Text(_) => "text".into(),
            Self::Quiet(seconds) => format!("quiet {seconds}s"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Close {
    pub(crate) item: Item,
    #[serde(default)]
    pub(crate) remove_worktree: bool,
    #[serde(default)]
    pub(crate) force: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Rename {
    pub(crate) item: Option<Item>,
    pub(crate) name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Focus {
    pub(crate) item: Item,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Notify {
    pub(crate) text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Todo {
    Add(String),
    List,
    Done(u64),
    Rm(u64),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Events {
    pub(crate) panes: Vec<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RestartWhenIdle {
    pub(crate) timeout: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    Project,
    Workspace,
    Tab,
    Pane,
}

impl Kind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Workspace => "workspace",
            Self::Tab => "tab",
            Self::Pane => "pane",
        }
    }

    pub(crate) fn id(self, ids: &Ids) -> Option<u64> {
        match self {
            Self::Project => ids.project,
            Self::Workspace => ids.workspace,
            Self::Tab => ids.tab,
            Self::Pane => ids.pane,
        }
    }

    pub(crate) fn parent(self) -> Option<Self> {
        match self {
            Self::Project => None,
            Self::Workspace => Some(Self::Project),
            Self::Tab => Some(Self::Workspace),
            Self::Pane => Some(Self::Tab),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "lowercase")]
pub(crate) enum What {
    Opened { kind: Kind },
    Closed { kind: Kind },
    Status { agent: String, from: Option<String>, to: Option<String> },
    Exited { program: String },
    Dropped { count: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Event {
    pub(crate) time: String,
    #[serde(flatten)]
    pub(crate) what: What,
    #[serde(flatten)]
    pub(crate) ids: Ids,
}

impl Event {
    pub(crate) fn kind(&self) -> Option<Kind> {
        match &self.what {
            What::Opened { kind } | What::Closed { kind } => Some(*kind),
            What::Status { .. } | What::Exited { .. } => Some(Kind::Pane),
            What::Dropped { .. } => None,
        }
    }

    pub(crate) fn streamed(&self) -> Option<String> {
        serde_json::to_string(&Response::Ok(serde_json::to_value(self).ok()?)).ok()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Response {
    Ok(Value),
    Error(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ids {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) project: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tab: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Done {
    #[serde(flatten)]
    pub(crate) ids: Ids,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ended: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) line: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) todo: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) panes: Vec<Done>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) written: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) turn_over: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Report {
    pub(crate) version: String,
    pub caller: Option<u64>,
    pub shown: Ids,
    pub(crate) groups: Vec<GroupInfo>,
    pub projects: Vec<ProjectInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) keep_awake: Option<KeepAwake>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct KeepAwake {
    pub(crate) held: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) unavailable: Option<String>,
}

impl Report {
    pub(crate) fn workspace_on(&self, branch: &str) -> Result<u64, String> {
        let project = self.here().ok_or("no project is open; open one with `cornercase open PATH`")?;
        let on = |w: &&WorkspaceInfo| w.branch.as_deref() == Some(branch);
        let found: Vec<&WorkspaceInfo> = project.workspaces.iter().filter(on).collect();
        let place = format!("project {} ({})", project.id, project.name);
        match found[..] {
            [workspace] => Ok(workspace.id),
            [] if project.workspaces.is_empty() => {
                Err(format!("{place} has no workspace, so none is on branch {branch}"))
            }
            [] => Err(format!(
                "no workspace of {place} is on branch {branch}; its workspaces:{}",
                candidates(&project.workspaces)
            )),
            _ => Err(format!(
                "branch {branch} is checked out in {} workspaces of {place}; pick one with --workspace:{}",
                found.len(),
                candidates(found.iter().copied())
            )),
        }
    }

    fn here(&self) -> Option<&ProjectInfo> {
        let panes =
            |p: &ProjectInfo| p.workspaces.iter().flat_map(|w| &w.tabs).flat_map(|t| &t.panes).any(|pane| pane.caller);
        let shown = |p: &ProjectInfo| Some(p.id) == self.shown.project;
        self.projects.iter().find(|p| panes(p)).or_else(|| self.projects.iter().find(|p| shown(p)))
    }

    pub(crate) fn loose(&self) -> impl Iterator<Item = &ProjectInfo> {
        let grouped = |p: &ProjectInfo| p.group.is_some_and(|g| self.groups.iter().any(|group| group.id == g));
        self.projects.iter().filter(move |p| !grouped(p))
    }

    pub(crate) fn grouped(&self, group: u64) -> impl Iterator<Item = &ProjectInfo> {
        self.projects.iter().filter(move |p| p.group == Some(group))
    }

    pub fn panes(&self) -> Vec<PaneRow> {
        let projects = self.loose().chain(self.groups.iter().flat_map(|group| self.grouped(group.id)));
        let mut rows = Vec::new();
        for project in projects {
            for workspace in &project.workspaces {
                for tab in &workspace.tabs {
                    rows.extend(tab.panes.iter().map(|pane| PaneRow {
                        pane: pane.id,
                        tab: tab.id,
                        workspace: workspace.id,
                        project: project.id,
                        program: pane.program.clone(),
                        agent: pane.agent.clone(),
                        status: pane.status.clone(),
                        background_shell: pane.background_shell,
                        dialog: pane.dialog,
                        at_prompt: pane.at_prompt,
                        model: pane.model.clone(),
                        context: pane.context,
                        caller: pane.caller,
                        shown: self.shown.pane == Some(pane.id),
                        active: pane.active,
                        tab_name: tab.name.clone(),
                        workspace_name: workspace.name.clone(),
                        branch: workspace.branch.clone(),
                        worktree: workspace.worktree,
                        project_name: project.name.clone(),
                        path: pane.path.clone(),
                        survey: pane.survey,
                    }));
                }
            }
        }
        rows
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaneRows {
    pub panes: Vec<PaneRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_excessive_bools, reason = "each flag is its own key in the JSON of `status --panes`")]
pub struct PaneRow {
    pub pane: u64,
    pub tab: u64,
    pub(crate) workspace: u64,
    pub(crate) project: u64,
    pub(crate) program: Option<String>,
    pub(crate) agent: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) background_shell: bool,
    pub(crate) dialog: bool,
    pub(crate) at_prompt: Option<bool>,
    pub(crate) model: Option<String>,
    pub(crate) context: Option<u16>,
    pub caller: bool,
    pub(crate) shown: bool,
    pub(crate) active: bool,
    pub(crate) tab_name: String,
    pub(crate) workspace_name: String,
    pub(crate) branch: Option<String>,
    pub(crate) worktree: bool,
    pub(crate) project_name: String,
    pub(crate) path: Option<PathBuf>,
    pub(crate) survey: bool,
}

impl PaneRow {
    pub const COLUMNS: [&str; 22] = [
        "pane",
        "tab",
        "workspace",
        "project",
        "program",
        "agent",
        "status",
        "background_shell",
        "dialog",
        "at_prompt",
        "model",
        "context",
        "caller",
        "shown",
        "active",
        "tab_name",
        "workspace_name",
        "branch",
        "worktree",
        "project_name",
        "path",
        "survey",
    ];
}

fn candidates<'a>(workspaces: impl IntoIterator<Item = &'a WorkspaceInfo>) -> String {
    let mut text = String::new();
    for workspace in workspaces {
        let _ = write!(text, "\n  workspace {}  {}", workspace.id, workspace.name);
        if let Some(branch) = &workspace.branch {
            let _ = write!(text, "  branch {branch}");
        }
        if workspace.worktree {
            text.push_str("  worktree");
        }
    }
    text
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct GroupInfo {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) collapsed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectInfo {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub path: PathBuf,
    pub(crate) group: Option<u64>,
    pub(crate) active: bool,
    pub workspaces: Vec<WorkspaceInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceInfo {
    pub id: u64,
    pub name: String,
    pub(crate) path: PathBuf,
    pub(crate) branch: Option<String>,
    pub(crate) worktree: bool,
    pub(crate) behind: u32,
    pub(crate) active: bool,
    pub tabs: Vec<TabInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabInfo {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) status: Option<String>,
    pub(crate) active: bool,
    pub panes: Vec<PaneInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_excessive_bools, reason = "each flag is its own key in the JSON of `status`")]
pub struct PaneInfo {
    pub id: u64,
    pub(crate) path: Option<PathBuf>,
    pub program: Option<String>,
    pub(crate) agent: Option<String>,
    pub status: Option<String>,
    pub(crate) background_shell: bool,
    pub(crate) dialog: bool,
    pub(crate) survey: bool,
    pub(crate) at_prompt: Option<bool>,
    pub(crate) model: Option<String>,
    pub(crate) context: Option<u16>,
    pub(crate) active: bool,
    pub(crate) caller: bool,
    pub(crate) resumes: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TodoList {
    pub(crate) todos: Vec<TodoItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TodoItem {
    pub(crate) id: u64,
    pub(crate) text: String,
    pub(crate) done: bool,
}

pub(crate) fn unknown_command(name: &str) -> String {
    format!(
        "the running cornercase server ({}) has no `{name}` command; update cornercase and restart the server",
        crate::version::CURRENT
    )
}

pub(crate) fn command_name(request: &Value) -> Option<&str> {
    request.get("command").and_then(Value::as_str)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Stats {}

#[cfg(test)]
mod tests;
