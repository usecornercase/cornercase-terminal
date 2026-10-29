use std::collections::BTreeMap;

use serde_json::Value;
use toml::{Table, Value as Toml};

use crate::config::Config;
use crate::text;

pub(crate) const VERSION: u64 = 1;
pub(crate) const MAX_BYTES: usize = 256 * 1024;
pub(crate) const HEADER: &str =
    "# A cornercase setup: import it with `cornercase config import <file>` or Settings → share";
pub(crate) const NEVER_SHARED: [&str; 4] =
    ["tokens, bots, paired chats and browsers", "jira_email", "accounts and profiles", "remote access and keep awake"];
#[cfg(test)]
pub(crate) const LEFT_OUT: [&str; 15] = [
    "jira_email",
    "discord_forum",
    "discord_forum_close",
    "remote",
    "remote_port",
    "keep_awake",
    "projects",
    "profiles",
    "profile",
    "account_names",
    "active_accounts",
    "hidden_flows",
    "default_flow",
    "flow_order",
    "machines",
];

const AGENTS: [&str; 9] = [
    "agent",
    "agent_args",
    "agent_modes",
    "agent_commands",
    "prompt",
    "submit",
    "accept_trust_prompts",
    "resume_agents",
    "trust_prompt_pattern",
];
const ISSUES: [&str; 10] = [
    "issue_branch",
    "issue_tabs",
    "gitlab_url",
    "jira_site",
    "jira_jql",
    "plane_url",
    "plane_workspace",
    "plane_filter",
    "asana_workspace",
    "asana_projects",
];
const WORKTREES: [&str; 7] =
    ["worktrees_dir", "fetch_minutes", "pull_requests", "merged_pull_requests", "shared_files", "gh", "glab"];
const INTERFACE: [&str; 24] = [
    "sidebar",
    "tabs",
    "icon_bar",
    "icons",
    "agents_section",
    "working_icon",
    "counts",
    "todo_dot",
    "dim_inactive_panes",
    "prefix_key",
    "model",
    "context",
    "memory",
    "account_mark",
    "account_switcher",
    "account_picker",
    "desktop_notifications",
    "sound",
    "sound_waiting",
    "sound_done",
    "sound_subagents",
    "check_updates",
    "star_prompt",
    "tour",
];
const REPOSITORIES: &str = "repositories";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    Agents,
    Issues,
    Worktrees,
    Interface,
    Repositories,
}

impl Section {
    pub(crate) const ALL: [Self; 5] =
        [Self::Agents, Self::Issues, Self::Worktrees, Self::Interface, Self::Repositories];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Agents => "agents",
            Self::Issues => "issues",
            Self::Worktrees => "worktrees",
            Self::Interface => "interface",
            Self::Repositories => REPOSITORIES,
        }
    }

    pub(crate) fn keys(self) -> &'static [&'static str] {
        match self {
            Self::Agents => &AGENTS,
            Self::Issues => &ISSUES,
            Self::Worktrees => &WORKTREES,
            Self::Interface => &INTERFACE,
            Self::Repositories => &[],
        }
    }

    pub(crate) fn of(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|section| section.id() == id)
    }
}

pub(crate) fn section_of(key: &str) -> Option<Section> {
    Section::ALL.into_iter().find(|section| section.keys().contains(&key))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Repository {
    pub(crate) remotes: Vec<String>,
    pub(crate) setup: String,
    pub(crate) teardown: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Shared {
    pub(crate) settings: BTreeMap<String, Value>,
    pub(crate) repositories: Vec<Repository>,
    pub(crate) connect: Vec<String>,
    pub(crate) unknown: Vec<String>,
}

pub(crate) fn counts(settings: usize, repositories: usize) -> (String, String) {
    (text::plural(settings, "setting", "settings"), text::plural(repositories, "repository", "repositories"))
}

pub(crate) fn too_large() -> String {
    format!("too large to share (at most {} KiB)", MAX_BYTES / 1024)
}

pub(crate) fn settings_of(config: &Config) -> BTreeMap<String, Value> {
    let Ok(Value::Object(all)) = serde_json::to_value(config) else { return BTreeMap::new() };
    all.into_iter().filter(|(key, _)| section_of(key).is_some()).collect()
}

pub(crate) fn to_text(shared: &Shared) -> Result<String, String> {
    let mut document = Table::new();
    document.insert("version".into(), Toml::Integer(VERSION.cast_signed()));
    if !shared.connect.is_empty() {
        document.insert("connect".into(), strings(&shared.connect));
    }
    for section in Section::ALL.into_iter().filter(|section| *section != Section::Repositories) {
        document.insert(section.id().into(), Toml::Table(section_table(section, &shared.settings)?));
    }
    if !shared.repositories.is_empty() {
        document.insert(REPOSITORIES.into(), Toml::Array(shared.repositories.iter().map(repository_table).collect()));
    }
    let text = format!("{HEADER}\n{}", toml::to_string_pretty(&document).map_err(|e| e.to_string())?);
    if text.len() > MAX_BYTES { Err(too_large()) } else { Ok(text) }
}

fn section_table(section: Section, settings: &BTreeMap<String, Value>) -> Result<Table, String> {
    let mut table = Table::new();
    for key in section.keys() {
        if let Some(value) = settings.get(*key) {
            table.insert((*key).into(), Toml::try_from(value).map_err(|e| format!("{key}: {e}"))?);
        }
    }
    Ok(table)
}

fn strings(list: &[String]) -> Toml {
    Toml::Array(list.iter().cloned().map(Toml::String).collect())
}

fn repository_table(repository: &Repository) -> Toml {
    let mut table = Table::new();
    table.insert("remotes".into(), strings(&repository.remotes));
    table.insert("setup".into(), Toml::String(repository.setup.clone()));
    table.insert("teardown".into(), Toml::String(repository.teardown.clone()));
    Toml::Table(table)
}

pub(crate) fn parse(text: &str) -> Result<Shared, String> {
    if text.len() > MAX_BYTES {
        return Err(too_large());
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n");
    let document: Table = toml::from_str(&text).map_err(|e| match e.span() {
        Some(span) => format!("line {}: {}", text[..span.start.min(text.len())].matches('\n').count() + 1, e.message()),
        None => e.message().to_string(),
    })?;
    check_version(document.get("version"))?;
    let mut shared = Shared::default();
    for (key, value) in document {
        read_top(&mut shared, &key, value)?;
    }
    Ok(shared)
}

fn check_version(version: Option<&Toml>) -> Result<(), String> {
    match version {
        None => Err("this is not a cornercase setup: it has no version".into()),
        Some(Toml::Integer(version)) if *version > VERSION.cast_signed() => {
            Err(format!("made by a newer cornercase (setup version {version}); update cornercase to import it"))
        }
        Some(Toml::Integer(version)) if *version >= 1 => Ok(()),
        Some(_) => Err(format!("version must be a whole number from 1 to {VERSION}")),
    }
}

fn read_top(shared: &mut Shared, key: &str, value: Toml) -> Result<(), String> {
    match (key, Section::of(key)) {
        ("version", _) => {}
        ("connect", _) => shared.connect = string_list(value).ok_or("connect must be a list of tracker names")?,
        (_, Some(Section::Repositories)) => read_repositories(shared, value)?,
        (_, Some(section)) => read_section(shared, section, value)?,
        (_, None) => shared.unknown.push(key.to_string()),
    }
    Ok(())
}

fn string_list(value: Toml) -> Option<Vec<String>> {
    let Toml::Array(items) = value else { return None };
    items
        .into_iter()
        .map(|item| match item {
            Toml::String(text) => Some(text),
            _ => None,
        })
        .collect()
}

fn read_section(shared: &mut Shared, section: Section, value: Toml) -> Result<(), String> {
    let Toml::Table(table) = value else { return Err(format!("{0} must be a table, [{0}]", section.id())) };
    for (key, value) in table {
        if section_of(&key) == Some(section) {
            let value = serde_json::to_value(&value).map_err(|e| format!("{key}: {e}"))?;
            shared.settings.insert(key, value);
        } else {
            shared.unknown.push(format!("{}.{key}", section.id()));
        }
    }
    Ok(())
}

fn read_repositories(shared: &mut Shared, value: Toml) -> Result<(), String> {
    let refused = || format!("{REPOSITORIES} must be a list of tables, each [[{REPOSITORIES}]]");
    let Toml::Array(items) = value else { return Err(refused()) };
    for item in items {
        let Toml::Table(table) = item else { return Err(refused()) };
        let repository = read_repository(table, &mut shared.unknown)?;
        shared.repositories.push(repository);
    }
    Ok(())
}

fn read_repository(table: Table, unknown: &mut Vec<String>) -> Result<Repository, String> {
    let mut repository = Repository::default();
    let mut remotes = None;
    for (key, value) in table {
        match key.as_str() {
            "remotes" => remotes = string_list(value),
            "setup" => repository.setup = text_of(value, "setup")?,
            "teardown" => repository.teardown = text_of(value, "teardown")?,
            _ => note_unknown(unknown, format!("{REPOSITORIES}.{key}")),
        }
    }
    repository.remotes =
        remotes.ok_or("remotes of a repository must be a list of strings, such as \"github.com/acme/web-shop\"")?;
    Ok(repository)
}

fn text_of(value: Toml, key: &str) -> Result<String, String> {
    match value {
        Toml::String(text) => Ok(text),
        _ => Err(format!("{key} of a repository must be a string")),
    }
}

fn note_unknown(unknown: &mut Vec<String>, name: String) {
    if !unknown.contains(&name) {
        unknown.push(name);
    }
}

pub(crate) mod dialog;
pub(crate) mod preview;
pub(crate) mod secrets;

#[cfg(test)]
mod tests;
