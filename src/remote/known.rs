use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;

use super::shell::RemoteShell;
use crate::{paths, persist};

static KNOWN: Mutex<Option<Known>> = Mutex::new(None);

type Shells = BTreeMap<String, RemoteShell>;

struct Known {
    file: PathBuf,
    shells: Shells,
}

impl Known {
    fn at(file: PathBuf) -> Self {
        let shells = read(&file);
        Self { file, shells }
    }

    fn of(&self, destination: &str) -> Option<RemoteShell> {
        self.shells.get(destination).copied()
    }

    fn learn(&mut self, destination: &str, shell: RemoteShell) {
        if self.of(destination) == Some(shell).filter(|shell| shell.windows()) {
            return;
        }
        let mut shells = read(&self.file);
        let changed = if shell.windows() {
            shells.insert(destination.to_string(), shell) != Some(shell)
        } else {
            shells.remove(destination).is_some()
        };
        if changed {
            let _ = persist::write(&self.file, &shells, true);
        }
        self.shells = shells;
    }
}

fn read(file: &Path) -> Shells {
    std::fs::read(file).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn with<T>(act: impl FnOnce(&mut Known) -> T) -> T {
    let mut known = KNOWN.lock();
    act(known.get_or_insert_with(|| Known::at(paths::remote_shells(&paths::session()))))
}

pub(super) fn shell(destination: &str) -> Option<RemoteShell> {
    with(|known| known.of(destination))
}

pub(super) fn learn(destination: &str, shell: RemoteShell) {
    with(|known| known.learn(destination, shell));
}

#[cfg(test)]
mod tests;
