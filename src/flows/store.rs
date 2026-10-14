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
}

pub(crate) fn project_dir(project: &Path) -> PathBuf {
    project.join(".cornercase").join("flows")
}

pub(crate) fn list(builtins: Vec<Flow>, user: &Path, project: Option<&Path>) -> Listing {
    let mut listing = Listing {
        flows: builtins.into_iter().map(|flow| Saved { flow, origin: Origin::BuiltIn }).collect(),
        broken: Vec::new(),
    };
    read_dir(user, Origin::User, &mut listing);
    if let Some(project) = project {
        read_dir(project, Origin::Project, &mut listing);
    }
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
    let mut flow: Flow = toml::from_str(text).map_err(|e| e.message().to_string())?;
    super::ORCHESTRATOR.clone_into(&mut flow.orchestrator.name);
    if flow.version > super::VERSION {
        return Err("it was made by a newer cornercase".to_string());
    }
    Ok(flow)
}

pub(crate) fn to_text(flow: &Flow) -> std::result::Result<String, String> {
    toml::to_string(flow).map_err(|e| e.to_string())
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
