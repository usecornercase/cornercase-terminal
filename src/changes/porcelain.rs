use std::collections::HashMap;
use std::path::Path;

use super::diff::{File, Staged, Status};

#[derive(Clone, Copy, Default)]
struct Sides {
    staged: bool,
    unstaged: bool,
}

impl Sides {
    fn or(self, other: Self) -> Self {
        Self { staged: self.staged || other.staged, unstaged: self.unstaged || other.unstaged }
    }
}

#[derive(Default)]
pub(super) struct Porcelain {
    pub(super) statuses: HashMap<String, Status>,
    pub(super) untracked: Vec<String>,
    pub(super) renames: HashMap<String, String>,
    sides: HashMap<String, Sides>,
}

impl Porcelain {
    pub(super) fn parse(raw: &[u8], prefix: &str) -> Self {
        let mut porcelain = Self::default();
        let mut records = raw.split(|b| *b == 0);
        while let Some(record) = records.next() {
            let text = String::from_utf8_lossy(record);
            let old_path = if record.first() == Some(&b'2') {
                records.next().map(|old| String::from_utf8_lossy(old).into_owned())
            } else {
                None
            };
            porcelain.take(&text, old_path, prefix);
        }
        porcelain
    }

    fn take(&mut self, text: &str, old_path: Option<String>, prefix: &str) {
        let fields: Vec<&str> = match text.as_bytes().first() {
            Some(b'1') => text.splitn(9, ' ').collect(),
            Some(b'2') => text.splitn(10, ' ').collect(),
            Some(b'u') => text.splitn(11, ' ').collect(),
            Some(b'?') => {
                if let Some(path) = text.strip_prefix("? ").and_then(|p| p.strip_prefix(prefix)) {
                    self.untracked.push(path.to_string());
                }
                return;
            }
            _ => return,
        };
        let (Some(xy), Some(path)) = (fields.get(1), fields.last()) else { return };
        let Some(path) = path.strip_prefix(prefix) else { return };
        if let Some(old) = old_path.and_then(|old| old.strip_prefix(prefix).map(str::to_string)) {
            self.renames.insert(path.to_string(), old);
        }
        self.statuses.insert(path.to_string(), status_of(xy));
        self.sides.insert(path.to_string(), sides_of(text.as_bytes()[0], xy));
    }

    pub(super) fn staged(&self, file: &File) -> Staged {
        if file.status == Status::Untracked {
            return Staged::No;
        }
        let found = |path: &str| self.sides.get(path).copied().unwrap_or_default();
        let sides = file.old_path.as_deref().map_or_else(|| found(&file.path), |old| found(&file.path).or(found(old)));
        match (sides.staged, sides.unstaged) {
            (true, false) => Staged::Yes,
            (true, true) => Staged::Partly,
            _ => Staged::No,
        }
    }
}

fn status_of(xy: &str) -> Status {
    if xy.contains('R') {
        Status::Renamed
    } else if xy.contains('D') {
        Status::Deleted
    } else if xy.contains('A') {
        Status::Added
    } else {
        Status::Modified
    }
}

fn sides_of(kind: u8, xy: &str) -> Sides {
    if kind == b'u' {
        return Sides { staged: false, unstaged: true };
    }
    let mut chars = xy.chars();
    let (x, y) = (chars.next().unwrap_or('.'), chars.next().unwrap_or('.'));
    Sides { staged: x != '.', unstaged: y != '.' }
}

pub(super) fn path_prefix(dir: &Path) -> String {
    dir.ancestors()
        .find(|root| root.join(".git").exists())
        .and_then(|root| dir.strip_prefix(root).ok())
        .filter(|prefix| !prefix.as_os_str().is_empty())
        .map_or_else(String::new, |prefix| format!("{}/", prefix.display()))
}

#[cfg(test)]
mod tests;
