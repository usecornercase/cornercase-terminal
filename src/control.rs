use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PANE_ENV: &str = "CORNERCASE_PANE";
pub const SERVER_ENV: &str = "CORNERCASE_SERVER";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    #[serde(default)]
    pub caller: Option<u64>,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(flatten)]
    pub command: Command,
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

pub fn server_token() -> &'static str {
    static TOKEN: OnceLock<String> = OnceLock::new();
    TOKEN.get_or_init(|| {
        let started = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_nanos());
        format!("{}-{started}", std::process::id())
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", content = "args", rename_all = "kebab-case")]
pub enum Command {
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
}

impl Command {
    pub fn name(&self) -> &'static str {
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
        }]
    }

    pub fn fields(&self) -> Vec<(&'static str, String)> {
        let mut fields = Fields::default();
        match self {
            Self::Status(_) => {}
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
                fields.maybe("timeout", send.timeout);
            }
            Self::Keys(keys) => {
                fields.maybe("pane", keys.pane).maybe("tab", keys.tab).add("keys", keys.keys.len());
            }
            Self::Read(read) => {
                fields.maybe("pane", read.pane).maybe("tab", read.tab).maybe("lines", read.lines);
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
        }
        fields.0
    }

    pub const NAMES: [&str; 16] = [
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
pub enum Item {
    Group(u64),
    Project(u64),
    Workspace(u64),
    Tab(u64),
    Pane(u64),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Status {}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Open {
    pub path: PathBuf,
    pub focus: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewWorkspace {
    pub name: String,
    pub project: Option<u64>,
    pub worktree: bool,
    pub focus: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NewTab {
    pub workspace: Option<u64>,
    pub name: Option<String>,
    pub command: Option<String>,
    pub focus: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Split {
    pub pane: Option<u64>,
    pub down: bool,
    pub command: Option<String>,
    pub focus: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Start {
    pub agent: Option<String>,
    pub worktree: Option<String>,
    pub workspace: Option<u64>,
    pub name: Option<String>,
    pub prompt: Option<String>,
    pub wait: bool,
    pub until: Until,
    pub timeout: Option<f64>,
    pub focus: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SendText {
    pub pane: Option<u64>,
    pub tab: Option<u64>,
    pub text: Option<String>,
    pub enter: bool,
    pub wait: bool,
    pub until: Until,
    pub timeout: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Keys {
    pub pane: Option<u64>,
    pub tab: Option<u64>,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Read {
    pub pane: Option<u64>,
    pub tab: Option<u64>,
    pub lines: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Wait {
    pub pane: Option<u64>,
    pub tab: Option<u64>,
    pub panes: Vec<u64>,
    pub tabs: Vec<u64>,
    pub all: bool,
    pub until: Until,
    pub timeout: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Until {
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
    pub fn name(&self) -> String {
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
pub struct Close {
    pub item: Item,
    #[serde(default)]
    pub remove_worktree: bool,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rename {
    pub item: Option<Item>,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Focus {
    pub item: Item,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notify {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Todo {
    Add(String),
    List,
    Done(u64),
    Rm(u64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Response {
    Ok(Value),
    Error(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ids {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Done {
    #[serde(flatten)]
    pub ids: Ids,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub todo: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub panes: Vec<Done>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Report {
    pub version: String,
    pub caller: Option<u64>,
    pub shown: Ids,
    pub groups: Vec<GroupInfo>,
    pub projects: Vec<ProjectInfo>,
}

impl Report {
    pub fn workspace_on(&self, branch: &str) -> Result<u64, String> {
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
pub struct GroupInfo {
    pub id: u64,
    pub name: String,
    pub collapsed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectInfo {
    pub id: u64,
    pub name: String,
    pub path: PathBuf,
    pub group: Option<u64>,
    pub active: bool,
    pub workspaces: Vec<WorkspaceInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceInfo {
    pub id: u64,
    pub name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub worktree: bool,
    pub behind: u32,
    pub active: bool,
    pub tabs: Vec<TabInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TabInfo {
    pub id: u64,
    pub name: String,
    pub status: Option<String>,
    pub active: bool,
    pub panes: Vec<PaneInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_excessive_bools, reason = "each flag is its own key in the JSON of `status`")]
pub struct PaneInfo {
    pub id: u64,
    pub path: Option<PathBuf>,
    pub program: Option<String>,
    pub agent: Option<String>,
    pub status: Option<String>,
    pub background_shell: bool,
    pub at_prompt: Option<bool>,
    pub model: Option<String>,
    pub context: Option<u16>,
    pub active: bool,
    pub caller: bool,
    pub resumes: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TodoList {
    pub todos: Vec<TodoItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TodoItem {
    pub id: u64,
    pub text: String,
    pub done: bool,
}

pub fn command_name(request: &Value) -> Option<&str> {
    request.get("command").and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn read(pane: u64) -> Command {
        Command::Read(Read { pane: Some(pane), ..Read::default() })
    }

    #[test]
    fn a_request_names_its_command_next_to_its_arguments() {
        let request = Request { caller: Some(7), server: Some("1-2".into()), command: read(3) };

        let sent = serde_json::to_value(&request).expect("json");

        assert_eq!(
            sent,
            json!({"caller": 7, "server": "1-2", "command": "read", "args": {"pane": 3, "tab": null, "lines": null}})
        );
    }

    #[test]
    fn fields_from_a_newer_command_are_ignored() {
        let text = r#"{"caller": 7, "command": "read", "args": {"pane": 3, "colour": "red"}, "sent_at": 1}"#;

        let request: Request = serde_json::from_str(text).expect("a request");

        assert_eq!(request, Request { caller: Some(7), server: None, command: read(3) });
    }

    #[test]
    fn fields_an_older_command_lacks_take_their_defaults() {
        let request: Request = serde_json::from_str(r#"{"command": "new-tab", "args": {}}"#).expect("a request");

        assert_eq!(request.command, Command::NewTab(NewTab::default()));
    }

    #[test]
    fn every_command_is_listed_by_name() {
        let commands = [
            Command::Status(Status::default()),
            Command::Open(Open::default()),
            Command::NewWorkspace(NewWorkspace::default()),
            Command::NewTab(NewTab::default()),
            Command::Split(Split::default()),
            Command::Start(Start::default()),
            Command::Send(SendText::default()),
            Command::Keys(Keys::default()),
            read(1),
            Command::Wait(Wait::default()),
            Command::Close(Close { item: Item::Tab(1), remove_worktree: false, force: false }),
            Command::Rename(Rename::default()),
            Command::Focus(Focus { item: Item::Pane(1) }),
            Command::Notify(Notify::default()),
            Command::Todo(Todo::List),
            Command::WaitSeveral(Wait::default()),
        ];

        let names: Vec<String> = commands
            .iter()
            .map(|command| {
                let request = Request { caller: None, server: None, command: command.clone() };
                let sent = serde_json::to_value(request).expect("json");
                command_name(&sent).expect("a name").to_string()
            })
            .collect();

        assert_eq!(names, Command::NAMES);
        assert_eq!(commands.iter().map(Command::name).collect::<Vec<_>>(), Command::NAMES);
    }

    #[test]
    fn the_fields_to_log_leave_out_what_is_typed() {
        let secret = "hunter2-marker";
        let commands = [
            Command::Send(SendText { text: Some(secret.into()), enter: true, ..SendText::default() }),
            Command::Keys(Keys { keys: vec![secret.into()], ..Keys::default() }),
            Command::Start(Start { prompt: Some(secret.into()), ..Start::default() }),
            Command::NewTab(NewTab { command: Some(secret.into()), ..NewTab::default() }),
            Command::Split(Split { command: Some(secret.into()), ..Split::default() }),
            Command::Wait(Wait { until: Until::Text(secret.into()), ..Wait::default() }),
            Command::WaitSeveral(Wait { until: Until::Text(secret.into()), panes: vec![3, 4], ..Wait::default() }),
            Command::Notify(Notify { text: secret.into() }),
            Command::Todo(Todo::Add(secret.into())),
        ];

        for command in commands {
            let fields = command.fields();
            assert!(fields.iter().all(|(_, value)| !value.contains(secret)), "{fields:?}");
        }
    }

    mod workspace_on {
        use super::*;

        fn workspace(id: u64, name: &str, branch: Option<&str>, worktree: bool) -> WorkspaceInfo {
            WorkspaceInfo {
                id,
                name: name.into(),
                branch: branch.map(str::to_string),
                worktree,
                ..WorkspaceInfo::default()
            }
        }

        fn project(id: u64, name: &str, workspaces: Vec<WorkspaceInfo>) -> ProjectInfo {
            ProjectInfo { id, name: name.into(), workspaces, ..ProjectInfo::default() }
        }

        fn shop() -> ProjectInfo {
            project(
                1,
                "shop",
                vec![
                    workspace(2, "default", Some("main"), false),
                    workspace(3, "fix/login", Some("fix/login"), true),
                    workspace(4, "experiments", Some("main"), false),
                ],
            )
        }

        fn api(caller: bool) -> ProjectInfo {
            let mut api = project(5, "api", vec![workspace(6, "fix/login", Some("fix/login"), true)]);
            let pane = PaneInfo { id: 8, caller, ..PaneInfo::default() };
            api.workspaces[0].tabs = vec![TabInfo { id: 7, panes: vec![pane], ..TabInfo::default() }];
            api
        }

        fn shown(project: u64, projects: Vec<ProjectInfo>) -> Report {
            Report { shown: Ids { project: Some(project), ..Ids::default() }, projects, ..Report::default() }
        }

        #[test]
        fn finds_the_workspace_on_the_branch_in_the_shown_project() {
            let report = shown(1, vec![api(false), shop()]);

            assert_eq!(report.workspace_on("fix/login"), Ok(3));
        }

        #[test]
        fn looks_in_the_project_of_the_pane_that_asks_before_the_shown_one() {
            let report = shown(1, vec![shop(), api(true)]);

            assert_eq!(report.workspace_on("fix/login"), Ok(6));
        }

        #[test]
        fn a_missing_branch_lists_the_workspaces_of_the_project() {
            let report = shown(1, vec![shop(), api(false)]);

            assert_eq!(
                report.workspace_on("feat/x"),
                Err("no workspace of project 1 (shop) is on branch feat/x; its workspaces:\n\
                     \x20 workspace 2  default  branch main\n\
                     \x20 workspace 3  fix/login  branch fix/login  worktree\n\
                     \x20 workspace 4  experiments  branch main"
                    .into())
            );
        }

        #[test]
        fn a_branch_in_several_workspaces_lists_them() {
            let report = shown(1, vec![shop()]);

            assert_eq!(
                report.workspace_on("main"),
                Err("branch main is checked out in 2 workspaces of project 1 (shop); pick one with --workspace:\n\
                     \x20 workspace 2  default  branch main\n\
                     \x20 workspace 4  experiments  branch main"
                    .into())
            );
        }

        #[rstest::rstest]
        #[case::no_project(shown(1, vec![]), "no project is open")]
        #[case::no_workspace(shown(9, vec![project(9, "empty", vec![])]), "project 9 (empty) has no workspace")]
        fn says_why_nothing_is_found(#[case] report: Report, #[case] expected: &str) {
            let error = report.workspace_on("main").expect_err("nothing found");

            assert!(error.starts_with(expected), "{error}");
        }
    }

    #[test]
    fn turn_over_is_spelt_as_on_the_command_line() {
        let wait = Command::Wait(Wait { until: Until::TurnOver, ..Wait::default() });

        let sent = serde_json::to_value(Request { caller: None, server: None, command: wait }).expect("json");

        assert_eq!(sent["args"]["until"], json!("turn-over"));
    }

    #[test]
    fn an_answer_carries_only_the_ids_it_has() {
        let done = Done { ids: Ids { pane: Some(4), ..Ids::default() }, ended: Some("idle".into()), ..Done::default() };

        assert_eq!(serde_json::to_value(done).expect("json"), json!({"pane": 4, "ended": "idle"}));
    }
}
