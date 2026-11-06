use std::path::{Path, PathBuf};

use super::super::close::shell_quote;
use super::super::{Handled, Pending, Stage, json, malformed};
use crate::app::App;
use crate::app::event::AppEvent;
use crate::app::worktrees::{RemovalStep, RemoveOptions, WorktreeEvent};
use crate::app::worktrees_dialog::{BEING_REMOVED, How, Tally, UNCHECKED, stake};
use crate::control::arrange::RoleTree;
use crate::control::contract::{Code, Refusal};
use crate::control::worktrees::kind_name;
use crate::control::worktrees::remove::{FAILED, FORCE, FolderRemoved, REFUSED, REMOVABLE, REMOVED, UNLOCK};
use crate::control::{Done, WorktreeInfo, WorktreesRemove, WorktreesRemoved};
use crate::log::{Job, Level};
use crate::os::fs::canonical;
use crate::project::Hierarchy;
use crate::worktrees::remove::{Named, named, still_listed};
use crate::worktrees::{Found, Kind, Row, fill};

const NO_LONGER_LISTED: &str = "it is no longer listed";

const FOLDER_STAYS: &str = "its folder is still there with its files: only git's entry went, so delete it yourself";

const OPEN_LEFTOVER: &str = "git no longer knows this folder, so it could not remove it, and its workspace is open: \
    close its project or the workspace, then remove the folder";

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

    fn goes(&self) -> bool {
        self.waits() || self.answer.outcome == REMOVABLE
    }

    fn carries(&self, path: &Path) -> bool {
        self.answer.roles.iter().any(|tree| canonical(&tree.path) == path)
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
        let going: Vec<bool> = self.folders.iter().map(Folder::goes).collect();
        let carriers = |folders: &[Folder], at: usize| -> Vec<usize> {
            let carries = |i: &usize| *i != at && going[*i] && folders[*i].carries(&folders[at].answer.path);
            (0..folders.len()).filter(carries).collect()
        };
        let carried: Vec<bool> = (0..self.folders.len()).map(|at| !carriers(&self.folders, at).is_empty()).collect();
        for at in 0..self.folders.len() {
            let lead = carriers(&self.folders, at).into_iter().find(|lead| !carried[*lead]);
            match lead.filter(|_| self.folders[at].answer.worktree.is_some()) {
                Some(lead) => self.follow_its_lead(at, lead),
                None => self.follow_the_first(at),
            }
        }
    }

    fn follow_its_lead(&mut self, at: usize, lead: usize) {
        let path = self.folders[lead].answer.path.clone();
        let outcome = if self.request.check { REMOVABLE } else { "" };
        let answer = &mut self.folders[at].answer;
        (answer.outcome, answer.why, answer.removed_with) = (outcome.to_string(), None, Some(path));
        self.folders[at].follows = Some(lead);
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

fn ended(answer: &mut FolderRemoved, tally: Tally) {
    answer.branches = tally.deleted;
    answer.kept = tally.notes;
    if let Some(why) = tally.error {
        (answer.outcome, answer.why) = (FAILED.to_string(), Some(why));
    } else {
        (answer.outcome, answer.warnings) = (REMOVED.to_string(), tally.failed);
        answer.warnings.extend(folder_stays(answer));
    }
}

fn folder_stays(answer: &FolderRemoved) -> Option<String> {
    let prunable = answer.worktree.as_ref().is_some_and(|row| row.kind == kind_name(Kind::Prunable));
    (prunable && answer.path.exists()).then(|| FOLDER_STAYS.to_string())
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
                let named = asked.iter().map(|path| named(path, &listing.open, &found)).collect();
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
        for folder in removing.folders.iter_mut().filter(|folder| folder.waits()) {
            if let Some(why) = self.holds_a_worktree(&folder.answer.path, &found) {
                folder.answer = refused(folder.answer.path.clone(), why);
            }
        }
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

    fn role_folders(&self, folder: &Path) -> Vec<PathBuf> {
        let Some((at, _)) = self.live(folder) else { return Vec::new() };
        self.projects.workspace(at).role_worktrees.iter().map(|tree| canonical(&tree.path)).collect()
    }

    fn holds_a_worktree(&self, folder: &Path, found: &[Found]) -> Option<String> {
        let mut going = self.role_folders(folder);
        going.push(folder.to_path_buf());
        let under = |row: &&Found| going.iter().any(|holder| row.path.starts_with(holder));
        let held = found.iter().filter(|row| row.kind != Kind::Prunable && !going.contains(&row.path)).find(under)?;
        Some(format!("the worktree {} is inside it and would go with it: remove that first", held.path.display()))
    }

    fn rows_to_look_at(&self, found: Vec<Found>, folders: &[Folder]) -> Vec<Row> {
        let mut wanted: Vec<PathBuf> =
            folders.iter().filter(|folder| folder.waits()).map(|folder| folder.answer.path.clone()).collect();
        let roles: Vec<PathBuf> = wanted.iter().flat_map(|path| self.role_folders(path)).collect();
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
        let cannot = self.cannot_go(row).or_else(|| busy_role(rows, &roles));
        let (outcome, why) = folder_verdict(cannot, request, &stakes);
        FolderRemoved {
            path: row.found.path.clone(),
            outcome,
            why,
            needs: stakes.needs(),
            lines: said.into_iter().filter(|said| !said.lost).map(|said| said.text).collect(),
            lost,
            unchecked: unchecked(rows, row, &roles),
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

    fn cannot_go(&self, row: &Row) -> Option<String> {
        if row.tickable() {
            let open_leftover = row.live.is_some() && matches!(row.found.kind, Kind::Orphan | Kind::Unknown);
            return open_leftover.then(|| OPEN_LEFTOVER.to_string());
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

fn folder_verdict(cannot: Option<String>, request: &WorktreesRemove, stakes: &Stakes) -> (String, Option<String>) {
    if let Some(why) = cannot {
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

fn role_row<'a>(rows: &'a [Row], tree: &RoleTree) -> Option<&'a Row> {
    let path = canonical(&tree.path);
    rows.iter().find(|row| row.found.path == path)
}

fn busy_role(rows: &[Row], roles: &[RoleTree]) -> Option<String> {
    roles.iter().find_map(|tree| {
        let role = role_row(rows, tree)?;
        if role.freeing {
            return Some(format!("the build folders of `{}` are being freed", tree.role));
        }
        let removing = role.removing || role.live.as_ref().is_some_and(|live| live.removing);
        removing.then(|| format!("the worktree of `{}` is already being removed or closed", tree.role))
    })
}

fn scan_error(row: &Row) -> Option<&str> {
    match &row.scan {
        Some(Err(error)) => Some(error),
        _ => None,
    }
}

fn unchecked(rows: &[Row], row: &Row, roles: &[RoleTree]) -> Vec<String> {
    let mut unread: Vec<String> = scan_error(row).map(str::to_string).into_iter().collect();
    for tree in roles {
        match role_row(rows, tree) {
            Some(role) => unread.extend(scan_error(role).map(|error| format!("role {}: {error}", tree.role))),
            None => unread.push(format!("role {}: {UNCHECKED}", tree.role)),
        }
    }
    unread
}

fn stakes(rows: &[Row], row: &Row, roles: &[RoleTree], lost: &[String]) -> Stakes {
    let mut stakes = Stakes { force: stake(row), locks: Vec::new() };
    stakes.locks.extend(locked(row, None));
    for tree in roles {
        let Some(role) = role_row(rows, tree) else {
            stakes.force.push(format!("role {}: {UNCHECKED}", tree.role));
            continue;
        };
        stakes.force.extend(stake(role).into_iter().map(|risk| format!("role {}: {risk}", tree.role)));
        stakes.locks.extend(locked(role, Some(&tree.role)));
    }
    stakes.force.extend_from_slice(lost);
    stakes
}
