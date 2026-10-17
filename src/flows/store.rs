use std::io::Write;
use std::path::{Path, PathBuf};

use super::Flow;
use crate::error::{Error, Result};

pub(crate) const EXTENSION: &str = "toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Origin {
    BuiltIn,
    User(PathBuf),
    Project(PathBuf),
}

impl Origin {
    pub(crate) fn editable(&self) -> bool {
        !matches!(self, Self::BuiltIn)
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        match self {
            Self::BuiltIn => None,
            Self::User(path) | Self::Project(path) => Some(path),
        }
    }

    #[cfg(test)]
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Self::BuiltIn => "built in",
            Self::User(_) => "yours",
            Self::Project(_) => "this project",
        }
    }

    pub(crate) fn whose(&self) -> &'static str {
        match self {
            Self::BuiltIn => "the built-in one",
            Self::User(_) => "yours",
            Self::Project(_) => "this project's",
        }
    }

    fn rank(&self) -> u8 {
        match self {
            Self::User(_) => 0,
            Self::BuiltIn => 1,
            Self::Project(_) => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Saved {
    pub(crate) flow: Flow,
    pub(crate) origin: Origin,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Listing {
    pub(crate) flows: Vec<Saved>,
    pub(crate) broken: Vec<(PathBuf, String)>,
    pub(crate) builtins: Vec<String>,
}

pub(crate) fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

impl Listing {
    pub(crate) fn named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Saved> {
        self.flows.iter().filter(move |saved| same_name(&saved.flow.name, name))
    }

    pub(crate) fn conflict(&self, saved: &Saved) -> Option<String> {
        conflict(&self.flows, saved)
    }
}

pub(crate) fn conflict(flows: &[Saved], saved: &Saved) -> Option<String> {
    let first = flows.iter().find(|other| same_name(&other.flow.name, &saved.flow.name))?;
    (first.origin != saved.origin)
        .then(|| format!("{} is also named `{}` and runs instead", first.origin.whose(), saved.flow.name.trim()))
}

pub(crate) fn edits_builtin(builtins: &[String], saved: &Saved) -> bool {
    matches!(saved.origin, Origin::User(_)) && builtins.iter().any(|name| same_name(name, &saved.flow.name))
}

pub(crate) fn tag(builtins: &[String], saved: &Saved) -> Option<&'static str> {
    match saved.origin {
        Origin::Project(_) => Some("this project"),
        Origin::User(_) if edits_builtin(builtins, saved) => Some("edited"),
        Origin::User(_) | Origin::BuiltIn => None,
    }
}

pub(crate) fn project_dir(project: &Path) -> PathBuf {
    project.join(".cornercase").join("flows")
}

pub(crate) fn list(builtins: Vec<Flow>, user: &Path, project: Option<&Path>) -> Listing {
    let mut listing = Listing {
        builtins: builtins.iter().map(|flow| flow.name.clone()).collect(),
        flows: builtins.into_iter().map(|flow| Saved { flow, origin: Origin::BuiltIn }).collect(),
        broken: Vec::new(),
    };
    read_dir(user, Origin::User, &mut listing);
    if let Some(project) = project {
        read_dir(project, Origin::Project, &mut listing);
    }
    let yours: Vec<String> = listing
        .flows
        .iter()
        .filter(|saved| matches!(saved.origin, Origin::User(_)))
        .map(|saved| saved.flow.name.clone())
        .collect();
    listing
        .flows
        .retain(|saved| saved.origin != Origin::BuiltIn || !yours.iter().any(|n| same_name(n, &saved.flow.name)));
    listing.flows.sort_by_key(|saved| (saved.flow.name.trim().to_lowercase(), saved.origin.rank()));
    listing
}

fn read_dir(dir: &Path, origin: fn(PathBuf) -> Origin, listing: &mut Listing) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == EXTENSION) && path.is_file())
        .collect();
    paths.sort();
    for path in paths {
        match read(&path) {
            Ok(flow) => listing.flows.push(Saved { flow, origin: origin(path) }),
            Err(e) => listing.broken.push((path, e.to_string())),
        }
    }
}

pub(crate) fn read(path: &Path) -> Result<Flow> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::FlowFile { path: path.to_path_buf(), reason: e.to_string() })?;
    parse(&text).map_err(|reason| Error::FlowFile { path: path.to_path_buf(), reason })
}

pub(crate) fn parse(text: &str) -> std::result::Result<Flow, String> {
    let mut flow: Flow = toml::from_str(text).map_err(|e| match e.span() {
        Some(span) => format!("line {}: {}", text[..span.start.min(text.len())].matches('\n').count() + 1, e.message()),
        None => e.message().to_string(),
    })?;
    super::ORCHESTRATOR.clone_into(&mut flow.orchestrator.name);
    if flow.version > super::VERSION {
        return Err("it was made by a newer cornercase".to_string());
    }
    if let Some(role) =
        flow.roles.iter().find(|role| role.own_worktree()).filter(|_| flow.version < flow.file_version())
    {
        return Err(format!(
            "it gives `{}` a worktree of its own, which needs `version = {}`: an older cornercase would run it in \
             the shared worktree",
            role.name.trim(),
            flow.file_version()
        ));
    }
    flow.version = super::VERSION;
    Ok(flow)
}

pub(crate) fn to_text(flow: &Flow) -> std::result::Result<String, String> {
    let versioned = Flow { version: flow.file_version(), ..flow.clone() };
    toml::to_string(&versioned).map_err(|e| e.to_string())
}

pub(crate) fn file_name(name: &str) -> String {
    let mut slug = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() { "flow".to_string() } else { slug.to_string() }
}

pub(crate) fn save(dir: &Path, flow: &Flow, previous: Option<&Path>) -> Result<PathBuf> {
    let text = to_text(flow).map_err(|reason| Error::FlowFile { path: dir.to_path_buf(), reason })?;
    std::fs::create_dir_all(dir)?;
    let stem = file_name(&flow.name);
    let path = (1..=1000)
        .map(|n| if n == 1 { stem.clone() } else { format!("{stem}-{n}") })
        .map(|stem| dir.join(format!("{stem}.{EXTENSION}")))
        .find(|path| Some(path.as_path()) == previous || !path.exists())
        .ok_or_else(|| Error::FlowFile { path: dir.to_path_buf(), reason: "too many flows share its name".into() })?;
    let tmp = path.with_extension(format!("{EXTENSION}.tmp"));
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&tmp, &path)?;
    if let Some(previous) = previous.filter(|previous| *previous != path) {
        std::fs::remove_file(previous)?;
    }
    Ok(path)
}

pub(crate) fn delete(path: &Path) -> Result<()> {
    std::fs::remove_file(path)?;
    Ok(())
}
