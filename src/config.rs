use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::agents;
use crate::host::notify;
use crate::persist;
use crate::shortcuts::Prefix;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Sidebar {
    SideBySide,
    #[default]
    ProjectsOnTop,
    WorkspacesOnTop,
    Tree,
}

impl Sidebar {
    pub(crate) const ALL: [Self; 4] = [Self::SideBySide, Self::ProjectsOnTop, Self::WorkspacesOnTop, Self::Tree];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::SideBySide => "side_by_side",
            Self::ProjectsOnTop => "projects_on_top",
            Self::WorkspacesOnTop => "workspaces_on_top",
            Self::Tree => "tree",
        }
    }

    fn note(self) -> &'static str {
        match self {
            Self::SideBySide => "projects and workspaces in two columns",
            Self::ProjectsOnTop => "one column, workspaces below projects",
            Self::WorkspacesOnTop => "one column, projects below workspaces",
            Self::Tree => "one list: projects, workspaces and tabs",
        }
    }

    pub(crate) fn choices() -> Vec<(&'static str, &'static str)> {
        Self::ALL.into_iter().map(|s| (s.id(), s.note())).collect()
    }

    pub(crate) fn from_setting(setting: &str) -> Self {
        from_setting(&Self::ALL, Self::id, setting)
    }

    pub(crate) fn stacked(self) -> bool {
        self != Self::SideBySide
    }
}

fn from_setting<T: Copy + Default>(all: &[T], id: fn(T) -> &'static str, setting: &str) -> T {
    all.iter().copied().find(|s| id(*s).eq_ignore_ascii_case(setting.trim())).unwrap_or_default()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Tabs {
    #[default]
    Sidebar,
    Top,
}

impl Tabs {
    pub(crate) const ALL: [Self; 2] = [Self::Sidebar, Self::Top];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Sidebar => "sidebar",
            Self::Top => "top",
        }
    }

    pub(crate) fn choices() -> Vec<(&'static str, &'static str)> {
        let note = |t: Self| match t {
            Self::Sidebar => "under their workspace in the sidebar",
            Self::Top => "in a bar above the pane, only the active workspace's",
        };
        Self::ALL.into_iter().map(|t| (t.id(), note(t))).collect()
    }

    pub(crate) fn from_setting(setting: &str) -> Self {
        from_setting(&Self::ALL, Self::id, setting)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum WorkingIcon {
    #[default]
    Moon,
    Spinner,
}

impl WorkingIcon {
    const ALL: [Self; 2] = [Self::Moon, Self::Spinner];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Moon => "moon",
            Self::Spinner => "spinner",
        }
    }

    pub(crate) fn choices() -> Vec<(&'static str, &'static str)> {
        vec![(Self::Moon.id(), "◐, still while the agent works"), (Self::Spinner.id(), "⠋, turning ten times a second")]
    }

    pub(crate) fn from_setting(setting: &str) -> Self {
        from_setting(&Self::ALL, Self::id, setting)
    }
}

pub(crate) const DEFAULT_WORKTREES_DIR: &str = "~/.cornercase/worktrees";
pub(crate) const DEFAULT_PORT: u16 = 47681;
pub(crate) const DEFAULT_PROMPT: &str = "{url}";
pub(crate) const DEFAULT_GH: &str = "gh";
pub(crate) const DEFAULT_ISSUE_TABS: [&str; 6] = ["all", "github", "shortcut", "linear", "jira", "plane"];
pub(crate) const DEFAULT_FETCH_MINUTES: u64 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_excessive_bools, reason = "each switch is its own key in config.json")]
pub(crate) struct Config {
    pub(crate) worktrees_dir: String,
    pub(crate) fetch_minutes: u64,
    pub(crate) issue_tabs: Vec<String>,
    pub(crate) jira_site: String,
    pub(crate) jira_email: String,
    pub(crate) jira_jql: String,
    pub(crate) plane_url: String,
    pub(crate) plane_workspace: String,
    pub(crate) plane_filter: String,
    pub(crate) agent: String,
    pub(crate) agent_args: BTreeMap<String, Vec<String>>,
    pub(crate) agent_modes: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    pub(crate) agent_commands: BTreeMap<String, String>,
    pub(crate) prompt: String,
    pub(crate) submit: bool,
    pub(crate) accept_trust_prompts: bool,
    pub(crate) resume_agents: bool,
    pub(crate) keep_awake: bool,
    pub(crate) trust_prompt_pattern: String,
    pub(crate) gh: String,
    pub(crate) sidebar: String,
    pub(crate) tabs: String,
    pub(crate) agents_section: bool,
    pub(crate) working_icon: String,
    pub(crate) counts: bool,
    pub(crate) todo_dot: bool,
    pub(crate) dim_inactive_panes: bool,
    pub(crate) prefix_key: String,
    pub(crate) model: bool,
    pub(crate) context: bool,
    pub(crate) memory: bool,
    pub(crate) desktop_notifications: String,
    pub(crate) check_updates: bool,
    pub(crate) remote: bool,
    pub(crate) remote_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            worktrees_dir: DEFAULT_WORKTREES_DIR.into(),
            fetch_minutes: DEFAULT_FETCH_MINUTES,
            issue_tabs: DEFAULT_ISSUE_TABS.map(String::from).to_vec(),
            jira_site: String::new(),
            jira_email: String::new(),
            jira_jql: String::new(),
            plane_url: String::new(),
            plane_workspace: String::new(),
            plane_filter: String::new(),
            agent: agents::AUTO.into(),
            agent_args: BTreeMap::new(),
            agent_modes: BTreeMap::new(),
            agent_commands: BTreeMap::new(),
            prompt: DEFAULT_PROMPT.into(),
            submit: false,
            accept_trust_prompts: false,
            resume_agents: true,
            keep_awake: false,
            trust_prompt_pattern: agents::DEFAULT_TRUST_PROMPT.into(),
            gh: DEFAULT_GH.into(),
            sidebar: Sidebar::default().id().into(),
            tabs: Tabs::default().id().into(),
            agents_section: false,
            working_icon: WorkingIcon::default().id().into(),
            counts: true,
            todo_dot: true,
            dim_inactive_panes: true,
            prefix_key: String::new(),
            model: true,
            context: true,
            memory: false,
            desktop_notifications: notify::AUTO.into(),
            check_updates: true,
            remote: false,
            remote_port: DEFAULT_PORT,
        }
    }
}

impl Config {
    pub(crate) fn worktrees_dir(&self, home: Option<&Path>) -> PathBuf {
        expand_home(&self.worktrees_dir, home)
    }

    pub(crate) fn gh(&self, home: Option<&Path>) -> PathBuf {
        expand_home(&self.gh, home)
    }

    pub(crate) fn prefix(&self) -> Option<Prefix> {
        Prefix::parse(&self.prefix_key)
    }

    pub(crate) fn spinner(&self) -> bool {
        WorkingIcon::from_setting(&self.working_icon) == WorkingIcon::Spinner
    }

    pub(crate) fn fetch_every(&self) -> Option<Duration> {
        (self.fetch_minutes > 0).then(|| Duration::from_secs(self.fetch_minutes.saturating_mul(60)))
    }
}

pub(crate) fn load(path: &Path) -> Config {
    let Some(mut value) = std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()) else {
        return Config::default();
    };
    if let Value::Object(keys) = &mut value {
        migrate(keys);
    }
    serde_json::from_value(value).unwrap_or_default()
}

fn migrate(keys: &mut Map<String, Value>) {
    if let Some(old) = keys.remove("context_line") {
        keys.entry("context").or_insert(old);
    }
    if let Some(context) = keys.get("context").cloned() {
        keys.entry("model").or_insert(context);
    }
    if !keys.contains_key("jira_site")
        && let Some(Value::Array(tabs)) = keys.get_mut("issue_tabs")
        && !tabs.iter().any(|t| t.as_str().is_some_and(|t| t.trim().eq_ignore_ascii_case("jira")))
    {
        tabs.push(Value::String("jira".into()));
    }
    if !keys.contains_key("plane_workspace")
        && let Some(Value::Array(tabs)) = keys.get_mut("issue_tabs")
        && !tabs.iter().any(|t| t.as_str().is_some_and(|t| t.trim().eq_ignore_ascii_case("plane")))
    {
        tabs.push(Value::String("plane".into()));
    }
}

pub(crate) fn save(path: &Path, config: &Config) -> io::Result<()> {
    persist::save(path, config)
}

pub(crate) fn expand_home(path: &str, home: Option<&Path>) -> PathBuf {
    match (path.strip_prefix('~'), home) {
        (Some(""), Some(home)) => home.to_path_buf(),
        (Some(rest), Some(home)) if rest.starts_with('/') => home.join(rest.trim_start_matches('/')),
        _ => PathBuf::from(path),
    }
}

pub(crate) fn check_worktrees_dir(input: &str, home: Option<&Path>) -> Result<String, &'static str> {
    let dir = input.trim();
    if dir.is_empty() {
        return Err("the folder is required");
    }
    if !expand_home(dir, home).is_absolute() {
        return Err("use an absolute path or one starting with ~/");
    }
    Ok(dir.to_string())
}

pub(crate) fn check_fetch_minutes(input: &str) -> Result<u64, &'static str> {
    input.trim().parse().map_err(|_| "use a whole number of minutes, 0 turns it off")
}

#[cfg(test)]
mod tests;
