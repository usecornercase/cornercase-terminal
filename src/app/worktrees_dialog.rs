use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use super::App;
use super::agent_watch::WATCH_AGENTS_EVERY;
use super::event::AppEvent;
use super::input::{ListPointer, list_pointer, wheel_span};
use super::layout::Side;
use super::overlay::Overlay;
use super::worktrees::WorktreeEvent;
use crate::error::Result;
use crate::git;
use crate::log::{Job, Level};
use crate::process;
use crate::project::search::Goto;
use crate::project::{Hierarchy, WorkspaceAt};
use crate::pull_request::State;
use crate::time;
use crate::ui;
use crate::worktrees::{
    Action, Dialog, Found, Hit, Kind, List, Live, Row, SCAN_GIVE_UP, SCANS_AT_ONCE, Scan, Step, discover, scan,
};

mod cleanup;
mod confirm;
mod free;
mod sizes;

pub(super) use cleanup::{Cleanup, Deleted};
pub(super) use confirm::wrap;
pub(super) use free::Frees;

pub(super) type Place = (PathBuf, u64, u64);

#[derive(Default)]
pub(super) struct Scans {
    generation: u64,
    listing: Option<Instant>,
    queue: VecDeque<Queued>,
    running: HashMap<u64, Running>,
    next: u64,
    outside: Outside,
    extra: Vec<PathBuf>,
    sent: usize,
    places: Vec<Place>,
    synced: Option<Instant>,
    sizes: sizes::Sizes,
}

struct Queued {
    path: PathBuf,
    repo: PathBuf,
    base: Option<String>,
    count: Option<String>,
    confirm: bool,
}

struct Running {
    path: PathBuf,
    confirm: bool,
    since: Instant,
    stale: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Outside {
    #[default]
    Idle,
    Running {
        ticket: u64,
        since: Instant,
        confirm: bool,
    },
    Failed,
}

impl App {
    pub(super) fn open_worktrees(&mut self, project: Option<u64>) {
        let only = match project {
            Some(id) => {
                let Some(p) = self.projects.project_index(id) else { return };
                let project = &self.projects[p];
                Some((project.id, self.project_label(project), project.path.clone()))
            }
            None => None,
        };
        let generation = self.restart_scans();
        self.spawn_worktree_listing(generation, only.as_ref().map(|(_, _, path)| path.clone()));
        let project = only.map(|(id, label, _)| (id, label));
        let dialog = Dialog::new(project, generation, List::new(Vec::new()));
        self.overlay = Some(Overlay::Worktrees(Box::new(dialog)));
    }

    fn restart_scans(&mut self) -> u64 {
        let generation = self.git.scans.generation + 1;
        let places = std::mem::take(&mut self.git.scans.places);
        let running = std::mem::take(&mut self.git.scans.running)
            .into_iter()
            .map(|(ticket, running)| (ticket, Running { stale: true, ..running }))
            .collect();
        self.git.scans = Scans {
            generation,
            next: self.git.scans.next,
            listing: Some(Instant::now()),
            places,
            running,
            ..Scans::default()
        };
        generation
    }

    fn spawn_worktree_listing(&mut self, generation: u64, keep: Option<PathBuf>) {
        let repos: Vec<PathBuf> = match &keep {
            Some(path) => vec![path.clone()],
            None => self.projects.iter().filter(|p| git::is_repo_root(&p.path)).map(|p| p.path.clone()).collect(),
        };
        let open: Vec<PathBuf> = self.projects.iter().map(|p| p.path.clone()).collect();
        let dir = self.config.worktrees_dir(self.home.as_deref());
        let job = Job::new(Level::Info, "worktrees", "list").with("repos", repos.len()).with("dir", dir.display());
        self.spawn_job(
            job,
            move || {
                let open: HashSet<PathBuf> = open.iter().map(|p| canonical(p)).collect();
                let keep = keep.map(|path| discover::main_checkout(&canonical(&path)));
                let found = discover::discover(&repos, &dir);
                Ok(found
                    .into_iter()
                    .filter(|f| !open.contains(&f.path) && keep.as_ref().is_none_or(|repo| &f.repo == repo))
                    .collect::<Vec<Found>>())
            },
            move |found, job| {
                let found = found.unwrap_or_default();
                job.note("found", found.len());
                Some(AppEvent::Worktree(WorktreeEvent::Listed { generation, found }))
            },
        );
    }

    fn worktrees_dialog(&mut self, generation: u64) -> Option<&mut Dialog> {
        match &mut self.overlay {
            Some(Overlay::Worktrees(dialog)) if dialog.generation == generation => Some(dialog),
            _ => None,
        }
    }

    pub(super) fn worktrees_listed(&mut self, generation: u64, found: Vec<Found>) {
        if self.worktrees_dialog(generation).is_none() {
            return;
        }
        self.refresh_places();
        let rows = self.fresh_rows(found);
        let queued: Vec<Queued> = rows
            .iter()
            .filter(|row| row.found.kind == Kind::Linked)
            .map(|row| Queued {
                path: row.found.path.clone(),
                repo: row.found.repo.clone(),
                base: self.base_of(&row.found.path),
                count: None,
                confirm: false,
            })
            .collect();
        self.git.scans.listing = None;
        self.git.scans.queue.extend(queued);
        self.queue_sizes(&rows);
        if let Some(dialog) = self.worktrees_dialog(generation) {
            let mut list = List::new(rows);
            list.filter = std::mem::take(&mut dialog.list.filter);
            list.order_by(dialog.list.order);
            *dialog = Dialog::new(dialog.project.take(), generation, list);
        }
        self.spawn_outside(false);
        self.pump_worktree_scans();
        self.pump_sizes();
    }

    fn fresh_rows(&self, found: Vec<Found>) -> Vec<Row> {
        found
            .into_iter()
            .map(|found| Row {
                live: self.live(&found.path).map(|(_, live)| live),
                removing: self.cleaning_path(&found.path),
                found,
                scan: None,
                size: None,
                freeing: false,
                outside: Vec::new(),
                elsewhere: Vec::new(),
                ticked: false,
                gone: false,
            })
            .collect()
    }

    fn base_of(&self, path: &Path) -> Option<String> {
        let (at, _) = self.live(path)?;
        self.projects.workspace(at).base.clone()
    }

    fn refresh_places(&mut self) {
        let old = std::mem::take(&mut self.git.scans.places);
        let mut places = Vec::new();
        for project in &self.projects {
            for workspace in &project.workspaces {
                let known = old.iter().find(|(_, _, id)| *id == workspace.id).map(|(path, ..)| path.clone());
                let path = known.unwrap_or_else(|| canonical(&workspace.path));
                places.push((path, project.id, workspace.id));
            }
        }
        self.git.scans.places = places;
    }

    pub(super) fn live(&self, path: &Path) -> Option<(WorkspaceAt, Live)> {
        let (_, project, workspace) = self.git.scans.places.iter().find(|(at, ..)| at == path)?;
        let at = self.projects.workspace_in(*project, *workspace)?;
        let (project, workspace) = (&self.projects[at.project()], self.projects.workspace(at));
        let running =
            workspace.terms().filter(|t| !t.shell_in_foreground()).filter_map(|t| t.program(&self.config)).collect();
        let live = Live {
            project: project.id,
            workspace: workspace.id,
            running,
            removing: !workspace.open() || project.closing,
            role_of_run: self.role_owner(at).is_some(),
            run_roles: workspace.role_worktrees.len(),
            pull_merged: self.pull_request(workspace.id).is_some_and(|pull| pull.state == State::Merged),
        };
        Some((at, live))
    }

    fn sync_worktree_rows(&mut self) {
        if !matches!(self.overlay, Some(Overlay::Worktrees(_))) {
            return;
        }
        self.refresh_places();
        let Some(Overlay::Worktrees(dialog)) = &self.overlay else { return };
        let lives: Vec<(bool, Option<Live>)> = dialog
            .list
            .rows
            .iter()
            .map(|row| match self.live(&row.found.path) {
                Some((at, live)) => (!self.projects.workspace(at).worktree, Some(live)),
                None => (false, None),
            })
            .collect();
        let Some(Overlay::Worktrees(dialog)) = &mut self.overlay else { return };
        for (row, (own, live)) in dialog.list.rows.iter_mut().zip(lives) {
            if own || (row.live.is_some() && live.is_none() && !row.found.path.exists()) {
                row.gone = true;
                row.ticked = false;
            }
            row.live = live;
        }
    }

    pub(super) fn watch_worktrees_dialog(&mut self, now: Instant) {
        if !matches!(self.overlay, Some(Overlay::Worktrees(_))) {
            return;
        }
        if self.git.scans.synced.is_none_or(|at| now.saturating_duration_since(at) >= WATCH_AGENTS_EVERY) {
            self.git.scans.synced = Some(now);
            self.sync_worktree_rows();
        }
        self.git
            .scans
            .running
            .retain(|_, running| !running.stale || now.saturating_duration_since(running.since) < SCAN_GIVE_UP);
        let late: Vec<u64> = self
            .git
            .scans
            .running
            .iter()
            .filter(|(_, running)| now.saturating_duration_since(running.since) >= SCAN_GIVE_UP)
            .map(|(ticket, _)| *ticket)
            .collect();
        for ticket in late {
            if let Some(running) = self.git.scans.running.remove(&ticket) {
                crate::log::warning!("worktrees", "gave up on a scan", path = running.path.display());
                self.apply_scan(&running.path, running.confirm, Err("gave up".to_string()));
            }
        }
        if let Outside::Running { since, .. } = self.git.scans.outside
            && now.saturating_duration_since(since) >= SCAN_GIVE_UP
        {
            self.git.scans.outside = Outside::Failed;
        }
        if self.git.scans.listing.is_some_and(|since| now.saturating_duration_since(since) >= SCAN_GIVE_UP) {
            self.git.scans.listing = None;
        }
        self.give_up_sizes(now);
        self.pump_worktree_scans();
    }

    fn pump_worktree_scans(&mut self) {
        while self.git.scans.running.len() < SCANS_AT_ONCE {
            let Some(queued) = self.git.scans.queue.pop_front() else { break };
            self.spawn_scan(queued);
        }
    }

    fn spawn_scan(&mut self, Queued { path, repo, base, count, confirm }: Queued) {
        let generation = self.git.scans.generation;
        let ticket = self.git.scans.next;
        self.git.scans.next += 1;
        let running = Running { path: path.clone(), confirm, since: Instant::now(), stale: false };
        self.git.scans.running.insert(ticket, running);
        let job = Job::new(Level::Debug, "worktrees", "scan").with("path", path.display()).with("confirm", confirm);
        let at = path.clone();
        self.spawn_job(
            job,
            move || {
                let base = base.or_else(|| crate::changes::git::default_base(&repo));
                match count {
                    Some(branch) => scan::branch_commits(&repo, &branch, base.as_deref()),
                    None => scan::scan(&at, base.as_deref()),
                }
            },
            move |scan, _| {
                let scan = Box::new(scan);
                Some(AppEvent::Worktree(WorktreeEvent::Scanned { generation, ticket, path, scan, confirm }))
            },
        );
    }

    pub(super) fn worktree_scanned(
        &mut self,
        (generation, ticket): (u64, u64),
        path: &Path,
        scan: Result<Scan>,
        confirm: bool,
    ) {
        let Some(running) = self.git.scans.running.remove(&ticket) else { return };
        if running.stale || self.worktrees_dialog(generation).is_none() {
            if self.worktrees_dialog(self.git.scans.generation).is_some() {
                self.pump_worktree_scans();
            }
            return;
        }
        self.apply_scan(path, confirm, scan.map_err(|e| e.to_string()));
        self.pump_worktree_scans();
    }

    fn apply_scan(&mut self, path: &Path, confirm: bool, scan: std::result::Result<Scan, String>) {
        let Some(Overlay::Worktrees(dialog)) = &mut self.overlay else { return };
        if confirm {
            dialog.rescanned(path, scan);
            self.git.scans.extra.retain(|p| p != path);
        } else {
            dialog.list.scanned(path, scan);
        }
        dialog.list.sort_when_done();
    }

    fn spawn_outside(&mut self, confirm: bool) {
        let Some(Overlay::Worktrees(dialog)) = &self.overlay else { return };
        let paths: Vec<PathBuf> = dialog
            .list
            .rows
            .iter()
            .filter(|row| row.found.kind != Kind::Prunable && !row.gone)
            .map(|row| row.found.path.clone())
            .collect();
        let shells = self.pane_shells(&paths);
        let (generation, ticket) = (self.git.scans.generation, self.git.scans.next);
        self.git.scans.next += 1;
        self.git.scans.outside = Outside::Running { ticket, since: Instant::now(), confirm };
        let job = Job::new(Level::Debug, "worktrees", "outside").with("paths", paths.len());
        self.spawn_job(
            job,
            move || {
                let panes = process::owners::trees(&shells);
                let server = i32::try_from(std::process::id()).unwrap_or_default();
                let found = scan::outside(&paths, &panes, server);
                Ok(found.into_iter().map(|(at, name, tab)| (paths[at].clone(), name, tab)).collect::<Vec<_>>())
            },
            move |found, _| Some(AppEvent::Worktree(WorktreeEvent::Outside { generation, ticket, found })),
        );
    }

    fn pane_shells(&self, paths: &[PathBuf]) -> Vec<(i32, Vec<usize>)> {
        let index = |path: &Path| paths.iter().position(|p| p == path);
        let mut shells = Vec::new();
        for project in &self.projects {
            for workspace in &project.workspaces {
                let own = self.git.scans.places.iter().find(|(_, p, w)| *p == project.id && *w == workspace.id);
                let mut owns: Vec<usize> = own.and_then(|(path, ..)| index(path)).into_iter().collect();
                owns.extend(workspace.role_worktrees.iter().filter_map(|tree| index(&canonical(&tree.path))));
                shells
                    .extend(workspace.terms().filter_map(crate::term::Term::shell_pid).map(|pid| (pid, owns.clone())));
            }
        }
        shells
    }

    pub(super) fn outside_found(
        &mut self,
        (generation, ticket): (u64, u64),
        found: Result<Vec<(PathBuf, String, bool)>>,
    ) {
        if !matches!(self.git.scans.outside, Outside::Running { ticket: t, .. } if t == ticket) {
            return;
        }
        let Some(dialog) = self.worktrees_dialog(generation) else { return };
        let failed = found.is_err();
        let found = found.unwrap_or_default();
        for row in &mut dialog.list.rows {
            let of = |tab: bool| -> Vec<String> {
                found.iter().filter(|(path, _, t)| *path == row.found.path && *t == tab).map(|f| f.1.clone()).collect()
            };
            (row.outside, row.elsewhere) = (of(false), of(true));
        }
        self.git.scans.outside = if failed { Outside::Failed } else { Outside::Idle };
    }

    pub(super) fn worktrees_key(&mut self, key: KeyEvent, area: Rect) {
        self.sync_worktree_rows();
        let compact = area.width < ui::COMPACT_WIDTH;
        let (rows, _) = ui::worktrees_room(area, compact);
        let Some(Overlay::Worktrees(dialog)) = &mut self.overlay else { return };
        let action = dialog.key(key);
        dialog.reveal(rows);
        self.worktrees_act(action);
    }

    pub(super) fn worktrees_paste(&mut self, text: &str) {
        if let Some(Overlay::Worktrees(dialog)) = &mut self.overlay {
            dialog.type_filter(text);
        }
    }

    pub(super) fn worktrees_mouse(&mut self, ev: MouseEvent, pos: Position, area: Rect) {
        let Some(pointer) = list_pointer(ev.kind) else { return };
        self.sync_worktree_rows();
        let Some(Overlay::Worktrees(dialog)) = &self.overlay else { return };
        let view = self.worktrees_view(dialog, area);
        let shown = dialog.list.shown();
        if let ListPointer::Wheel(delta) = pointer {
            let lines = view.confirm.as_ref().map(|confirm| confirm.lines.len());
            let (rows, lines) = wheel_span(ui::worktrees_room(area, view.compact), lines);
            if let Some(Overlay::Worktrees(dialog)) = &mut self.overlay {
                dialog.scroll_by(delta, rows, lines);
            }
            return;
        }
        let row = |i: usize| shown.get(i).copied();
        let hit = match ui::worktrees_hit(area, &view, pos) {
            Some(ui::WorktreesHit::Tick(i)) => row(i).map(Hit::Tick),
            Some(ui::WorktreesHit::Row(i)) => row(i).map(Hit::Row),
            Some(ui::WorktreesHit::Changes(i)) => row(i).map(Hit::Changes),
            Some(ui::WorktreesHit::Free(i)) => row(i).map(Hit::Free),
            Some(ui::WorktreesHit::FreeTicked) => Some(Hit::FreeTicked),
            Some(ui::WorktreesHit::Order(order)) => Some(Hit::Order(order)),
            Some(ui::WorktreesHit::Remove) => Some(Hit::Remove),
            Some(ui::WorktreesHit::Done) => Some(Hit::Done),
            Some(ui::WorktreesHit::Back) => Some(Hit::Back),
            Some(ui::WorktreesHit::Branches) => Some(Hit::Branches),
            Some(ui::WorktreesHit::Filter) | None => None,
        };
        let Some(hit) = hit else { return };
        let Some(Overlay::Worktrees(dialog)) = &mut self.overlay else { return };
        let action = dialog.click(hit);
        self.worktrees_act(action);
    }

    fn worktrees_act(&mut self, action: Action) {
        match action {
            Action::None | Action::Toggle(_) | Action::Back => {}
            Action::Close => self.overlay = None,
            Action::Rescan(rows) => self.rescan_worktrees(&rows),
            Action::Remove { branches } => self.remove_worktrees(branches),
            Action::Free => self.spawn_outside(true),
            Action::FreeBuilds => self.free_confirmed(),
            Action::Changes(row) => self.worktree_changes(row),
        }
    }

    fn rescan_worktrees(&mut self, rows: &[usize]) {
        self.git.scans.extra.clear();
        if let Outside::Running { confirm, .. } = &mut self.git.scans.outside {
            *confirm = false;
        }
        let Some(Overlay::Worktrees(dialog)) = &self.overlay else { return };
        let ticked = dialog.list.ticked();
        let roles: Vec<PathBuf> = ticked
            .iter()
            .filter_map(|&i| self.live(&dialog.list.rows[i].found.path))
            .flat_map(|(at, _)| self.projects.workspace(at).role_worktrees.iter().map(|tree| canonical(&tree.path)))
            .filter(|path| dialog.list.rows.iter().any(|row| &row.found.path == path && row.checked_on_confirm()))
            .collect();
        let mut targets: Vec<&Row> = rows.iter().map(|&i| &dialog.list.rows[i]).collect();
        targets.extend(roles.iter().filter_map(|path| dialog.list.rows.iter().find(|row| &row.found.path == path)));
        let targets: Vec<Queued> = targets
            .into_iter()
            .map(|row| Queued {
                path: row.found.path.clone(),
                repo: row.found.repo.clone(),
                base: self.base_of(&row.found.path),
                count: (row.found.kind == Kind::Prunable).then(|| row.found.branch.clone()).flatten(),
                confirm: true,
            })
            .collect();
        self.git.scans.sent = targets.len();
        for queued in targets.into_iter().rev() {
            self.git.scans.queue.retain(|q| q.path != queued.path);
            for running in self.git.scans.running.values_mut().filter(|running| running.path == queued.path) {
                running.stale = true;
            }
            self.git.scans.queue.push_front(queued);
        }
        self.git.scans.extra = roles;
        self.spawn_outside(true);
        self.pump_worktree_scans();
    }

    fn worktrees_ready(&self, dialog: &Dialog) -> bool {
        dialog.can_remove() && !self.confirm_checking()
    }

    fn confirm_checking(&self) -> bool {
        !self.git.scans.extra.is_empty() || matches!(self.git.scans.outside, Outside::Running { confirm: true, .. })
    }

    fn worktree_changes(&mut self, row: usize) {
        let Some(Overlay::Worktrees(dialog)) = &self.overlay else { return };
        let Some(found) = dialog.list.rows.get(row).map(|row| row.found.clone()) else { return };
        let Some((_, live)) = self.live(&found.path).filter(|(_, live)| !live.removing) else { return };
        if found.kind != Kind::Linked {
            return;
        }
        self.overlay = None;
        self.goto(Goto::Place { project: live.project, workspace: Some(live.workspace), tab: None });
        self.show_side(Some(Side::Changes));
    }

    pub(super) fn worktrees_view(&self, dialog: &Dialog, area: Rect) -> Box<ui::Worktrees> {
        let now = SystemTime::now();
        let rows: Vec<ui::WorktreeRow> =
            dialog.list.shown().iter().map(|&i| self.worktree_row(&dialog.list.rows[i], now)).collect();
        let ticked = dialog.list.rows.iter().filter(|row| row.ticked && row.tickable()).count();
        let title = match &dialog.project {
            Some((_, label)) => format!("Worktrees › {label}"),
            None => "Worktrees".to_string(),
        };
        let confirm = match &dialog.step {
            Step::List => None,
            Step::Confirm { .. } => Some(self.confirm_view(dialog, area)),
            Step::Free { paths } => Some(self.free_view(dialog, area, paths)),
        };
        let free = dialog.list.rows.iter().filter(|row| row.ticked && row.can_free()).count();
        let total = Self::total_text(dialog, now);
        let note = self.worktrees_note(dialog, confirm.as_ref(), area);
        Box::new(ui::Worktrees {
            title,
            filter: dialog.list.filter.clone(),
            rows,
            selected: dialog.selected,
            scroll: dialog.scroll,
            ticked,
            free,
            order: dialog.list.order,
            total,
            note,
            confirm,
            compact: area.width < ui::COMPACT_WIDTH,
        })
    }

    fn worktrees_note(&self, dialog: &Dialog, confirm: Option<&ui::ConfirmView>, area: Rect) -> Option<ui::Note> {
        if self.git.scans.listing.is_some() {
            return Some(ui::Note::Busy("listing worktrees…"));
        }
        let Some(confirm) = confirm else {
            let total = dialog.list.rows.iter().filter(|row| row.found.kind == Kind::Linked).count();
            let pending = dialog.list.pending();
            if pending > 0 {
                return Some(ui::Note::Info(format!("checking {} of {total}…", total - pending)));
            }
            return Self::sizes_note(dialog).map(ui::Note::Info);
        };
        if confirm.checking {
            let waiting = dialog.checking() + self.git.scans.extra.len();
            let total = self.git.scans.sent.max(waiting);
            return Some(match waiting {
                0 => ui::Note::Busy("checking processes…"),
                _ => ui::Note::Info(format!("checking {} of {total}…", total - waiting)),
            });
        }
        if matches!(self.git.scans.outside, Outside::Failed) {
            return Some(ui::Note::Error("could not look for processes outside cornercase".into()));
        }
        let (_, body) = ui::worktrees_room(area, area.width < ui::COMPACT_WIDTH);
        (confirm.scroll + usize::from(body.height) < confirm.lines.len())
            .then(|| ui::Note::Info("↓ scroll for more".into()))
    }

    fn worktree_row(&self, row: &Row, now: SystemTime) -> ui::WorktreeRow {
        let live = row.live.as_ref();
        let closing = live
            .and_then(|l| self.projects.workspace_in(l.project, l.workspace))
            .is_some_and(|at| !self.projects.workspace(at).removing());
        let removing = row.removing || live.is_some_and(|l| l.removing);
        let status = if row.freeing {
            Some("freeing…")
        } else if removing {
            Some(if closing && !row.removing { "closing…" } else { "removing…" })
        } else {
            live.is_some_and(|l| l.role_of_run).then_some("owned by a flow run")
        };
        let folder = row.found.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        ui::WorktreeRow {
            ticked: row.ticked,
            tickable: row.tickable(),
            project: row.found.project.clone(),
            branch: row.branch().map_or(folder, str::to_string),
            folder: crate::text::display_path(&row.found.path, self.home.as_deref()),
            age: row_age(row, now),
            marks: self.worktree_marks(row),
            changes: live.is_some() && !removing && row.found.kind == Kind::Linked,
            free: row.can_free() && !removing,
            size: row_size(row),
            builds: row
                .builds_bytes()
                .filter(|bytes| *bytes > 0)
                .map(|bytes| format!("{} builds", crate::text::size(bytes))),
            status: status.map(String::from),
        }
    }

    fn worktree_marks(&self, row: &Row) -> Vec<(String, Color)> {
        let muted = ui::muted(self.themes.host());
        let mut marks: Vec<(String, Color)> = kind_mark(row.found.kind, muted).into_iter().collect();
        match &row.scan {
            Some(Ok(scan)) => marks.extend(scan_marks(row, scan, muted)),
            Some(Err(_)) => marks.push(("?".into(), Color::Red)),
            None => {}
        }
        let live = row.live.as_ref();
        match live.map_or(&[][..], |l| l.running.as_slice()) {
            [] => {}
            [one] => marks.push((one.clone(), Color::Yellow)),
            many => marks.push((format!("{} running", many.len()), Color::Yellow)),
        }
        let elsewhere = row.outside.len() + row.elsewhere.len();
        if elsewhere > 0 {
            marks.push((format!("{elsewhere} outside"), Color::Yellow));
        }
        if live.is_some() {
            marks.push(("open".into(), ui::soft(self.themes.host())));
        }
        marks
    }
}

fn kind_mark(kind: Kind, muted: Color) -> Option<(String, Color)> {
    match kind {
        Kind::Prunable => Some(("prunable".into(), muted)),
        Kind::Orphan => Some(("repository gone".into(), Color::Red)),
        Kind::Unknown => Some(("unknown to git".into(), Color::Red)),
        Kind::Linked => None,
    }
}

fn scan_marks(row: &Row, scan: &Scan, muted: Color) -> Vec<(String, Color)> {
    let mut marks = Vec::new();
    if scan.changed > 0 {
        marks.push((format!("●{}", scan.changed), Color::Yellow));
    }
    if scan.unpushed > 0 {
        marks.push((format!("↑{}", scan.unpushed), Color::Cyan));
    }
    if row.merged() {
        marks.push(("merged".into(), Color::Indexed(99)));
    } else if row.no_commits() {
        marks.push(("no commits".into(), muted));
    }
    if scan.upstream_gone {
        marks.push(("upstream gone".into(), Color::Indexed(214)));
    }
    if scan.lock.is_some() {
        marks.push(("locked".into(), Color::Yellow));
    }
    marks
}

fn row_size(row: &Row) -> String {
    match &row.size {
        _ if !row.has_folder() => String::new(),
        Some(Ok(size)) => crate::text::size(size.bytes),
        Some(Err(_)) => "?".into(),
        None => "…".into(),
    }
}

fn row_age(row: &Row, now: SystemTime) -> String {
    match &row.scan {
        _ if row.found.kind != Kind::Linked => String::new(),
        Some(Ok(scan)) => {
            scan.last_changed.map_or_else(|| "?".into(), |at| time::ago(now.duration_since(at).unwrap_or_default()))
        }
        Some(Err(_)) => "?".into(),
        None => "…".into(),
    }
}

#[cfg(test)]
impl App {
    pub(super) fn worktree_outside_running(&self) -> bool {
        matches!(self.git.scans.outside, Outside::Running { .. })
    }

    pub(super) fn worktree_scans(&self) -> (usize, usize) {
        let running = &self.git.scans.running;
        (running.len(), running.values().filter(|r| r.confirm && !r.stale).count())
    }
}

pub(super) fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
