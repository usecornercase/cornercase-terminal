use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Issue, Query, Source};
use crate::log;

const VERSION: u32 = 1;
const MAX_LISTS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Key {
    pub(crate) source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) project: Option<PathBuf>,
    pub(crate) query: Query,
}

#[derive(Debug, Serialize, Deserialize)]
struct File {
    version: u32,
    lists: Vec<Entry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    key: Key,
    issues: Vec<Issue>,
}

#[derive(Debug, Default)]
pub(crate) struct Cache {
    path: Option<PathBuf>,
    loaded: bool,
    lists: Vec<Entry>,
}

impl Cache {
    pub(crate) fn new(path: Option<PathBuf>) -> Self {
        Self { path, loaded: false, lists: Vec::new() }
    }

    fn load(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        let Some(path) = &self.path else { return };
        let file: Option<File> = std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok());
        if let Some(file) = file.filter(|f| f.version == VERSION) {
            self.lists = file.lists;
        }
    }

    fn save(&self) {
        let Some(path) = &self.path else { return };
        let file = File {
            version: VERSION,
            lists: self.lists.iter().map(|e| Entry { key: e.key.clone(), issues: e.issues.clone() }).collect(),
        };
        if let Err(e) = crate::persist::save(path, &file) {
            log::warning!("issues", "failed to save the issue cache", error = e);
        }
    }

    pub(crate) fn get(&mut self, key: &Key) -> Option<Vec<Issue>> {
        self.load();
        self.lists.iter().find(|e| e.key == *key).map(|e| e.issues.clone())
    }

    pub(crate) fn put(&mut self, key: Key, issues: Vec<Issue>) {
        self.load();
        self.lists.retain(|e| e.key != key);
        self.lists.insert(0, Entry { key, issues });
        self.lists.truncate(MAX_LISTS);
        self.save();
    }

    pub(crate) fn forget(&mut self, source: Source) {
        self.load();
        self.lists.retain(|e| e.key.source != source);
        self.save();
    }
}

#[cfg(test)]
mod tests;
