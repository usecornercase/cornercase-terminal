use std::collections::HashSet;
use std::io::Read as _;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

use rustix::fs::OFlags;
use rustix::process::getgid;
use serde::{Deserialize, Serialize};

use crate::version;

pub(crate) const FILE: &str = "cornercase-plugin.toml";
pub(crate) const API: u32 = 1;
const MAX_ID: usize = 64;
const MAX_BYTES: u64 = 64 * 1024;
const MAX_ENTRIES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Manifest {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) min_cornercase: Option<String>,
    #[serde(default = "first_api")]
    pub(crate) api: u32,
    #[serde(default)]
    pub(crate) description: String,
    #[serde(default)]
    pub(crate) platforms: Vec<String>,
    #[serde(default)]
    pub(crate) permissions: Vec<String>,
    #[serde(default)]
    pub(crate) actions: Vec<Action>,
    #[serde(default)]
    pub(crate) events: Vec<Hook>,
    #[serde(default)]
    pub(crate) service: Vec<Process>,
    #[serde(default)]
    pub(crate) window: Vec<Process>,
}

fn first_api() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Action {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) contexts: Vec<Context>,
    pub(crate) command: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Context {
    Project,
    Workspace,
    Tab,
    Pane,
}

impl Context {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Workspace => "workspace",
            Self::Tab => "tab",
            Self::Pane => "pane",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Hook {
    pub(crate) on: On,
    pub(crate) command: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum On {
    Opened,
    Closed,
    Status,
    Exited,
}

impl On {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Opened => "opened",
            Self::Closed => "closed",
            Self::Status => "status",
            Self::Exited => "exited",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Process {
    pub(crate) id: String,
    pub(crate) command: Vec<String>,
}

pub(crate) fn platform() -> &'static str {
    if cfg!(target_os = "macos") { "macos" } else { "linux" }
}

impl Manifest {
    pub(crate) fn load(root: &Path) -> Result<Self, String> {
        let path = root.join(FILE);
        let cannot = |e: &dyn std::fmt::Display| format!("cannot read {}: {e}", path.display());
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(OFlags::NONBLOCK.bits().cast_signed())
            .open(&path)
            .map_err(|e| cannot(&e))?;
        let meta = file.metadata().map_err(|e| cannot(&e))?;
        if !meta.is_file() {
            return Err(cannot(&"it is not a file"));
        }
        let folder = std::fs::metadata(root).map_err(|e| cannot(&e))?;
        for (what, meta) in [("the plugin's folder", &folder), (FILE, &meta)] {
            if meta.uid() != crate::protocol::own_uid() {
                return Err(format!("{what} belongs to another user; a plugin runs as you"));
            }
            let shared_group = meta.mode() & 0o020 != 0 && meta.gid() != getgid().as_raw();
            if meta.mode() & 0o002 != 0 || shared_group {
                return Err(format!("other users can write to {what}; a plugin runs as you"));
            }
        }
        let mut text = String::new();
        file.take(MAX_BYTES + 1).read_to_string(&mut text).map_err(|e| cannot(&e))?;
        if text.len() as u64 > MAX_BYTES {
            return Err(cannot(&format!("it is larger than {} KiB", MAX_BYTES / 1024)));
        }
        Self::parse(&text)
    }

    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let manifest: Self = toml::from_str(text).map_err(|e| format!("{FILE}: {}", e.message()))?;
        manifest.check()?;
        Ok(manifest)
    }

    fn check(&self) -> Result<(), String> {
        valid_id("id", &self.id)?;
        for (field, value) in [("name", &self.name), ("version", &self.version)] {
            if value.trim().is_empty() {
                return Err(format!("{FILE}: `{field}` is empty"));
            }
        }
        if let Some(min) = &self.min_cornercase
            && version::newer(min, version::CURRENT)
        {
            return Err(format!("needs cornercase {min} or later; this is {}", version::CURRENT));
        }
        if self.api > API {
            return Err(format!("needs plugin API {}; this cornercase has {API}", self.api));
        }
        if !self.platforms.is_empty() && !self.platforms.iter().any(|p| p.eq_ignore_ascii_case(platform())) {
            return Err(format!("runs on {} only", self.platforms.join(", ")));
        }
        let entries = self.actions.len() + self.events.len() + self.service.len() + self.window.len();
        if entries > MAX_ENTRIES {
            return Err(format!("{FILE}: {entries} entries, more than the {MAX_ENTRIES} a plugin may declare"));
        }
        let mut seen = HashSet::new();
        for action in &self.actions {
            valid_id("action id", &action.id)?;
            if !seen.insert(("action", action.id.as_str())) {
                return Err(format!("{FILE}: two actions are called `{}`", action.id));
            }
            if action.title.trim().is_empty() || action.contexts.is_empty() {
                return Err(format!("{FILE}: action `{}` needs a title and at least one context", action.id));
            }
            valid_command(&format!("action `{}`", action.id), &action.command)?;
        }
        for hook in &self.events {
            valid_command(&format!("the hook on `{}`", hook.on.name()), &hook.command)?;
        }
        for (kind, processes) in [("service", &self.service), ("window", &self.window)] {
            for process in processes {
                valid_id(&format!("{kind} id"), &process.id)?;
                if !seen.insert((kind, process.id.as_str())) {
                    return Err(format!("{FILE}: two {kind} entries are called `{}`", process.id));
                }
                valid_command(&format!("{kind} `{}`", process.id), &process.command)?;
            }
        }
        Ok(())
    }

    pub(crate) fn runs(&self) -> Vec<String> {
        let count = |n: usize, one: &str, many: &str| match n {
            0 => None,
            1 => Some(format!("1 {one}")),
            n => Some(format!("{n} {many}")),
        };
        [
            count(self.service.len(), "service", "services"),
            count(self.window.len(), "window process", "window processes"),
            count(self.actions.len(), "action", "actions"),
            count(self.events.len(), "event hook", "event hooks"),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    pub(crate) fn asks(&self) -> String {
        Self::asking(&self.permissions)
    }

    pub(crate) fn asking(permissions: &[String]) -> String {
        if permissions.is_empty() {
            "declares no permissions".into()
        } else {
            format!("asks for {}", permissions.join(", "))
        }
    }
}

pub(crate) fn is_id(id: &str) -> bool {
    let mut chars = id.chars();
    let first = chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let rest = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    first && rest && id.len() <= MAX_ID
}

fn valid_id(field: &str, id: &str) -> Result<(), String> {
    if is_id(id) {
        return Ok(());
    }
    Err(format!(
        "{FILE}: {field} `{id}` must be lower-case letters, digits, `.`, `_` or `-`, at most {MAX_ID} characters"
    ))
}

fn valid_command(what: &str, command: &[String]) -> Result<(), String> {
    let Some(program) = command.first().filter(|program| !program.trim().is_empty()) else {
        return Err(format!("{FILE}: {what} has no command"));
    };
    if command.iter().any(|arg| arg.contains('\0')) {
        return Err(format!("{FILE}: {what} has a NUL character in its command"));
    }
    let path = Path::new(program);
    if path.is_relative() && path.components().any(|part| part == Component::ParentDir) {
        return Err(format!("{FILE}: {what} runs `{program}`, outside the plugin's folder"));
    }
    Ok(())
}

pub(crate) fn program(root: &Path, command: &[String]) -> Result<(PathBuf, Vec<String>), String> {
    let (first, args) = command.split_first().ok_or("no command")?;
    let path = Path::new(first);
    if !(path.is_relative() && first.contains('/')) {
        return Ok((path.to_path_buf(), args.to_vec()));
    }
    let outside = || format!("`{first}` is outside the plugin's folder");
    let root = root.canonicalize().map_err(|e| format!("cannot open the plugin's folder: {e}"))?;
    let program = root.join(path).canonicalize().map_err(|e| format!("cannot run `{first}`: {e}"))?;
    if !program.starts_with(&root) {
        return Err(outside());
    }
    Ok((program, args.to_vec()))
}

#[cfg(test)]
mod tests;
