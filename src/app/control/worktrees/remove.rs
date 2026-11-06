use std::path::{Path, PathBuf};

use super::super::close::shell_quote;
use super::super::{Handled, Pending, Stage, json, malformed};
use crate::app::App;
use crate::app::event::AppEvent;
use crate::app::worktrees::{RemovalStep, RemoveOptions, WorktreeEvent};
use crate::app::worktrees_dialog::{BEING_REMOVED, How, Tally, UNCHECKED, stake};
use crate::control::arrange::RoleTree;
use crate::control::contract::{Code, Refusal};
use crate::control::worktrees::remove::{FAILED, FORCE, FolderRemoved, REFUSED, REMOVABLE, REMOVED, UNLOCK};
use crate::control::{Done, WorktreeInfo, WorktreesRemove, WorktreesRemoved};
use crate::log::{Job, Level};
use crate::os::fs::canonical;
use crate::project::Hierarchy;
use crate::worktrees::remove::{Named, named, still_listed};
use crate::worktrees::{Found, Row, fill};

const NO_LONGER_LISTED: &str = "it is no longer listed";

pub(in crate::app::control) struct Removing {
    request: WorktreesRemove,
    folders: Vec<Folder>,
}

struct Folder {
    follows: Option<usize>,
    answer: FolderRemoved,
}

struct Stakes {
    force: Vec<String>,
    locks: Vec<String>,
}

impl Folder {
    fn of(named: Named) -> Self {
        let answer = match named {
            Named::Listed(path) => FolderRemoved { path, ..FolderRemoved::default() },
            Named::Unknown { path, why } => refused(path, why),
        };
        Self { follows: None, answer }
    }

    fn waits(&self) -> bool {
        self.answer.outcome.is_empty()
    }
}

impl Removing {
    fn ended(&self) -> bool {
        !self.folders.iter().any(Folder::waits)
    }

    fn answer(&self) -> WorktreesRemoved {
        let folders = self.folders.iter().map(|folder| folder.answer.clone()).collect();
        WorktreesRemoved { check: self.request.check, folders }
    }

    fn link_followers(&mut self) {
        for at in 0..self.folders.len() {
            let Some(lead) = lead_of(&self.folders, at) else {
                self.follow_the_first(at);
                continue;
            };
            let path = self.folders[lead].answer.path.clone();
            let outcome = if self.request.check { REMOVABLE } else { "" };
            let answer = &mut self.folders[at].answer;
            (answer.outcome, answer.why, answer.removed_with) = (outcome.to_string(), None, Some(path));
            self.folders[at].follows = Some(lead);
        }
    }

    fn follow_the_first(&mut self, at: usize) {
        let path = &self.folders[at].answer.path;
        let first = self.folders[..at].iter().position(|earlier| earlier.answer.path == *path);
        if self.folders[at].waits() {
            self.folders[at].follows = first;
        }
    }

    fn settle(&mut self, at: usize, tally: Tally) {
        let Some(folder) = self.folders.get_mut(at) else { return };
        ended(&mut folder.answer, tally);
        self.tell_followers(at);
    }

    fn tell_followers(&mut self, at: usize) {
        let lead = self.folders[at].answer.clone();
        for folder in self.folders.iter_mut().filter(|folder| folder.follows == Some(at)) {
            follow(&mut folder.answer, &lead);
        }
    }
}

impl Stakes {
    fn needs(&self) -> Vec<String> {
        let force = (!self.force.is_empty()).then(|| FORCE.to_string());
        let unlock = (!self.locks.is_empty()).then(|| UNLOCK.to_string());
        force.into_iter().chain(unlock).collect()
    }

    fn refusal(&self, request: &WorktreesRemove) -> Option<String> {
        let mut parts = Vec::new();
        if !request.force && !self.force.is_empty() {
            parts.push(format!("{}; add --force to remove it anyway", self.force.join(", ")));
        }
        if !request.unlock {
            parts.extend(self.locks.iter().cloned());
        }
        (!parts.is_empty()).then(|| parts.join("; "))
    }
}

fn refused(path: PathBuf, why: impl Into<String>) -> FolderRemoved {
    FolderRemoved { path, outcome: REFUSED.to_string(), why: Some(why.into()), ..FolderRemoved::default() }
}

fn lead_of(folders: &[Folder], at: usize) -> Option<usize> {
    let path = &folders[at].answer.path;
    let goes = |lead: &Folder| lead.waits() || lead.answer.outcome == REMOVABLE;
    let carries = |lead: &Folder| lead.answer.roles.iter().any(|tree| canonical(&tree.path) == *path);
    let listed = folders[at].answer.worktree.is_some();
    folders.iter().position(|lead| listed && goes(lead) && carries(lead))
}

fn ended(answer: &mut FolderRemoved, tally: Tally) {
    answer.branches = tally.deleted;
    answer.kept = tally.notes;
    if let Some(why) = tally.error {
        (answer.outcome, answer.why) = (FAILED.to_string(), Some(why));
    } else {
        (answer.outcome, answer.warnings) = (REMOVED.to_string(), tally.failed);
    }
}

fn follow(answer: &mut FolderRemoved, lead: &FolderRemoved) {
    if answer.path == lead.path {
        answer.clone_from(lead);
    } else if lead.outcome == REFUSED {
        (answer.outcome, answer.why) = (REFUSED.to_string(), lead.why.clone());
    } else if answer.path.exists() {
        let why = format!("it is still there after {} went", lead.path.display());
        (answer.outcome, answer.why) = (FAILED.to_string(), Some(why));
    } else {
        answer.outcome = REMOVED.to_string();
    }
}

fn locked(row: &Row, role: Option<&str>) -> Option<String> {
    let Some(Ok(scan)) = &row.scan else { return None };
    let lock = scan.lock.as_ref()?;
    let whose = role.map_or_else(|| "it".to_string(), |role| format!("the worktree of `{role}`"));
    let reason = if lock.reason.is_empty() { String::new() } else { format!(" ({})", lock.reason) };
    let path = shell_quote(&row.found.path.display().to_string());
    Some(format!(
        "{whose} is locked{reason}; add --unlock to unlock and remove it, or unlock it yourself with `git worktree unlock {path}`"
    ))
}

impl App {
    pub(in crate::app::control) fn worktrees_remove_request(
        &mut self,
        client: u64,
        request: &WorktreesRemove,
    ) -> Handled {
        if request.paths.is_empty() {
            return Err(malformed("`worktrees-remove` needs the folders: pass `paths`"));
        }
        let (key, listing, asked) = (self.clients.requests.key(), self.worktree_folders(None), request.paths.clone());
        let job = Job::new(Level::Info, "control", "worktrees remove: list").with("paths", asked.len());
        self.spawn_job(
            job,
            move || {
                let found = listing.found();
                let named = asked.iter().map(|path| named(path, &listing, &found)).collect();
                Ok((found, named))
            },
            move |listed, _| Some(removal_step(key, RemovalStep::Listed(listed))),
        );
        let removing = Removing { request: request.clone(), folders: Vec::new() };
        let stage = Stage::RemovingFolders(Box::new(removing));
        let pending = Pending { client: Some(client), key, timeout: None, done: Done::default(), stage };
        self.clients.requests.pending.push(pending);
        Ok(None)
    }

    pub(in crate::app) fn removal_step(&mut self, key: u64, step: RemovalStep) {
        let Some(pending) = self.clients.requests.take(key) else { return };
        match step {
            RemovalStep::Listed(Ok((found, named))) => self.removal_listed(pending, found, named),
            RemovalStep::Looked(Ok(rows)) => self.removal_looked(pending, rows),
            RemovalStep::Listed(Err(e)) | RemovalStep::Looked(Err(e)) => {
                self.refuse_or_answer(pending.client, Err(Refusal::new(Code::Failed, e.to_string())));
            }
        }
    }

    fn removal_listed(&mut self, mut pending: Pending, found: Vec<Found>, named: Vec<Named>) {
        let key = pending.key;
        let Stage::RemovingFolders(removing) = &mut pending.stage else { return };
        removing.folders = named.into_iter().map(Folder::of).collect();
        self.refresh_places();
        let rows = self.rows_to_look_at(found, &removing.folders);
        let asks = self.row_asks(&rows, false);
        let job = Job::new(Level::Debug, "control", "worktrees remove: look").with("rows", rows.len());
        self.spawn_job(
            job,
            move || Ok(fill(rows, asks)),
            move |rows, _| Some(removal_step(key, RemovalStep::Looked(rows))),
        );
        self.clients.requests.pending.push(pending);
    }

    fn rows_to_look_at(&self, found: Vec<Found>, folders: &[Folder]) -> Vec<Row> {
        let mut wanted: Vec<PathBuf> =
            folders.iter().filter(|folder| folder.waits()).map(|folder| folder.answer.path.clone()).collect();
        let roles: Vec<PathBuf> = wanted
            .iter()
            .filter_map(|path| self.live(path))
            .flat_map(|(at, _)| self.projects.workspace(at).role_worktrees.iter().map(|tree| canonical(&tree.path)))
            .collect();
        wanted.extend(roles);
        let mut rows = self.fresh_rows(found);
        rows.retain(|row| wanted.contains(&row.found.path));
        rows
    }

    fn removal_looked(&mut self, mut pending: Pending, mut rows: Vec<Row>) {
        self.refresh_places();
        for row in &mut rows {
            self.relive(row);
            row.freeing = self.freeing_path(&row.found.path);
        }
        let key = pending.key;
        let Stage::RemovingFolders(removing) = &mut pending.stage else { return };
        self.judge_folders(removing, &rows);
        removing.link_followers();
        if !removing.request.check {
            self.start_folders(key, removing, &rows);
        }
        if removing.ended() {
            let answer = json(&removing.answer());
            self.refuse_or_answer(pending.client, Ok(answer));
        } else {
            self.clients.requests.pending.push(pending);
        }
    }

    fn judge_folders(&self, removing: &mut Removing, rows: &[Row]) {
        let request = &removing.request;
        for folder in removing.folders.iter_mut().filter(|folder| folder.waits()) {
            folder.answer = match rows.iter().find(|row| row.found.path == folder.answer.path) {
                Some(row) => self.looked_at(rows, row, request),
                None => refused(folder.answer.path.clone(), NO_LONGER_LISTED),
            };
        }
    }

    fn looked_at(&self, rows: &[Row], row: &Row, request: &WorktreesRemove) -> FolderRemoved {
        let said = self.second_look(rows, row, request.delete_branch, false);
        let lost: Vec<String> = self
            .second_look(rows, row, true, false)
            .into_iter()
            .filter(|said| said.lost)
            .map(|said| said.text)
            .collect();
        let roles = self.roles_of(row);
        let stakes = stakes(rows, row, &roles, if request.delete_branch { &lost } else { &[] });
        let (outcome, why) = self.folder_verdict(row, request, &stakes);
        FolderRemoved {
            path: row.found.path.clone(),
            outcome,
            why,
            needs: stakes.needs(),
            lines: said.into_iter().filter(|said| !said.lost).map(|said| said.text).collect(),
            lost,
            teardown: self.teardown_runs(row),
            roles,
            worktree: Some(WorktreeInfo::of(row, self.risk_list(row, row.live.as_ref(), false))),
            ..FolderRemoved::default()
        }
    }

    fn roles_of(&self, row: &Row) -> Vec<RoleTree> {
        let at = row.live.as_ref().and_then(|live| self.projects.workspace_in(live.project, live.workspace));
        let trees = at.map_or(&[][..], |at| self.projects.workspace(at).role_worktrees.as_slice());
        let on_disk = trees.iter().filter(|tree| tree.path.is_dir());
        on_disk
            .map(|tree| RoleTree { role: tree.role.clone(), path: tree.path.clone(), branch: tree.branch.clone() })
            .collect()
    }

    fn folder_verdict(&self, row: &Row, request: &WorktreesRemove, stakes: &Stakes) -> (String, Option<String>) {
        if let Some(why) = self.cannot_tick(row) {
            return (REFUSED.to_string(), Some(why));
        }
        if request.check {
            return (REMOVABLE.to_string(), None);
        }
        match stakes.refusal(request) {
            Some(why) => (REFUSED.to_string(), Some(why)),
            None => (String::new(), None),
        }
    }

    fn cannot_tick(&self, row: &Row) -> Option<String> {
        if row.tickable() {
            return None;
        }
        if row.freeing {
            return Some("its build folders are being freed".to_string());
        }
        let live = self.live(&row.found.path).filter(|(_, live)| !row.removing && !live.removing);
        let owner = live.and_then(|(at, _)| self.role_owner(at));
        Some(owner.unwrap_or_else(|| BEING_REMOVED.to_string()))
    }

    fn start_folders(&mut self, key: u64, removing: &mut Removing, rows: &[Row]) {
        let options = RemoveOptions { force: removing.request.force, unlock: removing.request.unlock };
        let branches = removing.request.delete_branch;
        let mut going = Vec::new();
        for at in 0..removing.folders.len() {
            let folder = &mut removing.folders[at];
            if !folder.waits() || folder.follows.is_some() {
                continue;
            }
            let how = How { branches, options, request: Some((key, at)) };
            match self.start_folder(&folder.answer.path, rows, how) {
                Ok(paths) => going.extend(paths),
                Err(why) => {
                    folder.answer = refused(folder.answer.path.clone(), why);
                    removing.tell_followers(at);
                }
            }
        }
        self.worktree_rows_removing(&going);
    }

    fn start_folder(&mut self, path: &Path, rows: &[Row], how: How) -> Result<Vec<PathBuf>, String> {
        let Some(row) = rows.iter().find(|row| row.found.path == path) else {
            return Err(NO_LONGER_LISTED.to_string());
        };
        still_listed(path).map_err(|e| e.to_string())?;
        self.remove_one(row, rows, how).map_err(str::to_string)
    }

    pub(in crate::app) fn folder_cleaned(&mut self, key: u64, at: usize, tally: Tally) {
        let pending = self.clients.requests.pending.iter_mut().find(|pending| pending.key == key);
        let Some(Stage::RemovingFolders(removing)) = pending.map(|pending| &mut pending.stage) else { return };
        removing.settle(at, tally);
        if !removing.ended() {
            return;
        }
        let answer = json(&removing.answer());
        let Some(pending) = self.clients.requests.take(key) else { return };
        self.refuse_or_answer(pending.client, Ok(answer));
    }
}

fn removal_step(key: u64, step: RemovalStep) -> AppEvent {
    AppEvent::Worktree(WorktreeEvent::Removal { key, step: Box::new(step) })
}

fn stakes(rows: &[Row], row: &Row, roles: &[RoleTree], lost: &[String]) -> Stakes {
    let mut stakes = Stakes { force: stake(row), locks: Vec::new() };
    stakes.locks.extend(locked(row, None));
    for tree in roles {
        let path = canonical(&tree.path);
        let Some(role) = rows.iter().find(|row| row.found.path == path) else {
            stakes.force.push(format!("role {}: {UNCHECKED}", tree.role));
            continue;
        };
        stakes.force.extend(stake(role).into_iter().map(|risk| format!("role {}: {risk}", tree.role)));
        stakes.locks.extend(locked(role, Some(&tree.role)));
    }
    stakes.force.extend_from_slice(lost);
    stakes
}
