use std::io;
use std::path::{Path, PathBuf};

use crate::list::Cursor;
use crate::{git, project::vscode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) branch: Option<String>,
    pub(crate) workspace: bool,
}

#[derive(Debug)]
pub(crate) struct Picker {
    dir: PathBuf,
    home: Option<PathBuf>,
    parent: Option<Item>,
    folders: Vec<Item>,
    filter: String,
    selected: Option<usize>,
    scroll: usize,
    error: Option<String>,
}

impl Picker {
    pub(crate) fn open(dir: &Path, home: Option<&Path>) -> io::Result<Self> {
        let parent =
            dir.parent().map(|p| Item { name: "..".into(), path: p.to_path_buf(), branch: None, workspace: false });
        Ok(Self {
            dir: dir.to_path_buf(),
            home: home.map(Path::to_path_buf),
            parent,
            folders: entries(dir)?,
            filter: String::new(),
            selected: None,
            scroll: 0,
            error: None,
        })
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn filter(&self) -> &str {
        &self.filter
    }

    pub(crate) fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub(crate) fn scroll(&self) -> usize {
        self.scroll
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn items(&self) -> Vec<&Item> {
        let needle = self.filter.to_lowercase();
        let show_hidden = self.filter.starts_with('.');
        let mut matches: Vec<(u8, &Item)> = self
            .folders
            .iter()
            .filter(|f| show_hidden || !f.name.starts_with('.'))
            .filter_map(|f| {
                let name = f.name.to_lowercase();
                let rank = if name == needle {
                    0
                } else if name.starts_with(&needle) {
                    1
                } else if name.contains(&needle) {
                    2
                } else {
                    return None;
                };
                Some((rank, f))
            })
            .collect();
        matches.sort_by_key(|(rank, item)| (item.workspace, *rank));
        let parent = self.parent.as_ref().filter(|_| self.filter.is_empty());
        parent.into_iter().chain(matches.into_iter().map(|(_, f)| f)).collect()
    }

    pub(crate) fn push(&mut self, c: char) {
        self.error = None;
        match c {
            '/' if self.filter.is_empty() && self.selected.is_none() => self.go(Path::new("/")),
            '/' => self.enter_selected(),
            '~' if self.filter.is_empty() => {
                if let Some(home) = self.home.clone() {
                    self.go(&home);
                }
            }
            _ => {
                self.filter.push(c);
                self.selected = (!self.items().is_empty()).then_some(0);
                self.scroll = 0;
            }
        }
    }

    pub(crate) fn pop(&mut self) {
        self.error = None;
        if self.filter.pop().is_none() {
            self.up();
            return;
        }
        self.selected = (!self.filter.is_empty() && !self.items().is_empty()).then_some(0);
        self.scroll = 0;
    }

    pub(crate) fn up(&mut self) {
        if let Some(parent) = self.dir.parent().map(Path::to_path_buf) {
            self.go(&parent);
        }
    }

    pub(crate) fn enter(&mut self, i: usize) {
        if let Some(path) = self.items().get(i).filter(|item| !item.workspace).map(|item| item.path.clone()) {
            self.go(&path);
        }
    }

    pub(crate) fn choose(&mut self, i: usize) -> Option<PathBuf> {
        let item = self.items().get(i).map(|item| (item.workspace, item.path.clone()));
        match item {
            Some((true, path)) => Some(path),
            Some((false, path)) => {
                self.go(&path);
                None
            }
            None => None,
        }
    }

    pub(crate) fn fail(&mut self, error: String) {
        self.error = Some(error);
    }

    pub(crate) fn enter_selected(&mut self) {
        if let Some(i) = self.selected {
            self.enter(i);
        }
    }

    pub(crate) fn submit(&mut self) -> Option<PathBuf> {
        match self.selected {
            Some(i) => self.choose(i),
            None if self.filter.is_empty() => Some(self.dir.clone()),
            None => None,
        }
    }

    pub(crate) fn move_selection(&mut self, delta: isize, rows: usize) {
        let mut cursor = Cursor { selected: self.selected, scroll: self.scroll };
        cursor.move_by(delta, self.items().len(), rows);
        (self.selected, self.scroll) = (cursor.selected, cursor.scroll);
    }

    pub(crate) fn scroll_by(&mut self, delta: isize, rows: usize) {
        let mut cursor = Cursor { selected: self.selected, scroll: self.scroll };
        cursor.scroll_by(delta, self.items().len(), rows);
        self.scroll = cursor.scroll;
    }

    fn go(&mut self, dir: &Path) {
        match Self::open(dir, self.home.as_deref()) {
            Ok(picker) => *self = picker,
            Err(e) => self.error = Some(format!("cannot open {}: {e}", dir.display())),
        }
    }
}

fn entries(dir: &Path) -> io::Result<Vec<Item>> {
    let mut entries: Vec<Item> = std::fs::read_dir(dir)?
        .flatten()
        .map(|entry| entry.path())
        .filter_map(|path| {
            let workspace = vscode::is_workspace(&path) && path.is_file();
            if !workspace && !path.is_dir() {
                return None;
            }
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let branch = if path.join(".git").exists() { git::branch(&path) } else { None };
            Some(Item { name, path, branch, workspace })
        })
        .collect();
    entries.sort_by(|a, b| {
        a.workspace
            .cmp(&b.workspace)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

#[cfg(test)]
mod tests;
