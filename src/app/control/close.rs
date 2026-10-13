use super::{Handled, Pending, Stage, json, none};
use crate::app::App;
use crate::control::{self, Done, Item};
use crate::error;
use crate::term::Term;
use crate::worktree;

pub(super) fn shell_quote(path: &str) -> String {
    if !path.is_empty() && path.chars().all(|c| c.is_ascii_alphanumeric() || "/._~+-".contains(c)) {
        return path.to_string();
    }
    format!("'{}'", path.replace('\'', "'\\''"))
}

impl App {
    pub(super) fn close_request(&mut self, client: u64, close: &control::Close) -> Handled {
        match close.item {
            Item::Pane(id) => self.pane_by_mut(id).ok_or_else(|| none("pane", id))?.kill(),
            Item::Tab(id) => {
                let (p, w, t) = self.locate_tab(id).ok_or_else(|| none("tab", id))?;
                self.projects[p].workspaces[w].tabs[t].panes.iter_mut().for_each(Term::kill);
            }
            Item::Workspace(id) => {
                let (p, w) = self.usable_workspace(id)?;
                if close.remove_worktree {
                    if !self.projects[p].workspaces[w].worktree {
                        return Err(format!("workspace {id} is not a git worktree"));
                    }
                    let key = self.requests.key();
                    self.spawn_check(p, w, Some(key));
                    let pending = Pending {
                        client: Some(client),
                        key,
                        timeout: None,
                        done: Done::default(),
                        stage: Stage::Removing { force: close.force },
                    };
                    self.requests.pending.push(pending);
                    return Ok(None);
                }
                let workspace = &mut self.projects[p].workspaces[w];
                if workspace.worktree {
                    workspace.kill();
                } else {
                    self.drop_workspace(self.projects[p].id, id);
                }
            }
            Item::Project(id) => {
                self.project_index(id).ok_or_else(|| none("project", id))?;
                self.close_project(id);
            }
            Item::Group(id) => return Err(format!("group {id} cannot be closed; close its projects")),
        }
        Ok(Some(json(&Done::default())))
    }

    pub(in crate::app) fn removal_checked(
        &mut self,
        key: u64,
        project: u64,
        workspace: u64,
        status: &worktree::Status,
    ) {
        let force =
            self.requests.pending.iter().any(|p| p.key == key && matches!(p.stage, Stage::Removing { force: true }));
        let clear = status.lock.is_none() && (force || !status.changed);
        if clear && self.start_removal(project, workspace, force, false, Some(key)) {
            return;
        }
        let Some(pending) = self.requests.take(key) else { return };
        let message = if let Some(lock) = &status.lock {
            self.locked(workspace, lock)
        } else if status.changed && !force {
            format!("workspace {workspace} has changes that are not committed; add --force to remove it anyway")
        } else {
            self.usable_workspace(workspace).err().unwrap_or_else(|| none("workspace", workspace))
        };
        self.answer(pending.client, Err(message));
    }

    fn locked(&self, workspace: u64, lock: &worktree::Lock) -> String {
        let path = self
            .workspace_position(workspace)
            .map(|(p, w)| self.projects[p].workspaces[w].path.display().to_string())
            .unwrap_or_default();
        let reason = if lock.reason.is_empty() { String::new() } else { format!(" ({})", lock.reason) };
        format!(
            "workspace {workspace} is locked{reason}; unlock it with `git worktree unlock {}` to remove it",
            shell_quote(&path)
        )
    }

    pub(in crate::app) fn worktree_gone(&mut self, key: u64, project: u64, workspace: u64, result: error::Result<()>) {
        let pending = self.requests.take(key);
        self.settle_removal(project, workspace, result.is_ok());
        let Some(pending) = pending else { return };
        self.answer(pending.client, result.map(|()| json(&Done::default())).map_err(|e| e.to_string()));
    }
}

#[cfg(test)]
mod tests;
