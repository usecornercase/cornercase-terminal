use std::io::Read as _;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use flate2::read::GzDecoder;

use super::{EXTENSION, MAX_BYTES, MAX_IMPORT, is_persona, read_text, too_long};
use crate::http::{self, Service};

pub(crate) const RAW_URL: &str = "https://raw.githubusercontent.com";
pub(crate) const ARCHIVE_URL: &str = "https://codeload.github.com";
pub(crate) const RAW_ENV: &str = "CORNERCASE_PERSONAS_URL";
pub(crate) const ARCHIVE_ENV: &str = "CORNERCASE_PERSONAS_ARCHIVE_URL";
const DIVISIONS: &str = "divisions.json";
const MAX_ARCHIVE: u64 = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;
const MAX_KEPT: usize = 64 * 1024 * 1024;
const MAX_DEPTH: usize = 8;
const TIMEOUT: Duration = Duration::from_secs(20);
const ARCHIVE_TIMEOUT: Duration = Duration::from_secs(120);
const SERVICE: Service = Service { name: "the persona's host", rejected: "the persona's host refused the request" };

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Source {
    Stdin,
    File(PathBuf),
    Folder(PathBuf),
    Url(String),
    Repo { repo: String, path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) file: String,
    pub(crate) text: String,
    pub(crate) source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Gathered {
    pub(crate) found: Vec<Found>,
    pub(crate) skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hosts {
    pub(crate) raw: String,
    pub(crate) archive: String,
}

impl Hosts {
    pub(crate) fn from_env() -> Self {
        let read = |var: &str, default: &str| {
            std::env::var(var).unwrap_or_else(|_| default.to_string()).trim_end_matches('/').to_string()
        };
        Self { raw: read(RAW_ENV, RAW_URL), archive: read(ARCHIVE_ENV, ARCHIVE_URL) }
    }
}

impl Source {
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Stdin => "-".to_string(),
            Self::File(path) | Self::Folder(path) => path.display().to_string(),
            Self::Url(url) => url.clone(),
            Self::Repo { repo, path } if path.is_empty() => repo.clone(),
            Self::Repo { repo, path } => format!("{repo}/{path}"),
        }
    }

    pub(crate) fn page(&self) -> String {
        match self {
            Self::Repo { repo, path } if path.is_empty() => format!("https://github.com/{repo}"),
            Self::Repo { repo, path } => format!("https://github.com/{repo}/tree/HEAD/{path}"),
            other => other.label(),
        }
    }

    pub(crate) fn is_web(&self) -> bool {
        matches!(self, Self::Url(_) | Self::Repo { .. })
    }
}

fn part(text: &str) -> bool {
    !text.is_empty()
        && text != "."
        && text != ".."
        && !text.contains(['\\', ':'])
        && !text.chars().any(char::is_control)
}

fn named(text: &str) -> bool {
    part(text) && text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub(crate) fn repo_path(text: &str) -> Option<(String, String)> {
    let text = text.trim_end_matches('/');
    let mut parts = text.split('/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    let rest: Vec<&str> = parts.collect();
    (named(owner) && named(repo) && rest.iter().all(|p| part(p))).then(|| (format!("{owner}/{repo}"), rest.join("/")))
}

pub(crate) fn source(arg: &str, exists: impl Fn(&Path) -> Option<bool>) -> Result<Source, String> {
    if arg == "-" {
        return Ok(Source::Stdin);
    }
    if let Some(url) = crate::flows::registry::address(arg)? {
        return Ok(Source::Url(url));
    }
    let path = Path::new(arg);
    match exists(path) {
        Some(true) => return Ok(Source::Folder(path.to_path_buf())),
        Some(false) => return Ok(Source::File(path.to_path_buf())),
        None => {}
    }
    match repo_path(arg) {
        Some((repo, path)) => Ok(Source::Repo { repo, path }),
        None => Ok(Source::File(path.to_path_buf())),
    }
}

pub(crate) fn on_disk(path: &Path) -> Option<bool> {
    std::fs::metadata(path).ok().map(|meta| meta.is_dir())
}

fn not_persona(what: &str) -> String {
    format!("{what} is not a persona: a persona is a .md file whose front matter (a `---` block) gives its `name`")
}

pub(crate) fn single(file: &str, text: String, source: Option<String>, what: &str) -> Result<Gathered, String> {
    if text.len() > MAX_BYTES {
        return Err(too_long(what));
    }
    if !is_persona(&text) {
        return Err(not_persona(what));
    }
    Ok(Gathered { found: vec![Found { file: file.to_string(), text, source }], skipped: 0 })
}

pub(crate) fn gather(source: &Source, hosts: &Hosts) -> Result<Gathered, String> {
    match source {
        Source::Stdin => Err("standard input is read by the command".into()),
        Source::File(path) => {
            let text = read_text(path)?;
            let file = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
            single(file, text, Some(absolute(path)), &path.display().to_string())
        }
        Source::Folder(path) => picked(walk(path)?, &source.label()),
        Source::Url(url) => {
            let text = fetch_text(url)?;
            let file = url.split(['?', '#']).next().unwrap_or_default().rsplit('/').next().unwrap_or_default();
            single(file, text, Some(url.clone()), url)
        }
        Source::Repo { repo, path } if has_extension(path) => {
            let text = fetch_text(&format!("{}/{repo}/HEAD/{path}", hosts.raw))?;
            let file = path.rsplit('/').next().unwrap_or_default();
            single(file, text, Some(format!("{repo}/{path}")), &source.label())
        }
        Source::Repo { repo, path } => picked(archive(hosts, repo, path)?, &source.label()),
    }
}

fn has_extension(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|ext| ext.eq_ignore_ascii_case(EXTENSION))
}

fn absolute(path: &Path) -> String {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()).display().to_string()
}

fn fetch_text(url: &str) -> Result<String, String> {
    match http::download_strictly(&SERVICE, url, MAX_BYTES as u64 + 1, TIMEOUT) {
        Ok(bytes) if bytes.len() > MAX_BYTES => Err(too_long(url)),
        Ok(bytes) => String::from_utf8(bytes).map_err(|_| format!("{url} is not text")),
        Err(e) if e.to_string() == http::TOO_LARGE => Err(too_long(url)),
        Err(e) if e.to_string().contains("answered 404") => Err(format!("nothing at {url}")),
        Err(e) => Err(format!("cannot fetch {url}: {e}")),
    }
}

struct Entry {
    relative: String,
    text: String,
    source: String,
}

#[derive(Default)]
struct Tree {
    entries: Vec<Entry>,
    divisions: Option<String>,
    skipped: usize,
    kept: usize,
}

impl Tree {
    fn add(&mut self, relative: String, bytes: Vec<u8>, source: String) -> Result<(), String> {
        let Ok(text) = String::from_utf8(bytes) else {
            self.skipped += 1;
            return Ok(());
        };
        if relative == DIVISIONS {
            self.divisions = Some(text);
            return Ok(());
        }
        if !is_persona(&text) {
            self.skipped += 1;
            return Ok(());
        }
        self.kept += text.len();
        if self.kept > MAX_KEPT {
            return Err(format!("its personas take more than {} MiB", MAX_KEPT / 1024 / 1024));
        }
        self.entries.push(Entry { relative, text, source });
        Ok(())
    }

    fn wanted(relative: &str) -> bool {
        relative == DIVISIONS || has_extension(relative)
    }
}

fn walk(root: &Path) -> Result<Tree, String> {
    let mut tree = Tree::default();
    let mut pending = vec![(root.to_path_buf(), String::new(), 0usize)];
    let mut seen = 0;
    while let Some((folder, prefix, depth)) = pending.pop() {
        let entries = std::fs::read_dir(&folder).map_err(|e| format!("cannot read {}: {e}", folder.display()))?;
        for entry in entries.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                return Err(format!("{} holds more than {MAX_ENTRIES} files", root.display()));
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else { continue };
            let relative = format!("{prefix}{name}");
            if kind.is_dir() && !name.starts_with('.') && depth < MAX_DEPTH {
                pending.push((entry.path(), format!("{relative}/"), depth + 1));
            } else if kind.is_file() && Tree::wanted(&relative) {
                tree_file(&mut tree, &entry.path(), relative).map_err(|e| format!("{}: {e}", root.display()))?;
            }
        }
    }
    Ok(tree)
}

fn tree_file(tree: &mut Tree, path: &Path, relative: String) -> Result<(), String> {
    if let Ok(text) = read_text(path) {
        return tree.add(relative, text.into_bytes(), absolute(path));
    }
    tree.skipped += 1;
    Ok(())
}

fn archive(hosts: &Hosts, repo: &str, path: &str) -> Result<Tree, String> {
    let url = format!("{}/{repo}/tar.gz/HEAD", hosts.archive);
    let bytes = match http::download_strictly(&SERVICE, &url, MAX_ARCHIVE, ARCHIVE_TIMEOUT) {
        Ok(bytes) => bytes,
        Err(e) if e.to_string() == http::TOO_LARGE => {
            return Err(format!("{repo} is larger than {} MiB to download", MAX_ARCHIVE / 1024 / 1024));
        }
        Err(e) if e.to_string().contains("answered 404") => {
            return Err(format!("no repository {repo} on GitHub, or it is private"));
        }
        Err(e) => return Err(format!("cannot fetch {url}: {e}")),
    };
    let prefix = if path.is_empty() { String::new() } else { format!("{path}/") };
    unpacked(&bytes, repo, &prefix).map_err(|e| format!("cannot read the download of {repo}: {e}"))
}

fn unpacked(bytes: &[u8], repo: &str, prefix: &str) -> Result<Tree, String> {
    let mut archive = tar::Archive::new(GzDecoder::new(bytes));
    let mut tree = Tree::default();
    for (count, entry) in archive.entries().map_err(|e| e.to_string())?.enumerate() {
        if count >= MAX_ENTRIES {
            return Err(format!("it holds more than {MAX_ENTRIES} files"));
        }
        let mut entry = entry.map_err(|e| e.to_string())?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let Some(inside) = inner(&entry.path().map_err(|e| e.to_string())?) else { continue };
        let Some(relative) = inside.strip_prefix(prefix).map(str::to_string) else { continue };
        if !Tree::wanted(&relative) {
            continue;
        }
        if entry.size() > MAX_BYTES as u64 {
            tree.skipped += 1;
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        tree.add(relative, bytes, format!("{repo}/{inside}"))?;
    }
    Ok(tree)
}

fn inner(path: &Path) -> Option<String> {
    let mut parts = path.components();
    parts.next()?;
    let names: Option<Vec<&str>> = parts
        .map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    names.filter(|names| !names.is_empty()).map(|names| names.join("/"))
}

fn divisions(text: &str) -> Option<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    Some(value.get("divisions")?.as_object()?.keys().cloned().collect())
}

fn picked(tree: Tree, what: &str) -> Result<Gathered, String> {
    let kept = tree.divisions.as_deref().and_then(divisions);
    let in_division = |relative: &str| {
        kept.as_ref().is_none_or(|kept| relative.split_once('/').is_some_and(|(top, _)| kept.iter().any(|k| k == top)))
    };
    let mut gathered = Gathered { skipped: tree.skipped, ..Gathered::default() };
    let mut entries = tree.entries;
    entries.sort_by(|a, b| a.relative.cmp(&b.relative));
    for entry in entries {
        if in_division(&entry.relative) {
            let file = entry.relative.rsplit('/').next().unwrap_or_default().to_string();
            gathered.found.push(Found { file, text: entry.text, source: Some(entry.source) });
        } else {
            gathered.skipped += 1;
        }
    }
    if gathered.found.is_empty() {
        return Err(format!("no personas in {what}: a persona is a .md file whose front matter gives its `name`"));
    }
    if gathered.found.len() > MAX_IMPORT {
        return Err(format!(
            "{what} holds {} personas, more than the {MAX_IMPORT} one import takes: import a folder of it",
            gathered.found.len()
        ));
    }
    Ok(gathered)
}

#[cfg(test)]
mod tests;
