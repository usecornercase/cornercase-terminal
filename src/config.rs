use std::collections::BTreeMap;
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
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
pub(crate) const DEFAULT_GLAB: &str = "glab";
pub(crate) const DEFAULT_ISSUE_TABS: [&str; 8] =
    ["all", "github", "gitlab", "shortcut", "linear", "jira", "plane", "asana"];
pub(crate) const DEFAULT_FETCH_MINUTES: u64 = 5;
pub(crate) const SYSTEM_ACCOUNT: &str = "System";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ProjectSettings {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) setup: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) teardown: String,
}

impl ProjectSettings {
    pub(crate) fn normalised(self) -> Self {
        let clean = |text: String| {
            let text = text.replace("\r\n", "\n");
            let text: String = text.chars().map(|c| if c.is_control() && c != '\n' { ' ' } else { c }).collect();
            text.trim_end().to_string()
        };
        Self { setup: clean(self.setup), teardown: clean(self.teardown) }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.setup.is_empty() && self.teardown.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[expect(clippy::struct_excessive_bools, reason = "each switch is its own key in config.json")]
pub(crate) struct Config {
    pub(crate) worktrees_dir: String,
    pub(crate) fetch_minutes: u64,
    pub(crate) pull_requests: bool,
    pub(crate) merged_pull_requests: bool,
    pub(crate) issue_branch: String,
    pub(crate) issue_tabs: Vec<String>,
    pub(crate) gitlab_url: String,
    pub(crate) jira_site: String,
    pub(crate) jira_email: String,
    pub(crate) jira_jql: String,
    pub(crate) plane_url: String,
    pub(crate) plane_workspace: String,
    pub(crate) plane_filter: String,
    pub(crate) asana_workspace: String,
    pub(crate) asana_projects: String,
    pub(crate) agent: String,
    pub(crate) agent_args: BTreeMap<String, Vec<String>>,
    pub(crate) agent_modes: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    pub(crate) agent_commands: BTreeMap<String, String>,
    pub(crate) prompt: String,
    pub(crate) submit: bool,
    pub(crate) accept_trust_prompts: bool,
    pub(crate) resume_agents: bool,
    pub(crate) keep_awake: bool,
    pub(crate) hidden_flows: Vec<String>,
    pub(crate) trust_prompt_pattern: String,
    pub(crate) gh: String,
    pub(crate) glab: String,
    pub(crate) sidebar: String,
    pub(crate) tabs: String,
    pub(crate) icon_bar: bool,
    pub(crate) agents_section: bool,
    pub(crate) working_icon: String,
    pub(crate) counts: bool,
    pub(crate) todo_dot: bool,
    pub(crate) dim_inactive_panes: bool,
    pub(crate) prefix_key: String,
    pub(crate) model: bool,
    pub(crate) context: bool,
    pub(crate) memory: bool,
    pub(crate) account_mark: bool,
    pub(crate) account_switcher: bool,
    pub(crate) account_picker: bool,
    pub(crate) desktop_notifications: String,
    pub(crate) sound: bool,
    pub(crate) sound_waiting: String,
    pub(crate) sound_done: String,
    pub(crate) sound_subagents: bool,
    pub(crate) check_updates: bool,
    pub(crate) star_prompt: bool,
    pub(crate) tour: bool,
    pub(crate) remote: bool,
    pub(crate) remote_port: u16,
    pub(crate) projects: BTreeMap<String, ProjectSettings>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) profiles: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub(crate) profile: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) account_names: BTreeMap<String, BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) active_accounts: BTreeMap<String, String>,
}

impl Config {
    const BLANK: Self = Self {
        worktrees_dir: String::new(),
        fetch_minutes: 0,
        pull_requests: false,
        merged_pull_requests: false,
        issue_branch: String::new(),
        issue_tabs: Vec::new(),
        gitlab_url: String::new(),
        jira_site: String::new(),
        jira_email: String::new(),
        jira_jql: String::new(),
        plane_url: String::new(),
        plane_workspace: String::new(),
        plane_filter: String::new(),
        asana_workspace: String::new(),
        asana_projects: String::new(),
        agent: String::new(),
        agent_args: BTreeMap::new(),
        agent_modes: BTreeMap::new(),
        agent_commands: BTreeMap::new(),
        prompt: String::new(),
        submit: false,
        accept_trust_prompts: false,
        resume_agents: false,
        keep_awake: false,
        hidden_flows: Vec::new(),
        trust_prompt_pattern: String::new(),
        gh: String::new(),
        glab: String::new(),
        sidebar: String::new(),
        tabs: String::new(),
        icon_bar: false,
        agents_section: false,
        working_icon: String::new(),
        counts: false,
        todo_dot: false,
        dim_inactive_panes: false,
        prefix_key: String::new(),
        model: false,
        context: false,
        memory: false,
        account_mark: false,
        account_switcher: false,
        account_picker: false,
        desktop_notifications: String::new(),
        sound: false,
        sound_waiting: String::new(),
        sound_done: String::new(),
        sound_subagents: false,
        check_updates: false,
        star_prompt: false,
        tour: false,
        remote: false,
        remote_port: 0,
        projects: BTreeMap::new(),
        profiles: BTreeMap::new(),
        profile: String::new(),
        account_names: BTreeMap::new(),
        active_accounts: BTreeMap::new(),
    };
}

impl Default for Config {
    fn default() -> Self {
        Self {
            worktrees_dir: DEFAULT_WORKTREES_DIR.into(),
            fetch_minutes: DEFAULT_FETCH_MINUTES,
            pull_requests: true,
            merged_pull_requests: true,
            issue_branch: String::new(),
            issue_tabs: DEFAULT_ISSUE_TABS.map(String::from).to_vec(),
            agent: agents::AUTO.into(),
            prompt: DEFAULT_PROMPT.into(),
            resume_agents: true,
            trust_prompt_pattern: agents::DEFAULT_TRUST_PROMPT.into(),
            gh: DEFAULT_GH.into(),
            glab: DEFAULT_GLAB.into(),
            sidebar: Sidebar::default().id().into(),
            tabs: Tabs::default().id().into(),
            working_icon: WorkingIcon::default().id().into(),
            counts: true,
            todo_dot: true,
            dim_inactive_panes: true,
            model: true,
            context: true,
            account_mark: true,
            account_switcher: true,
            account_picker: true,
            desktop_notifications: notify::AUTO.into(),
            sound: true,
            check_updates: true,
            star_prompt: true,
            tour: true,
            remote_port: DEFAULT_PORT,
            ..Self::BLANK
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

    pub(crate) fn glab(&self, home: Option<&Path>) -> PathBuf {
        expand_home(&self.glab, home)
    }

    pub(crate) fn prefix(&self) -> Option<Prefix> {
        Prefix::parse(&self.prefix_key)
    }

    pub(crate) fn spinner(&self) -> bool {
        WorkingIcon::from_setting(&self.working_icon) == WorkingIcon::Spinner
    }

    pub(crate) fn project(&self, folder: &Path, home: Option<&Path>) -> ProjectSettings {
        self.projects
            .iter()
            .find(|(key, _)| names_folder(key, folder, home))
            .map(|(_, s)| s.clone())
            .unwrap_or_default()
    }

    pub(crate) fn set_project(&mut self, folder: &Path, home: Option<&Path>, settings: ProjectSettings) {
        self.projects.retain(|key, _| !names_folder(key, folder, home));
        let settings = settings.normalised();
        if !settings.is_empty() {
            self.projects.insert(crate::text::display_path(folder, home), settings);
        }
    }

    pub(crate) fn active_profile(&self) -> Option<(&str, &BTreeMap<String, String>)> {
        self.profiles.get_key_value(&self.profile).map(|(name, accounts)| (name.as_str(), accounts))
    }

    pub(crate) fn selected_accounts(&self) -> &BTreeMap<String, String> {
        self.active_profile().map_or(&self.active_accounts, |(_, chosen)| chosen)
    }

    pub(crate) fn account_label<'a>(&'a self, kind: &str, name: &'a str) -> &'a str {
        match self.account_names.get(kind).and_then(|labels| labels.get(name)) {
            Some(label) => label,
            None if name.is_empty() => SYSTEM_ACCOUNT,
            None => name,
        }
    }

    pub(crate) fn account_taken(&self, kind: &str, name: &str) -> String {
        match self.account_label(kind, name) {
            label if label == name => format!("the {kind} account `{name}` already exists"),
            label => format!("the {kind} account `{name}` already exists (shown as `{label}`)"),
        }
    }

    pub(crate) fn label_taken(&self, kind: &str, label: &str, accounts: &[String], except: &str) -> Option<String> {
        std::iter::once("")
            .chain(accounts.iter().map(String::as_str))
            .filter(|other| *other != except)
            .map(|other| self.account_label(kind, other).to_string())
            .find(|taken| taken.eq_ignore_ascii_case(label))
            .map(|taken| format!("another {kind} account is already called `{taken}`"))
    }

    pub(crate) fn forget_account(&mut self, kind: &str, name: &str) {
        for chosen in self.profiles.values_mut() {
            if chosen.get(kind).is_some_and(|used| used == name) {
                chosen.remove(kind);
            }
        }
        if self.active_accounts.get(kind).is_some_and(|used| used == name) {
            self.active_accounts.remove(kind);
        }
        if let Some(labels) = self.account_names.get_mut(kind) {
            labels.remove(name);
            if labels.is_empty() {
                self.account_names.remove(kind);
            }
        }
    }

    pub(crate) fn unknown_profile(&self) -> Option<&str> {
        (!self.profile.is_empty() && !self.profiles.contains_key(&self.profile)).then_some(self.profile.as_str())
    }

    pub(crate) fn fetch_every(&self) -> Option<Duration> {
        (self.fetch_minutes > 0).then(|| Duration::from_secs(self.fetch_minutes.saturating_mul(60)))
    }
}

#[cfg(test)]
pub(crate) fn load(path: &Path) -> Config {
    read(path).ok().flatten().unwrap_or_default()
}

const MAX_BYTES: u64 = 4 << 20;

pub(crate) fn read(path: &Path) -> Result<Option<Config>, String> {
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits().cast_signed())
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("it is not a regular file".into());
    }
    let mut text = String::new();
    file.take(MAX_BYTES + 1).read_to_string(&mut text).map_err(|e| e.to_string())?;
    if text.len() as u64 > MAX_BYTES {
        return Err("it is too large".into());
    }
    let mut value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if let Value::Object(keys) = &mut value {
        migrate(keys);
    }
    serde_json::from_value(value).map(Some).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Changes(Vec<(String, Option<Value>)>);

impl Changes {
    pub(crate) fn between(old: &Config, new: &Config) -> Self {
        let (Value::Object(old), Value::Object(new)) =
            (serde_json::to_value(old).unwrap_or_default(), serde_json::to_value(new).unwrap_or_default())
        else {
            return Self(Vec::new());
        };
        let gone: Vec<(String, Option<Value>)> =
            old.keys().filter(|key| !new.contains_key(*key)).map(|key| (key.clone(), None)).collect();
        let mut changed: Vec<(String, Option<Value>)> = new
            .into_iter()
            .filter(|(key, value)| old.get(key) != Some(value))
            .map(|(key, value)| (key, Some(value)))
            .collect();
        changed.extend(gone);
        changed.sort_by(|a, b| a.0.cmp(&b.0));
        Self(changed)
    }

    pub(crate) fn keys(&self) -> Vec<&str> {
        self.0.iter().map(|(key, _)| key.as_str()).collect()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn apply(&self, config: &Config) -> Config {
        let Ok(Value::Object(mut keys)) = serde_json::to_value(config) else { return config.clone() };
        for (key, value) in &self.0 {
            match value {
                Some(value) => keys.insert(key.clone(), value.clone()),
                None => keys.remove(key),
            };
        }
        serde_json::from_value(Value::Object(keys)).unwrap_or_else(|e| {
            crate::log::warning!("config", "a change could not be applied", why = e.to_string());
            config.clone()
        })
    }
}

fn migrate(keys: &mut Map<String, Value>) {
    if let Some(old) = keys.remove("context_line") {
        keys.entry("context").or_insert(old);
    }
    if let Some(context) = keys.get("context").cloned() {
        keys.entry("model").or_insert(context);
    }
    add_tab(keys, "jira_site", "jira", None);
    add_tab(keys, "plane_workspace", "plane", None);
    add_tab(keys, "asana_workspace", "asana", None);
    add_tab(keys, "gitlab_url", "gitlab", Some("github"));
}

fn add_tab(keys: &mut Map<String, Value>, written_since: &str, tab: &str, after: Option<&str>) {
    if keys.contains_key(written_since) {
        return;
    }
    let Some(Value::Array(tabs)) = keys.get_mut("issue_tabs") else { return };
    let named = |value: &Value, name: &str| value.as_str().is_some_and(|t| t.trim().eq_ignore_ascii_case(name));
    if tabs.iter().any(|t| named(t, tab)) {
        return;
    }
    let at = after.and_then(|after| tabs.iter().position(|t| named(t, after))).map_or(tabs.len(), |i| i + 1);
    tabs.insert(at, Value::String(tab.into()));
}

pub(crate) fn save(path: &Path, config: &Config) -> io::Result<persist::Mark> {
    persist::write_marked(path, config, false)
}

pub(crate) fn names_folder(key: &str, folder: &Path, home: Option<&Path>) -> bool {
    let key = expand_home(key, home);
    key == folder || key.canonicalize().is_ok_and(|key| key == folder)
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

pub(crate) fn check_sound_file(input: &str) -> Result<String, &'static str> {
    if input.chars().any(crate::text::hidden) {
        return Err("a file's path cannot hold control or invisible characters");
    }
    let path = input.trim();
    if !(path.is_empty() || path.starts_with('/') || path == "~" || path.starts_with("~/")) {
        return Err("use an absolute path or one starting with ~/");
    }
    Ok(input.to_string())
}

#[cfg(test)]
mod tests;
