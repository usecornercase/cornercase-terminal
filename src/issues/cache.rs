use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{Issue, Query, Source};
use crate::log;

const VERSION: u32 = 1;
const MAX_LISTS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Key {
    pub source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<PathBuf>,
    pub query: Query,
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
pub struct Cache {
    path: Option<PathBuf>,
    loaded: bool,
    lists: Vec<Entry>,
}

impl Cache {
    pub fn new(path: Option<PathBuf>) -> Self {
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
        if let Err(e) = crate::state::save(path, &file) {
            log::warning!("issues", "failed to save the issue cache", error = e);
        }
    }

    pub fn get(&mut self, key: &Key) -> Option<Vec<Issue>> {
        self.load();
        self.lists.iter().find(|e| e.key == *key).map(|e| e.issues.clone())
    }

    pub fn put(&mut self, key: Key, issues: Vec<Issue>) {
        self.load();
        self.lists.retain(|e| e.key != key);
        self.lists.insert(0, Entry { key, issues });
        self.lists.truncate(MAX_LISTS);
        self.save();
    }

    pub fn forget(&mut self, source: Source) {
        self.load();
        self.lists.retain(|e| e.key.source != source);
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issues::issue;
    use crate::test_util::TempDir;

    fn key(source: Source) -> Key {
        Key { source, project: (source == Source::Github).then(|| "/p".into()), query: Query::default() }
    }

    #[test]
    fn a_list_survives_a_new_cache_on_the_same_file() {
        let tmp = TempDir::new();
        let path = tmp.path().join("issues.json");
        Cache::new(Some(path.clone())).put(key(Source::Github), vec![issue(Source::Github, 7, "x")]);

        let found = Cache::new(Some(path)).get(&key(Source::Github));

        assert_eq!(found.map(|l| l.len()), Some(1));
    }

    #[test]
    fn another_query_is_another_list() {
        let mut cache = Cache::new(None);
        cache.put(key(Source::Github), vec![issue(Source::Github, 7, "x")]);
        let closed = Key { query: Query { closed: true, ..Query::default() }, ..key(Source::Github) };
        assert_eq!(cache.get(&closed), None);
    }

    #[test]
    fn forgetting_a_source_drops_its_lists_only() {
        let mut cache = Cache::new(None);
        cache.put(key(Source::Github), Vec::new());
        cache.put(key(Source::Linear), Vec::new());

        cache.forget(Source::Linear);

        assert_eq!((cache.get(&key(Source::Github)).is_some(), cache.get(&key(Source::Linear))), (true, None));
    }

    #[test]
    fn keeps_only_the_most_recent_lists() {
        let mut cache = Cache::new(None);
        for n in 0..=MAX_LISTS {
            let project = Some(PathBuf::from(format!("/p{n}")));
            cache.put(Key { project, ..key(Source::Github) }, Vec::new());
        }
        let oldest = Key { project: Some("/p0".into()), ..key(Source::Github) };
        assert_eq!((cache.lists.len(), cache.get(&oldest)), (MAX_LISTS, None));
    }

    #[test]
    fn a_corrupt_file_is_an_empty_cache() {
        let tmp = TempDir::new();
        let path = tmp.path().join("issues.json");
        std::fs::write(&path, "{ nope").expect("write");
        assert_eq!(Cache::new(Some(path)).get(&key(Source::Github)), None);
    }
}
