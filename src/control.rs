use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

mod app_state;
pub(crate) mod arrange;
pub(crate) mod config;
pub(crate) mod contract;
mod events;
mod flow;
pub(crate) mod issues;
mod search;
pub(crate) mod start_server;
mod status;
pub(crate) mod surface;
pub(crate) mod terminal;
pub(crate) mod theme;
mod todo;
pub(crate) mod view;
mod watch;

pub(crate) use todo::{Todo, TodoSend};

pub(crate) use app_state::{
    AppState, AppWatch, AwayInfo, InstallInfo, NoUpdate, Notes, ReleaseInfo, UpdateCheck, UpdateInfo, UpdateInstall,
};
pub(crate) use arrange::{Fold, GroupDelete, GroupNew, GroupStyle, Move, MovePane, RemovalCheck};
pub(crate) use config::{ConfigInfo, Configure};
pub(crate) use contract::Contract;
pub(crate) use events::{Event, Events, Go, Kind, What};
pub(crate) use flow::{
    BrokenFlow, Exported, FlowExport, FlowImport, FlowInfo, FlowList, FlowRun, Flows, Imported, MemberInfo, RoleInfo,
    RunInfo,
};
pub(crate) use issues::{IssueList, IssuePeople, IssueRead, IssueSources};
pub(crate) use search::{Hit, Matches, Search};
pub(crate) use start_server::StartServer;
pub(crate) use status::{GroupInfo, KeepAwake, PullRequestInfo};
pub use status::{PaneInfo, PaneRow, PaneRows, ProjectInfo, Report, TabInfo, WorkspaceInfo};
pub(crate) use surface::Surface;
pub(crate) use terminal::{Input, Link, Ratio, Redraw, Resize, Scroll, Selection};
pub(crate) use theme::{Theme, ThemeInfo};
pub(crate) use view::{OpenView, SetView, ViewInfo};
pub(crate) use watch::{Layout, Snapshot, Watch};

pub const PANE_ENV: &str = "CORNERCASE_PANE";
pub(crate) const SERVER_ENV: &str = "CORNERCASE_SERVER";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Request {
    #[serde(default)]
    pub(crate) caller: Option<u64>,
    #[serde(default)]
    pub(crate) server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) contract: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) boot: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) view: Option<u64>,
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
    StartWith(Start),
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
    TodoSend(TodoSend),
    Events(Events),
    LastMessage(LastMessage),
    RestartWhenIdle(RestartWhenIdle),
    Stats(Stats),
    Away(Away),
    Mark(Mark),
    Queue(Queue),
    FlowList(FlowList),
    FlowRun(FlowRun),
    FlowExport(FlowExport),
    FlowImport(FlowImport),
    Search(Search),
    Contract(Contract),
    StartServer(StartServer),
    Watch(Watch),
    Surface(Surface),
    View(OpenView),
    ViewSet(SetView),
    Input(Input),
    Scroll(Scroll),
    Selection(Selection),
    Resize(Resize),
    Ratio(Ratio),
    Link(Link),
    Theme(Theme),
    Go(Go),
    Redraw(Redraw),
    Config(Configure),
    Move(Move),
    MovePane(MovePane),
    Collapse(Fold),
    Expand(Fold),
    GroupNew(GroupNew),
    GroupStyle(GroupStyle),
    GroupDelete(GroupDelete),
    RemovalCheck(RemovalCheck),
    IssueSources(IssueSources),
    IssueList(IssueList),
    IssueRead(IssueRead),
    IssuePeople(IssuePeople),
    UpdateCheck(UpdateCheck),
    UpdateInstall(UpdateInstall),
    App(AppWatch),
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
            Self::Away(_) => 20,
            Self::Mark(_) => 21,
            Self::Queue(_) => 22,
            Self::StartWith(_) => 23,
            Self::FlowList(_) => 24,
            Self::FlowRun(_) => 25,
            Self::FlowExport(_) => 26,
            Self::FlowImport(_) => 27,
            Self::Search(_) => 28,
            Self::Contract(_) => 29,
            Self::StartServer(_) => 30,
            Self::Watch(_) => 31,
            Self::Surface(_) => 32,
            Self::View(_) => 33,
            Self::ViewSet(_) => 34,
            Self::Input(_) => 35,
            Self::Scroll(_) => 36,
            Self::Selection(_) => 37,
            Self::Resize(_) => 38,
            Self::Ratio(_) => 39,
            Self::Link(_) => 40,
            Self::TodoSend(_) => 41,
            Self::Theme(_) => 42,
            Self::Go(_) => 43,
            Self::Redraw(_) => 44,
            Self::Config(_) => 45,
            Self::Move(_) => 46,
            Self::MovePane(_) => 47,
            Self::Collapse(_) => 48,
            Self::Expand(_) => 49,
            Self::GroupNew(_) => 50,
            Self::GroupStyle(_) => 51,
            Self::GroupDelete(_) => 52,
            Self::RemovalCheck(_) => 53,
            Self::IssueSources(_) => 54,
            Self::IssueList(_) => 55,
            Self::IssueRead(_) => 56,
            Self::IssuePeople(_) => 57,
            Self::UpdateCheck(_) => 58,
            Self::UpdateInstall(_) => 59,
            Self::App(_) => 60,
        }]
    }

    #[expect(clippy::too_many_lines, reason = "one arm per request")]
    pub(crate) fn fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = Fields::default();
        match self {
            Self::Queue(_) => drop(fields.add("queue", true)),
            Self::Status(_)
            | Self::Stats(_)
            | Self::Contract(_)
            | Self::Watch(_)
            | Self::Go(_)
            | Self::UpdateCheck(_)
            | Self::UpdateInstall(_)
            | Self::App(_) => {}
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
            Self::Start(start) | Self::StartWith(start) => {
                fields.maybe("agent", start.agent.as_ref()).maybe("model", start.model.as_ref());
                fields.maybe("mode", start.mode.as_ref()).maybe("effort", start.effort.as_ref());
                fields.maybe("worktree", start.worktree.as_ref());
                fields.maybe("project", start.project);
                fields.maybe("workspace", start.workspace).add("prompt", start.prompt.is_some());
                fields.add("issue", start.issue.is_some()).maybe("source", start.source.as_ref());
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
                fields.add("unlock", close.unlock);
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
            Self::Todo(todo) => todo.fields(&mut fields),
            Self::TodoSend(send) => send.fields(&mut fields),
            Self::Events(events) => events.fields(&mut fields),
            Self::RestartWhenIdle(restart) => {
                fields.maybe("timeout", restart.timeout);
            }
            Self::Away(away) => {
                fields.add("off", away.off);
            }
            Self::Mark(mark) => {
                fields.maybe("pane", mark.pane).maybe("tab", mark.tab).add("reviewed", mark.reviewed);
            }
            Self::FlowList(list) => list.fields(&mut fields),
            Self::FlowRun(run) => run.fields(&mut fields),
            Self::FlowExport(export) => export.fields(&mut fields),
            Self::FlowImport(import) => import.fields(&mut fields),
            Self::Search(search) => {
                fields.add("bytes", search.query.len());
            }
            Self::Surface(surface) => surface.fields(&mut fields),
            Self::View(view) => view.fields(&mut fields),
            Self::ViewSet(set) => set.fields(&mut fields),
            Self::Input(input) => input.fields(&mut fields),
            Self::Scroll(scroll) => scroll.fields(&mut fields),
            Self::Selection(selection) => selection.fields(&mut fields),
            Self::Resize(resize) => resize.fields(&mut fields),
            Self::Redraw(redraw) => redraw.fields(&mut fields),
            Self::Ratio(ratio) => ratio.fields(&mut fields),
            Self::Link(link) => link.fields(&mut fields),
            Self::StartServer(start) => start.fields(&mut fields),
            Self::Theme(theme) => theme.fields(&mut fields),
            Self::Config(config) => config.fields(&mut fields),
            Self::Move(moving) => moving.fields(&mut fields),
            Self::MovePane(moving) => moving.fields(&mut fields),
            Self::Collapse(fold) | Self::Expand(fold) => {
                fields.add("item", fold.item);
            }
            Self::GroupNew(new) => {
                fields.add("name", &new.name);
            }
            Self::GroupStyle(style) => style.fields(&mut fields),
            Self::GroupDelete(delete) => delete.fields(&mut fields),
            Self::RemovalCheck(check) => {
                fields.maybe("workspace", check.workspace);
            }
            Self::IssueSources(sources) => sources.fields(&mut fields),
            Self::IssueList(list) => list.fields(&mut fields),
            Self::IssueRead(read) => read.fields(&mut fields),
            Self::IssuePeople(people) => people.fields(&mut fields),
        }
        fields.0
    }

    pub(crate) const NAMES: [&str; 61] = [
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
        "away",
        "mark",
        "queue",
        "start-with",
        "flow-list",
        "flow-run",
        "flow-export",
        "flow-import",
        "search",
        "contract",
        "start-server",
        "watch",
        "surface",
        "view",
        "view-set",
        "input",
        "scroll",
        "selection",
        "resize",
        "ratio",
        "link",
        "todo-send",
        "theme",
        "go",
        "redraw",
        "config",
        "move",
        "move-pane",
        "collapse",
        "expand",
        "group-new",
        "group-style",
        "group-delete",
        "removal-check",
        "issue-sources",
        "issue-list",
        "issue-read",
        "issue-people",
        "update-check",
        "update-install",
        "app",
    ];

    pub(crate) fn chatty(name: &str) -> bool {
        matches!(name, "input" | "scroll" | "selection" | "resize" | "ratio" | "link")
    }
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
    pub(crate) model: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) effort: Option<String>,
    pub(crate) project: Option<u64>,
    pub(crate) worktree: Option<String>,
    pub(crate) workspace: Option<u64>,
    pub(crate) name: Option<String>,
    pub(crate) prompt: Option<String>,
    pub(crate) issue: Option<String>,
    pub(crate) source: Option<String>,
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
    #[expect(clippy::struct_field_names, reason = "the keys request names this JSON field keys")]
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
    #[serde(default)]
    pub(crate) unlock: bool,
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Mark {
    pub(crate) pane: Option<u64>,
    pub(crate) tab: Option<u64>,
    pub(crate) reviewed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Away {
    pub(crate) off: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RestartWhenIdle {
    pub(crate) timeout: Option<f64>,
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
    pub(crate) warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ended: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) line: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) todo: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) unconfirmed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) checked: Option<bool>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) panes: Vec<Done>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) written: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) turn_over: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) projection: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) group: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct TodoList {
    pub(crate) todos: Vec<TodoItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) undo: Option<u64>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", content = "entry", rename_all = "kebab-case")]
pub(crate) enum Queue {
    Add(QueueAdd),
    Edit { id: u64, add: QueueAdd },
    List,
    Remove(u64),
    Run(u64),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct QueueAdd {
    pub(crate) at: String,
    pub(crate) zone: String,
    pub(crate) agent: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) project: Option<u64>,
    pub(crate) workspace: Option<u64>,
    pub(crate) worktree: Option<String>,
    pub(crate) prompt: Option<String>,
    pub(crate) issue: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) submit: Option<bool>,
}
