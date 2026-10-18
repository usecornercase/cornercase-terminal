use super::{Handled, Pending, Stage, json, malformed, none};
use crate::app::App;
use crate::control::arrange::{Locked, Removal, RemovalCheck, RoleTree};
use crate::control::contract::Refusal;
use crate::control::{self, Done, Item};
use crate::error;
use crate::git::{self, worktree};
use crate::project::Hierarchy;
use crate::term::Term;

pub(super) fn shell_quote(path: &str) -> String {
    if !path.is_empty() && path.chars().all(|c| c.is_ascii_alphanumeric() || "/._~+-".contains(c)) {
        return path.to_string();
    }
    format!("'{}'", path.replace('\'', "'\\''"))
}

impl App {
    pub(super) fn close_request(&mut self, client: u64, close: &control::Close) -> Handled {
        match close.item {
            Item::Pane(id) => self.projects.pane_by_mut(id).ok_or_else(|| none("pane", id))?.kill(),
            Item::Tab(id) => {
                let (p, w, t) = self.projects.tab_index(id).ok_or_else(|| none("tab", id))?;
                self.projects[p].workspaces[w].tabs[t].panes.iter_mut().for_each(Term::kill);
            }
            Item::Workspace(id) => {
                let (p, w) = self.usable_workspace(id)?;
                if close.unlock && !close.remove_worktree {
                    return Err(malformed("`unlock` goes with `remove_worktree`"));
                }
                if close.remove_worktree {
                    self.removable(id)?;
                    let key = self.requests.key();
                    self.spawn_check(p, w, Some(key));
                    let stage = Stage::Removing { force: close.force, unlock: close.unlock };
                    let pending = Pending { client: Some(client), key, timeout: None, done: Done::default(), stage };
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
                self.projects.project_index(id).ok_or_else(|| none("project", id))?;
                self.close_project(id);
            }
            Item::Group(id) => {
                return Err(format!(
                    "group {id} cannot be closed: delete it with `cornercase group delete {id}`, and --close-projects to close its projects too"
                )
                .into());
            }
        }
        Ok(Some(json(&Done::default())))
    }

    fn removable(&self, id: u64) -> Result<(usize, usize), Refusal> {
        let (p, w) = self.usable_workspace(id)?;
        if !self.projects[p].workspaces[w].worktree {
            return Err(format!("workspace {id} is not a git worktree").into());
        }
        if let Some(refusal) = self.role_owner(p, w) {
            return Err(refusal.into());
        }
        Ok((p, w))
    }

    pub(super) fn removal_check_request(&mut self, client: u64, check: &RemovalCheck) -> Handled {
        let id = check.workspace.ok_or_else(|| malformed("`removal-check` needs the workspace: pass its id"))?;
        let (p, w) = self.removable(id)?;
        let key = self.requests.key();
        self.spawn_check(p, w, Some(key));
        let pending = Pending { client: Some(client), key, timeout: None, done: Done::default(), stage: Stage::Check };
        self.requests.pending.push(pending);
        Ok(None)
    }

    fn removal(&self, p: usize, w: usize, status: &worktree::Status) -> Removal {
        let workspace = &self.projects[p].workspaces[w];
        let lock = status.lock.as_ref().map(|lock| {
            let (role, path) = match &lock.role {
                Some((name, path)) => (Some(name.clone()), path.clone()),
                None => (None, workspace.path.clone()),
            };
            let (pid, running) = match lock.holder {
                Some(worktree::Holder::Running(pid)) => (Some(pid), Some(true)),
                Some(worktree::Holder::Gone(pid)) => (Some(pid), Some(false)),
                None => (None, None),
            };
            Locked { reason: lock.reason.clone(), path, role, pid, running }
        });
        let roles = workspace.role_worktrees.iter().filter(|tree| tree.path.is_dir());
        Removal {
            workspace: workspace.id,
            path: workspace.path.clone(),
            branch: git::branch(&workspace.path),
            changed: status.changed,
            lock,
            roles: roles
                .map(|tree| RoleTree { role: tree.role.clone(), path: tree.path.clone(), branch: tree.branch.clone() })
                .collect(),
            teardown: self.teardown_script(p).is_some(),
        }
    }

    pub(in crate::app) fn removal_checked(
        &mut self,
        key: u64,
        project: u64,
        workspace: u64,
        status: &worktree::Status,
    ) {
        let Some(stage) = self.requests.pending.iter().find(|p| p.key == key).map(|p| &p.stage) else { return };
        let (force, unlock, check) = match stage {
            Stage::Removing { force, unlock } => (*force, *unlock, false),
            Stage::Check => (false, false, true),
            _ => return,
        };
        if check {
            let Some(pending) = self.requests.take(key) else { return };
            let answer = match self.projects.workspace_in(project, workspace) {
                Some((p, w)) => self.usable_workspace(workspace).map(|_| json(&self.removal(p, w, status))),
                None => Err(none("workspace", workspace)),
            };
            self.refuse_or_answer(pending.client, answer);
            return;
        }
        let clear = (status.lock.is_none() || unlock) && (force || !status.changed);
        if clear && self.start_removal(project, workspace, force, unlock, Some(key)) {
            return;
        }
        let Some(pending) = self.requests.take(key) else { return };
        let refusal = match &status.lock {
            Some(lock) if !unlock => self.locked(workspace, lock).into(),
            _ if status.changed && !force => {
                format!("workspace {workspace} has changes that are not committed; add --force to remove it anyway")
                    .into()
            }
            _ => self.usable_workspace(workspace).err().unwrap_or_else(|| none("workspace", workspace)),
        };
        self.refuse_or_answer(pending.client, Err(refusal));
    }

    fn locked(&self, workspace: u64, lock: &worktree::Lock) -> String {
        let path = self
            .projects
            .workspace_index(workspace)
            .map(|(p, w)| self.projects[p].workspaces[w].path.display().to_string())
            .unwrap_or_default();
        let reason = if lock.reason.is_empty() { String::new() } else { format!(" ({})", lock.reason) };
        match &lock.role {
            Some((name, role)) => format!(
                "the worktree of `{name}` in workspace {workspace} is locked{reason}; add --unlock to unlock it and remove the workspace, or unlock it yourself with `git worktree unlock {}`",
                shell_quote(&role.display().to_string())
            ),
            None => format!(
                "workspace {workspace} is locked{reason}; add --unlock to unlock and remove it, or unlock it yourself with `git worktree unlock {}`",
                shell_quote(&path)
            ),
        }
    }

    pub(in crate::app) fn worktree_gone(
        &mut self,
        key: u64,
        project: u64,
        workspace: u64,
        result: error::Result<Option<String>>,
    ) {
        let pending = self.requests.take(key);
        self.settle_removal(project, workspace, result.is_ok());
        let Some(pending) = pending else { return };
        let answer = result.map(|warning| json(&Done { warning, ..Done::default() }));
        self.answer(pending.client, answer.map_err(|e| e.to_string()));
    }
}

#[cfg(test)]
mod tests;
