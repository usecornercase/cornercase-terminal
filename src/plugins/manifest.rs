use std::collections::{BTreeMap, HashSet};
use std::fs::Metadata;
use std::io::Read as _;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

use rustix::fs::OFlags;
use rustix::process::getgid;
use serde::{Deserialize, Serialize};

use crate::{text, version};

pub(crate) const FILE: &str = "cornercase-plugin.toml";
pub(crate) const API: u32 = 1;
const MAX_ID: usize = 64;
const MAX_BYTES: u64 = 64 * 1024;
const MAX_ENTRIES: usize = 64;
const MAX_QUOTED: usize = 80;

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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) download: BTreeMap<String, Download>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) build: Vec<Build>,
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Download {
    pub(crate) url: String,
    pub(crate) sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Build {
    pub(crate) command: Vec<String>,
}

pub(crate) fn platform() -> &'static str {
    if cfg!(target_os = "macos") { "macos" } else { "linux" }
}

impl Manifest {
    pub(crate) fn load(root: &Path) -> Result<Self, String> {
        let manifest = Self::read(root)?;
        manifest.programs(root)?;
        Ok(manifest)
    }

    pub(crate) fn read(root: &Path) -> Result<Self, String> {
        Self::open(root).map_err(|e| text::escape_hidden(&e))
    }

    fn open(root: &Path) -> Result<Self, String> {
        let path = root.join(FILE);
        let cannot = |e: &dyn std::fmt::Display| format!("cannot read {}: {e}", path.display());
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags((OFlags::NONBLOCK | OFlags::NOFOLLOW).bits().cast_signed())
            .open(&path)
            .map_err(|e| cannot(&e))?;
        let meta = file.metadata().map_err(|e| cannot(&e))?;
        if !meta.is_file() {
            return Err(cannot(&"it is not a file"));
        }
        let folder = std::fs::metadata(root).map_err(|e| cannot(&e))?;
        guarded("the plugin's folder", &folder)?;
        guarded(FILE, &meta)?;
        let mut text = String::new();
        file.take(MAX_BYTES + 1).read_to_string(&mut text).map_err(|e| cannot(&e))?;
        if text.len() as u64 > MAX_BYTES {
            return Err(cannot(&format!("it is larger than {} KiB", MAX_BYTES / 1024)));
        }
        Self::parse(&text)
    }

    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let shown = |e: String| text::escape_hidden(&e);
        let manifest: Self = toml::from_str(text).map_err(|e| shown(format!("{FILE}: {}", e.message())))?;
        manifest.check().map_err(shown)?;
        Ok(manifest)
    }

    fn check(&self) -> Result<(), String> {
        self.check_texts()?;
        self.check_requirements()?;
        self.check_entries()?;
        for build in &self.build {
            valid_command("a build step", &build.command)?;
        }
        for (target, download) in &self.download {
            valid_download(target, download)?;
        }
        Ok(())
    }

    fn check_texts(&self) -> Result<(), String> {
        if let Some((field, value)) = self.texts().find(|(_, value)| value.chars().any(text::hidden)) {
            return Err(format!("{FILE}: {field} `{}` has an invisible or control character", around_hidden(value)));
        }
        valid_id("id", &self.id)?;
        if serde_json::to_value(self).is_ok_and(|value| has_control(&value)) {
            return Err(format!("{FILE}: a text in it holds a control character, such as an escape or a new line"));
        }
        for (field, value) in [("name", &self.name), ("version", &self.version)] {
            if value.trim().is_empty() {
                return Err(format!("{FILE}: `{field}` is empty"));
            }
        }
        Ok(())
    }

    fn check_requirements(&self) -> Result<(), String> {
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
        Ok(())
    }

    fn check_entries(&self) -> Result<(), String> {
        let entries = self.actions.len() + self.events.len() + self.service.len() + self.window.len();
        if entries > MAX_ENTRIES {
            return Err(format!("{FILE}: {entries} entries, more than the {MAX_ENTRIES} a plugin may declare"));
        }
        let mut seen = HashSet::new();
        for action in &self.actions {
            check_action(action, &mut seen)?;
        }
        for hook in &self.events {
            valid_command(&format!("the hook on `{}`", hook.on.name()), &hook.command)?;
        }
        for (kind, processes) in [("service", &self.service), ("window", &self.window)] {
            for process in processes {
                check_process(kind, process, &mut seen)?;
            }
        }
        Ok(())
    }

    fn texts(&self) -> impl Iterator<Item = (&'static str, &str)> {
        let fields =
            [("id", &self.id), ("name", &self.name), ("version", &self.version), ("description", &self.description)];
        let lists = [("platforms", &self.platforms), ("permissions", &self.permissions)];
        fields
            .into_iter()
            .map(|(field, value)| (field, value.as_str()))
            .chain(self.min_cornercase.iter().map(|value| ("min_cornercase", value.as_str())))
            .chain(lists.into_iter().flat_map(|(field, values)| values.iter().map(move |v| (field, v.as_str()))))
            .chain(self.actions.iter().flat_map(|a| {
                [("an action id", a.id.as_str()), ("an action title", a.title.as_str())]
                    .into_iter()
                    .chain(a.command.iter().map(|arg| ("a command", arg.as_str())))
            }))
            .chain(self.events.iter().flat_map(|h| h.command.iter().map(|arg| ("a command", arg.as_str()))))
            .chain(self.service.iter().chain(&self.window).flat_map(|p| {
                std::iter::once(("a process id", p.id.as_str()))
                    .chain(p.command.iter().map(|arg| ("a command", arg.as_str())))
            }))
            .chain(self.build.iter().flat_map(|b| b.command.iter().map(|arg| ("a build command", arg.as_str()))))
            .chain(self.download.iter().flat_map(|(target, d)| {
                [
                    ("a download target", target.as_str()),
                    ("a download url", d.url.as_str()),
                    ("a download sha256", d.sha256.as_str()),
                ]
            }))
    }

    pub(crate) fn commands(&self) -> Vec<(String, &[String])> {
        let mut commands: Vec<(String, &[String])> = Vec::new();
        for process in &self.service {
            commands.push((format!("service {}", process.id), &process.command));
        }
        for process in &self.window {
            commands.push((format!("window process {}", process.id), &process.command));
        }
        for action in &self.actions {
            let contexts: Vec<&str> = action.contexts.iter().map(|c| c.name()).collect();
            commands.push((format!("action {} ({})", action.id, contexts.join(", ")), &action.command));
        }
        for hook in &self.events {
            commands.push((format!("on {}", hook.on.name()), &hook.command));
        }
        commands
    }

    fn programs(&self, root: &Path) -> Result<(), String> {
        for (what, command) in self.commands() {
            program(root, command).map_err(|e| format!("{what}: {e}"))?;
        }
        Ok(())
    }

    pub(crate) fn download_here(&self) -> Option<&Download> {
        self.download.get(crate::update::target()?)
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
        if self.permissions.is_empty() {
            "declares no permissions".into()
        } else {
            format!("asks for {}", self.permissions.join(", "))
        }
    }
}

fn around_hidden(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let at = chars.iter().position(|&c| text::hidden(c)).unwrap_or(0);
    let start = at.saturating_sub(MAX_QUOTED / 2);
    let end = (start + MAX_QUOTED).min(chars.len());
    let cut = |cut: bool| if cut { "…" } else { "" };
    format!("{}{}{}", cut(start > 0), chars[start..end].iter().collect::<String>(), cut(end < chars.len()))
}

fn check_action<'a>(action: &'a Action, seen: &mut HashSet<(&'a str, &'a str)>) -> Result<(), String> {
    valid_id("action id", &action.id)?;
    if !seen.insert(("action", action.id.as_str())) {
        return Err(format!("{FILE}: two actions are called `{}`", action.id));
    }
    if action.title.trim().is_empty() || action.contexts.is_empty() {
        return Err(format!("{FILE}: action `{}` needs a title and at least one context", action.id));
    }
    valid_command(&format!("action `{}`", action.id), &action.command)
}

fn check_process<'a>(
    kind: &'a str,
    process: &'a Process,
    seen: &mut HashSet<(&'a str, &'a str)>,
) -> Result<(), String> {
    valid_id(&format!("{kind} id"), &process.id)?;
    if !seen.insert((kind, process.id.as_str())) {
        return Err(format!("{FILE}: two {kind} entries are called `{}`", process.id));
    }
    valid_command(&format!("{kind} `{}`", process.id), &process.command)
}

fn valid_id(field: &str, id: &str) -> Result<(), String> {
    let mut chars = id.chars();
    let first = chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let rest = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    if first && rest && id.len() <= MAX_ID {
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
    let path = Path::new(program);
    if path.is_relative() && path.components().any(|part| part == Component::ParentDir) {
        return Err(format!("{FILE}: {what} runs `{program}`, outside the plugin's folder"));
    }
    Ok(())
}

fn has_control(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(text) => text.chars().any(char::is_control),
        serde_json::Value::Array(items) => items.iter().any(has_control),
        serde_json::Value::Object(fields) => {
            fields.iter().any(|(key, value)| key.chars().any(char::is_control) || has_control(value))
        }
        _ => false,
    }
}

fn valid_download(target: &str, download: &Download) -> Result<(), String> {
    let sha = &download.sha256;
    if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("{FILE}: the download for {target} needs a sha256 of 64 hexadecimal digits"));
    }
    let url = download.url.as_str();
    let loopback = ["http://127.0.0.1:", "http://localhost:"].iter().any(|start| url.starts_with(start));
    if !(url.starts_with("https://") || loopback) || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!("{FILE}: the download for {target} must be an https:// address"));
    }
    Ok(())
}

pub(crate) fn program(root: &Path, command: &[String]) -> Result<(PathBuf, Vec<String>), String> {
    resolve(root, command).map_err(|e| text::escape_hidden(&e))
}

fn resolve(root: &Path, command: &[String]) -> Result<(PathBuf, Vec<String>), String> {
    let (first, args) = command.split_first().ok_or("no command")?;
    let path = Path::new(first);
    if !(path.is_relative() && first.contains('/')) {
        return Ok((path.to_path_buf(), args.to_vec()));
    }
    let outside = || format!("`{first}` is outside the plugin's folder");
    let root = root.canonicalize().map_err(|e| format!("cannot open the plugin's folder: {e}"))?;
    let program = root.join(path).canonicalize().map_err(|e| format!("cannot run `{first}`: {e}"))?;
    let Ok(inside) = program.strip_prefix(&root) else {
        return Err(outside());
    };
    guarded("the plugin's folder", &metadata(&root)?)?;
    let mut part = PathBuf::new();
    for component in inside.components() {
        part.push(component);
        guarded(&format!("`{}`", part.display()), &metadata(&root.join(&part))?)?;
    }
    Ok((program, args.to_vec()))
}

fn metadata(path: &Path) -> Result<Metadata, String> {
    std::fs::metadata(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

fn guarded(what: &str, meta: &Metadata) -> Result<(), String> {
    if meta.uid() != crate::protocol::own_uid() {
        return Err(format!("{what} belongs to another user; a plugin runs as you"));
    }
    let shared_group = meta.mode() & 0o020 != 0 && meta.gid() != getgid().as_raw();
    if meta.mode() & 0o002 != 0 || shared_group {
        return Err(format!("other users can write to {what}; a plugin runs as you"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
