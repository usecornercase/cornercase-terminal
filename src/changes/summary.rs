use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use super::diff::{self, Diff, File, Fold, Status};
use super::git::{self, Loaded, Request};
use crate::error::Result;

fn statuses(raw: &[u8], prefix: &str) -> (HashMap<String, Status>, Vec<String>, HashMap<String, String>) {
    let mut records = raw.split(|b| *b == 0);
    let mut statuses = HashMap::new();
    let mut untracked = Vec::new();
    let mut renames = HashMap::new();
    while let Some(record) = records.next() {
        let text = String::from_utf8_lossy(record);
        let old_path = if record.first() == Some(&b'2') {
            records.next().map(|old| String::from_utf8_lossy(old).into_owned())
        } else {
            None
        };
        let fields: Vec<&str> = match record.first() {
            Some(b'1') => text.splitn(9, ' ').collect(),
            Some(b'2') => text.splitn(10, ' ').collect(),
            Some(b'?') => {
                if let Some(path) = text.strip_prefix("? ").and_then(|p| p.strip_prefix(prefix)) {
                    untracked.push(path.to_string());
                }
                continue;
            }
            Some(b'u') => text.splitn(11, ' ').collect(),
            _ => continue,
        };
        let (Some(xy), Some(path)) = (fields.get(1), fields.last()) else { continue };
        let Some(path) = path.strip_prefix(prefix) else { continue };
        if let Some(old) = old_path.and_then(|old| old.strip_prefix(prefix).map(str::to_string)) {
            renames.insert(path.to_string(), old);
        }
        let status = if xy.contains('R') {
            Status::Renamed
        } else if xy.contains('D') {
            Status::Deleted
        } else if xy.contains('A') {
            Status::Added
        } else {
            Status::Modified
        };
        statuses.insert(path.to_string(), status);
    }
    (statuses, untracked, renames)
}

fn counted(raw: &[u8], statuses: &HashMap<String, Status>) -> Vec<File> {
    let mut records = raw.split(|b| *b == 0);
    let mut files = Vec::new();
    while let Some(record) = records.next().filter(|r| !r.is_empty()) {
        let text = String::from_utf8_lossy(record);
        let fields: Vec<&str> = text.splitn(3, '\t').collect();
        let [added, removed, path] = fields.as_slice() else { continue };
        let (path, old_path) = if path.is_empty() {
            let old = records.next().map(|r| String::from_utf8_lossy(r).into_owned());
            let Some(new) = records.next() else { continue };
            (String::from_utf8_lossy(new).into_owned(), old)
        } else {
            ((*path).to_string(), None)
        };
        let status =
            if old_path.is_some() { Status::Renamed } else { statuses.get(&path).copied().unwrap_or(Status::Modified) };
        let binary = *added == "-";
        let (added, removed) = (added.parse().unwrap_or(0), removed.parse().unwrap_or(0));
        let fold = Fold::of(&path, binary, added, removed);
        files.push(File {
            path,
            old_path,
            status,
            added,
            removed,
            fold,
            hunks: Vec::new(),
            digest: 0,
            viewed_digest: 0,
        });
    }
    files
}

pub(super) fn load(request: &Request, expanded: &[String]) -> Result<Loaded> {
    let dir = request.dir.as_path();
    let from = git::head(dir)?;
    let status = git::run(dir, &["status", "--porcelain=v2", "-z", "--untracked-files=all", "--", "."])?;
    let numstat = git::run(
        dir,
        &[
            "diff",
            "--numstat",
            "-z",
            "--find-renames",
            "--relative",
            "--no-ext-diff",
            "--no-textconv",
            &from,
            "--",
            ".",
        ],
    )?;
    let prefix = dir
        .ancestors()
        .find(|root| root.join(".git").exists())
        .and_then(|root| dir.strip_prefix(root).ok())
        .filter(|prefix| !prefix.as_os_str().is_empty())
        .map_or_else(String::new, |prefix| format!("{}/", prefix.display()));
    let (statuses, paths, renames) = statuses(&status, &prefix);
    let untracked = git::read_untracked(dir, paths);
    let previous: HashMap<&str, &Arc<File>> = request.previous.iter().map(|f| (f.path.as_str(), f)).collect();
    let paths: Vec<&str> = expanded
        .iter()
        .filter(|path| !untracked.iter().any(|u| u.path == **path))
        .flat_map(|path| {
            let old = renames
                .get(path)
                .map(String::as_str)
                .or_else(|| previous.get(path.as_str()).and_then(|file| file.old_path.as_deref()));
            std::iter::once(path.as_str()).chain(old)
        })
        .collect();
    let patch = if paths.is_empty() { Vec::new() } else { git::patch(dir, &from, &paths)? };
    let mut hasher = DefaultHasher::new();
    (&from, &status, &numstat, &untracked, expanded, &patch).hash(&mut hasher);
    let mut tracked: Vec<_> = statuses.keys().collect();
    tracked.sort();
    for path in tracked {
        (path, git::stamp(&dir.join(path))).hash(&mut hasher);
    }
    let digest = hasher.finish();
    if request.last == Some(digest) {
        return Ok(Loaded { base: None, digest, diff: None });
    }
    let mut files = counted(&numstat, &statuses);
    files.extend(untracked.iter().map(|u| diff::untracked_file(&u.path, u.size, u.bytes.as_deref(), false)));
    for file in &mut files {
        file.viewed_digest = git::fingerprint(dir, &from, file);
        file.digest = file.viewed_digest;
    }
    let mut patches: HashMap<String, File> =
        diff::parse(&String::from_utf8_lossy(&patch)).into_iter().map(|f| (f.path.clone(), f)).collect();
    let files = files
        .into_iter()
        .map(|mut file| {
            let open = expanded.contains(&file.path);
            if open {
                let loaded = if file.status == Status::Untracked {
                    untracked
                        .iter()
                        .find(|u| u.path == file.path)
                        .map(|u| diff::untracked(&u.path, u.size, u.bytes.as_deref()))
                } else {
                    patches.remove(&file.path)
                };
                if let Some(mut loaded) = loaded {
                    loaded.viewed_digest = file.viewed_digest;
                    file = loaded;
                }
            }
            git::reuse(file, &previous)
        })
        .collect::<Vec<_>>();
    let mut files = files;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Loaded { base: None, digest, diff: Some(Diff { files }) })
}

#[cfg(test)]
mod tests;
