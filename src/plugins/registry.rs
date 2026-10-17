use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::manifest::{self, Manifest};
use crate::{persist, text};

const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Entry {
    pub(crate) id: String,
    pub(crate) source: Source,
    #[serde(default)]
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum Source {
    Link {
        path: PathBuf,
    },
    Git {
        repo: String,
        url: String,
        #[serde(rename = "ref", default, skip_serializing_if = "Option::is_none")]
        reference: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subdir: Option<String>,
        commit: String,
        path: PathBuf,
    },
}

impl Source {
    pub(crate) fn root(&self) -> &Path {
        match self {
            Self::Link { path } | Self::Git { path, .. } => path,
        }
    }

    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Link { path } => format!("linked from {}", text::escape_hidden(&path.display().to_string())),
            Self::Git { repo, reference, subdir, commit, .. } => {
                let at = reference.as_ref().map_or_else(String::new, |r| format!("@{r}"));
                let inside = subdir.as_ref().map_or_else(String::new, |s| format!(", folder {s},"));
                text::escape_hidden(&format!("installed from {repo}{at}{inside} at {}", short(commit)))
            }
        }
    }

    pub(crate) fn installed(&self) -> bool {
        matches!(self, Self::Git { .. })
    }
}

pub(crate) fn short(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    version: u32,
    #[serde(default)]
    plugins: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Plugin {
    pub(crate) entry: Entry,
    pub(crate) manifest: Result<Manifest, String>,
}

impl Plugin {
    pub(crate) fn runs(&self) -> Option<&Manifest> {
        self.manifest.as_ref().ok().filter(|_| self.entry.enabled)
    }

    pub(crate) fn root(&self) -> &Path {
        self.entry.source.root()
    }

    pub(crate) fn name(&self) -> String {
        self.manifest.as_ref().map_or_else(|_| text::escape_hidden(&self.entry.id), |m| m.name.clone())
    }
}

#[derive(Debug)]
pub(crate) struct Registry {
    path: PathBuf,
    pub(crate) entries: Vec<Entry>,
}

impl Registry {
    pub(crate) fn load(path: &Path) -> Self {
        let entries = std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
            .filter(|saved| saved.version == VERSION)
            .map(|saved| saved.plugins)
            .unwrap_or_default();
        Self { path: path.to_path_buf(), entries }
    }

    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        let unreadable = |why: &dyn std::fmt::Display| {
            format!("cannot read {}: {why}; fix or remove it, cornercase will not write over it", path.display())
        };
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(Self { path: path.to_path_buf(), entries: Vec::new() });
            }
            Err(e) => return Err(unreadable(&e)),
        };
        let saved: Saved = serde_json::from_slice(&bytes).map_err(|e| unreadable(&e))?;
        if saved.version != VERSION {
            return Err(unreadable(&format!("it is version {}, this cornercase reads {VERSION}", saved.version)));
        }
        Ok(Self { path: path.to_path_buf(), entries: saved.plugins })
    }

    pub(crate) fn save(&self) -> io::Result<()> {
        persist::write(&self.path, &Saved { version: VERSION, plugins: self.entries.clone() }, true)
    }

    pub(crate) fn plugins(&self) -> Vec<Plugin> {
        self.entries
            .iter()
            .map(|entry| Plugin { entry: entry.clone(), manifest: Manifest::load(entry.source.root()) })
            .collect()
    }

    pub(crate) fn link(&mut self, folder: &Path) -> Result<(Manifest, bool), String> {
        let cannot = |e: io::Error| text::escape_hidden(&format!("cannot open {}: {e}", folder.display()));
        let path = folder.canonicalize().map_err(cannot)?;
        let manifest = Manifest::load(&path)?;
        let source = Source::Link { path };
        let enabled = match self.entries.iter_mut().find(|entry| entry.id == manifest.id) {
            Some(entry) if entry.source == source => entry.enabled,
            Some(entry) => {
                return Err(format!("a plugin called `{}` is already {}", manifest.id, entry.source.describe()));
            }
            None => {
                self.entries.push(Entry { id: manifest.id.clone(), source, enabled: false });
                false
            }
        };
        Ok((manifest, enabled))
    }

    pub(crate) fn entry(&self, id: &str) -> Result<&Entry, String> {
        self.entries.iter().find(|entry| entry.id == id).ok_or_else(|| missing(id))
    }

    pub(crate) fn unlink(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        self.entries.len() != before
    }

    pub(crate) fn set_enabled(&mut self, id: &str, on: bool) -> Result<Plugin, String> {
        let entry = self.entries.iter_mut().find(|entry| entry.id == id).ok_or_else(|| missing(id))?;
        let manifest = Manifest::load(entry.source.root());
        if on && let Err(e) = &manifest {
            return Err(format!("`{id}` cannot run: {e}"));
        }
        entry.enabled = on;
        Ok(Plugin { entry: entry.clone(), manifest })
    }
}

pub(crate) fn missing(id: &str) -> String {
    format!("no plugin called `{id}`; `cornercase plugin list` shows them")
}

#[derive(Debug)]
pub(crate) struct Watch {
    path: PathBuf,
    marks: Option<Vec<persist::Mark>>,
}

impl Watch {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path, marks: None }
    }

    pub(crate) fn changed(&mut self) -> Option<Vec<Plugin>> {
        let registry = Registry::load(&self.path);
        let mut marks = vec![persist::mark(&self.path)];
        marks.extend(registry.entries.iter().map(|entry| persist::mark(&entry.source.root().join(manifest::FILE))));
        if self.marks.as_ref() == Some(&marks) {
            return None;
        }
        self.marks = Some(marks);
        Some(registry.plugins())
    }
}

#[cfg(test)]
mod tests;
