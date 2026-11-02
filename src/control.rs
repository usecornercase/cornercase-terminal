use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) mod accounts;
mod app_state;
pub(crate) mod arrange;
pub(crate) mod changes;
pub(crate) mod clone;
pub(crate) mod closed_agents;
pub(crate) mod config;
pub(crate) mod contract;
mod events;
mod fields;
pub(crate) mod files;
mod flow;
pub(crate) mod issues;
pub(crate) mod machines;
pub(crate) mod plugins;
pub(crate) mod ports;
pub(crate) mod processes;
mod search;
pub(crate) mod start_server;
mod status;
pub(crate) mod surface;
pub(crate) mod terminal;
pub(crate) mod theme;
mod todo;
pub(crate) mod tour;
pub(crate) mod usage;
pub(crate) mod view;
mod watch;
pub(crate) mod worktrees;

use fields::{Fields, chosen};
pub(crate) use todo::{Todo, TodoSend};

pub(crate) use accounts::{
    AccountAdd, AccountList, AccountRemove, AccountRename, AccountRestart, AccountUse, AccountsInfo, ProfileDelete,
    ProfileList, ProfileNew, ProfileSet, ProfileUse, RestartInfo, SkippedPane,
};
pub(crate) use app_state::{
    AppState, AppWatch, AwayInfo, ForumInfo, InstallInfo, NoUpdate, Notes, PopupInfo, ReleaseInfo, UpdateCheck,
    UpdateInfo, UpdateInstall,
};
pub(crate) use arrange::{CloseAgent, Fold, GroupDelete, GroupNew, GroupStyle, Move, MovePane, RemovalCheck};
pub(crate) use changes::{
    Changes, ChangesBase, ChangesCount, ChangesDiscard, ChangesFile, ChangesGap, ChangesMergeCheck, ChangesMode,
    ChangesStage, ChangesViewed,
};
pub(crate) use clone::{CloneRepository, Cloned};
pub(crate) use closed_agents::{AgentsClose, AgentsForget, AgentsReopen};
pub(crate) use config::{ConfigExport, ConfigImport, ConfigInfo, Configure};
pub(crate) use contract::Contract;
pub(crate) use events::{Event, Events, Go, Kind, What};
pub(crate) use files::{FilesList, FilesRead, FilesSearch};
#[cfg(test)]
pub(crate) use flow::{BackSpec, RoleSpec, StageSpec};
pub(crate) use flow::{
    BrokenFlow, Exported, FlowBacklog, FlowDefault, FlowDefaultSet, FlowDelete, FlowExport, FlowFiles, FlowHide,
    FlowImport, FlowInfo, FlowList, FlowMessage, FlowMessages, FlowOrder, FlowOrdered, FlowRemoved, FlowRestore,
    FlowRun, FlowRuns, FlowRunsWatch, FlowSave, FlowSaved, FlowSpec, Flows, Imported, MemberInfo, RoleInfo, RoleState,
    RunEdge, RunFile, RunFiles, RunInfo, RunState, StepInfo,
};
pub(crate) use issues::{IssueList, IssuePeople, IssueRead, IssueSources};
pub(crate) use machines::{FocusMachine, MachineList, MachineSeen, MachineSet, Machines, ShownRemote, StreamState};
pub(crate) use plugins::{PluginActions, PluginList, PluginPopupClose, PluginRun};
pub(crate) use ports::{Listing, PortStopped, Ports, StopPort};
pub(crate) use processes::{ProcessStopped, Processes, StopProcess};
pub(crate) use search::{Hit, Matches, Reveal, Search};
pub(crate) use start_server::StartServer;
pub(crate) use status::{
    GroupInfo, KeepAwake, LayerInfo, PostInfo, PullRequestInfo, RemoteInfo, RunStatus, SharedInfo, StackInfo,
};
pub use status::{PaneInfo, PaneRow, PaneRows, ProjectInfo, Report, TabInfo, WorkspaceInfo};
pub(crate) use surface::Surface;
pub(crate) use terminal::{Input, Link, PasteFile, Ratio, Redraw, Resize, Scroll, Selection};
pub(crate) use theme::{Theme, ThemeInfo};
pub(crate) use usage::{Plans, Stats, Usage};
pub(crate) use view::{OpenView, SetView, ViewInfo};
pub(crate) use watch::{Layout, Snapshot, Watch};
pub(crate) use worktrees::{
    BuildsFreed, FreeBuilds, ListWorktrees, WorktreeInfo, WorktreeListing, WorktreeSize, WorktreeSized,
};

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
    Changes(Changes),
    ChangesMode(ChangesMode),
    ChangesFile(ChangesFile),
    ChangesGap(ChangesGap),
    ChangesBase(ChangesBase),
    ChangesViewed(ChangesViewed),
    ChangesDiscard(ChangesDiscard),
    ChangesStage(ChangesStage),
    Usage(Usage),
    PluginList(PluginList),
    PluginActions(PluginActions),
    PluginRun(PluginRun),
    PluginPopupClose(PluginPopupClose),
    FlowSave(FlowSave),
    FlowDelete(FlowDelete),
    FlowHide(FlowHide),
    FlowRestore(FlowRestore),
    FlowDefault(FlowDefault),
    FlowOrder(FlowOrder),
    FlowRuns(FlowRunsWatch),
    FlowMessages(FlowMessages),
    FlowFiles(FlowFiles),
    AccountList(AccountList),
    AccountAdd(AccountAdd),
    AccountRemove(AccountRemove),
    ProfileList(ProfileList),
    ProfileNew(ProfileNew),
    ProfileSet(ProfileSet),
    ProfileUse(ProfileUse),
    ProfileDelete(ProfileDelete),
    AccountRename(AccountRename),
    AccountUse(AccountUse),
    CloseAgent(CloseAgent),
    AccountRestart(AccountRestart),
    AgentsClose(AgentsClose),
    AgentsReopen(AgentsReopen),
    AgentsForget(AgentsForget),
    Tour(tour::TourRequest),
    ConfigExport(ConfigExport),
    ConfigImport(ConfigImport),
    Ports(Ports),
    StopPort(StopPort),
    FilesList(FilesList),
    FilesRead(FilesRead),
    FilesSearch(FilesSearch),
    PasteFile(PasteFile),
    ChangesMergeCheck(ChangesMergeCheck),
    Processes(Processes),
    StopProcess(StopProcess),
    WorktreeSize(WorktreeSize),
    FreeBuilds(FreeBuilds),
    Worktrees(ListWorktrees),
    Machines(Machines),
    Clone(CloneRepository),
    FocusMachine(FocusMachine),
}

impl Command {
    pub(crate) fn name(&self) -> &'static str {
        let at = self
            .pane_name()
            .or_else(|| self.row_name())
            .or_else(|| self.stream_name())
            .or_else(|| self.terminal_name())
            .or_else(|| self.change_name())
            .or_else(|| self.files_name())
            .or_else(|| self.issue_name())
            .or_else(|| self.flow_name())
            .or_else(|| self.account_name())
            .or_else(|| self.server_name());
        Self::NAMES[at.unwrap_or_default()]
    }

    fn pane_name(&self) -> Option<usize> {
        Some(match self {
            Self::Open(_) => 1,
            Self::NewWorkspace(_) => 2,
            Self::NewTab(_) => 3,
            Self::Split(_) => 4,
            Self::Start(_) => 5,
            Self::StartWith(_) => 23,
            Self::Send(_) => 6,
            Self::Keys(_) => 7,
            Self::Read(_) => 8,
            Self::LastMessage(_) => 17,
            Self::Wait(_) => 9,
            Self::WaitSeveral(_) => 15,
            Self::Mark(_) => 21,
            Self::Notify(_) => 13,
            Self::Focus(_) => 12,
            Self::FocusMachine(_) => 115,
            _ => return None,
        })
    }

    fn row_name(&self) -> Option<usize> {
        Some(match self {
            Self::Close(_) => 10,
            Self::Rename(_) => 11,
            Self::RemovalCheck(_) => 53,
            Self::CloseAgent(_) => 88,
            Self::AgentsClose(_) => 92,
            Self::AgentsReopen(_) => 93,
            Self::AgentsForget(_) => 94,
            Self::Move(_) => 46,
            Self::MovePane(_) => 47,
            Self::Collapse(_) => 48,
            Self::Expand(_) => 49,
            Self::GroupNew(_) => 50,
            Self::GroupStyle(_) => 51,
            Self::GroupDelete(_) => 52,
            Self::Todo(_) => 14,
            Self::TodoSend(_) => 41,
            _ => return None,
        })
    }

    fn stream_name(&self) -> Option<usize> {
        Some(match self {
            Self::Status(_) => 0,
            Self::Stats(_) => 19,
            Self::Contract(_) => 29,
            Self::Watch(_) => 31,
            Self::App(_) => 60,
            Self::Events(_) => 16,
            Self::FlowRuns(_) => 75,
            Self::Surface(_) => 32,
            Self::View(_) => 33,
            Self::ViewSet(_) => 34,
            Self::Search(_) => 28,
            _ => return None,
        })
    }

    fn terminal_name(&self) -> Option<usize> {
        Some(match self {
            Self::Input(_) => 35,
            Self::Scroll(_) => 36,
            Self::Selection(_) => 37,
            Self::Resize(_) => 38,
            Self::Redraw(_) => 44,
            Self::Ratio(_) => 39,
            Self::Link(_) => 40,
            Self::PasteFile(_) => 106,
            _ => return None,
        })
    }

    fn change_name(&self) -> Option<usize> {
        Some(match self {
            Self::Changes(_) => 61,
            Self::ChangesMode(_) => 62,
            Self::ChangesFile(_) => 63,
            Self::ChangesGap(_) => 64,
            Self::ChangesBase(_) => 65,
            Self::ChangesViewed(_) => 66,
            Self::ChangesDiscard(_) => 90,
            Self::ChangesStage(_) => 91,
            Self::ChangesMergeCheck(_) => 107,
            _ => return None,
        })
    }

    fn files_name(&self) -> Option<usize> {
        Some(match self {
            Self::FilesList(_) => 103,
            Self::FilesRead(_) => 104,
            Self::FilesSearch(_) => 105,
            _ => return None,
        })
    }

    fn issue_name(&self) -> Option<usize> {
        Some(match self {
            Self::IssueSources(_) => 54,
            Self::IssueList(_) => 55,
            Self::IssueRead(_) => 56,
            Self::IssuePeople(_) => 57,
            Self::Queue(_) => 22,
            _ => return None,
        })
    }

    fn flow_name(&self) -> Option<usize> {
        Some(match self {
            Self::FlowList(_) => 24,
            Self::FlowRun(_) => 25,
            Self::FlowExport(_) => 26,
            Self::FlowImport(_) => 27,
            Self::FlowSave(_) => 71,
            Self::FlowDelete(_) => 72,
            Self::FlowHide(_) => 73,
            Self::FlowRestore(_) => 74,
            Self::FlowDefault(_) => 98,
            Self::FlowOrder(_) => 99,
            Self::FlowMessages(_) => 76,
            Self::FlowFiles(_) => 77,
            _ => return None,
        })
    }

    fn account_name(&self) -> Option<usize> {
        Some(match self {
            Self::AccountList(_) => 78,
            Self::AccountAdd(_) => 79,
            Self::AccountRemove(_) => 80,
            Self::AccountRename(_) => 86,
            Self::AccountUse(_) => 87,
            Self::ProfileList(_) => 81,
            Self::ProfileNew(_) => 82,
            Self::ProfileSet(_) => 83,
            Self::ProfileUse(_) => 84,
            Self::ProfileDelete(_) => 85,
            Self::AccountRestart(_) => 89,
            _ => return None,
        })
    }

    fn server_name(&self) -> Option<usize> {
        Some(match self {
            Self::Config(_) => 45,
            Self::Theme(_) => 42,
            Self::Go(_) => 43,
            Self::UpdateCheck(_) => 58,
            Self::UpdateInstall(_) => 59,
            Self::RestartWhenIdle(_) => 18,
            Self::StartServer(_) => 30,
            Self::Away(_) => 20,
            Self::Usage(_) => 67,
            Self::Ports(_) => 101,
            Self::StopPort(_) => 102,
            Self::Processes(_) => 110,
            Self::StopProcess(_) => 111,
            Self::WorktreeSize(_) => 108,
            Self::FreeBuilds(_) => 109,
            Self::Worktrees(_) => 112,
            Self::Machines(_) => 113,
            Self::Clone(_) => 114,
            Self::Tour(_) => 95,
            Self::PluginList(_) => 68,
            Self::PluginActions(_) => 69,
            Self::PluginRun(_) => 70,
            Self::PluginPopupClose(_) => 100,
            Self::ConfigExport(_) => 96,
            Self::ConfigImport(_) => 97,
            _ => return None,
        })
    }

    pub(crate) const NAMES: [&str; 116] = [
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
        "changes",
        "changes-mode",
        "changes-file",
        "changes-gap",
        "changes-base",
        "changes-viewed",
        "usage",
        "plugin-list",
        "plugin-actions",
        "plugin-run",
        "flow-save",
        "flow-delete",
        "flow-hide",
        "flow-restore",
        "flow-runs",
        "flow-messages",
        "flow-files",
        "account-list",
        "account-add",
        "account-remove",
        "profile-list",
        "profile-new",
        "profile-set",
        "profile-use",
        "profile-delete",
        "account-rename",
        "account-use",
        "close-agent",
        "account-restart",
        "changes-discard",
        "changes-stage",
        "agents-close",
        "agents-reopen",
        "agents-forget",
        "tour",
        "config-export",
        "config-import",
        "flow-default",
        "flow-order",
        "plugin-popup-close",
        "ports",
        "stop-port",
        "files-list",
        "files-read",
        "files-search",
        "paste-file",
        "changes-merge-check",
        "worktree-size",
        "free-builds",
        "processes",
        "stop-process",
        "worktrees",
        "machines",
        "clone",
        "focus-machine",
    ];

    pub(crate) fn chatty(name: &str) -> bool {
        matches!(
            name,
            "input"
                | "scroll"
                | "selection"
                | "resize"
                | "ratio"
                | "link"
                | "changes-file"
                | "changes-gap"
                | "flow-files"
                | "processes"
                | "files-list"
                | "files-read"
                | "files-search"
        )
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
    pub(crate) profile: Option<String>,
    pub(crate) accounts: BTreeMap<String, String>,
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
    pub(crate) workspace_name: Option<String>,
    pub(crate) wait: bool,
    pub(crate) until: Until,
    pub(crate) timeout: Option<f64>,
    pub(crate) focus: bool,
    pub(crate) profile: Option<String>,
    pub(crate) accounts: BTreeMap<String, String>,
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
    #[serde(default)]
    pub(crate) reveal: Option<Reveal>,
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

pub(crate) fn streamed_line(value: &impl Serialize) -> Option<String> {
    let value = serde_json::to_value(value).ok()?;
    serde_json::to_string(&Response::Ok(value)).ok()
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) revealed: Option<bool>,
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

#[cfg(test)]
pub(crate) mod tests;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", content = "entry", rename_all = "kebab-case")]
pub(crate) enum Queue {
    Add(QueueAdd),
    Edit { id: u64, add: QueueAdd },
    List,
    Remove(u64),
    Run(u64),
    ClearStarted,
    Restore(u64),
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
    pub(crate) accounts: BTreeMap<String, String>,
}
