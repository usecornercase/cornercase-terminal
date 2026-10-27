use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::App;
use crate::files::pasted;
use crate::log::{Job, Level};
use crate::project::Hierarchy;

#[derive(Default)]
pub(super) struct PastedFiles {
    dir: Option<PathBuf>,
    kept: Vec<(u64, PathBuf)>,
}

impl PastedFiles {
    pub(super) fn folder(&self) -> Option<PathBuf> {
        self.dir.clone()
    }

    pub(super) fn folders(&self) -> Vec<PathBuf> {
        self.kept.iter().map(|(_, folder)| folder.clone()).collect()
    }

    pub(super) fn keep(&mut self, pane: u64, folder: PathBuf) {
        self.kept.push((pane, folder));
    }

    fn gone(&mut self, alive: impl Fn(u64) -> bool) -> Vec<PathBuf> {
        let (gone, kept) = std::mem::take(&mut self.kept).into_iter().partition(|(pane, _)| !alive(*pane));
        self.kept = kept;
        gone.into_iter().map(|(_, folder)| folder).collect()
    }
}

impl App {
    pub(crate) fn set_pasted_dir(&mut self, dir: &Path) {
        let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
        self.clients.pasted.dir = Some(dir.clone());
        let before = SystemTime::now().checked_sub(pasted::KEEP_FOR).unwrap_or(SystemTime::UNIX_EPOCH);
        self.spawn_job(
            Job::new(Level::Debug, "files", "pasted swept"),
            move || {
                pasted::sweep(&dir, &[], before);
                Ok(())
            },
            |_, _| None,
        );
    }

    pub(super) fn forget_pasted(&mut self) {
        let projects = &self.projects;
        let gone = self.clients.pasted.gone(|pane| projects.pane_by(pane).is_some());
        if !gone.is_empty() {
            self.drop_pasted(gone);
        }
    }

    pub(in crate::app) fn drop_pasted(&self, dirs: Vec<PathBuf>) {
        self.spawn_job(
            Job::new(Level::Debug, "files", "pasted removed"),
            move || {
                pasted::remove(&dirs);
                Ok(())
            },
            |_, _| None,
        );
    }
}
