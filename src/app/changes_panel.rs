use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::control::{BaseChange, Replied};
use super::event::AppEvent;
use super::input::wheel;
use super::layout::Side;
use crate::changes::diff::File as ChangedFile;
use crate::changes::{self, Checkout, Mode, Tints, every};
use crate::error::Result;
use crate::git;
use crate::log::{Job, Level};
use crate::project::Hierarchy;
use crate::ui::changes::{self as panel, Action as HunkAction, Hit as PanelHit};
use crate::ui::{self};

mod actions;
mod branches;
mod selection;
pub(super) mod streams;

pub(super) use actions::Seen;
pub(super) use streams::Streams;

const CHANGES_LABEL: &str = "Changes";

pub(crate) type Gapped = Option<(Arc<ChangedFile>, Option<Vec<changes::GapLine>>)>;

#[derive(Debug)]
pub(crate) enum ChangesEvent {
    Loaded {
        workspace: u64,
        generation: u64,
        request: Box<changes::git::Request>,
        result: Result<changes::git::Loaded>,
    },
    Branches {
        workspace: u64,
        branches: Vec<String>,
        default: Option<String>,
    },
    Gap {
        workspace: u64,
        file: Arc<ChangedFile>,
        hunk: usize,
        lines: Vec<changes::GapLine>,
    },
    File {
        request: u64,
        workspace: u64,
        mode: Mode,
        path: String,
        result: Result<Option<Arc<ChangedFile>>>,
    },
    GapRead {
        request: u64,
        workspace: u64,
        path: String,
        hunk: usize,
        result: Result<Gapped>,
    },
    Base {
        request: u64,
        workspace: u64,
        change: BaseChange,
        listed: Result<(Vec<String>, Option<String>)>,
    },
    Acted {
        request: Option<u64>,
        workspace: u64,
        path: String,
        action: changes::act::FileAction,
        result: Result<changes::act::Outcome>,
    },
}

impl ChangesEvent {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Loaded { .. } => "changes",
            Self::Branches { .. } => "branches",
            Self::Gap { .. } => "gap",
            Self::File { .. } => "changes file",
            Self::GapRead { .. } => "changes gap",
            Self::Base { .. } => "changes base",
            Self::Acted { action, .. } => match action {
                changes::act::FileAction::Discard => "changes discard",
                changes::act::FileAction::Stage => "changes stage",
                changes::act::FileAction::Unstage => "changes unstage",
            },
        }
    }
}

impl App {
    pub(super) fn changes_answered(&mut self, event: ChangesEvent) {
        match event {
            ChangesEvent::Loaded { workspace, generation, request, result } => {
                self.panels.changes.loaded(workspace, generation, &request, result);
                if self.every_shown() || !self.clients.change_streams.is_empty() {
                    self.refresh_changes(Instant::now());
                }
            }
            ChangesEvent::Branches { workspace, branches, default } => {
                self.branches_listed(workspace, branches, default);
            }
            ChangesEvent::Gap { workspace, file, hunk, lines } => self.gap_loaded(workspace, &file, hunk, lines),
            ChangesEvent::File { request, workspace, mode, path, result } => {
                self.changes_file_replied(Replied { key: request, workspace, path: &path }, mode, result);
            }
            ChangesEvent::GapRead { request, workspace, path, hunk, result } => {
                self.changes_gap_replied(Replied { key: request, workspace, path: &path }, hunk, result);
            }
            ChangesEvent::Base { request, workspace, change, listed } => {
                self.changes_base_replied(request, workspace, &change, listed);
            }
            ChangesEvent::Acted { request, workspace, path, action, result } => {
                self.file_action_answered(request, (workspace, &path), action, result);
            }
        }
    }

    pub(super) fn changes_target(&self) -> Option<Checkout> {
        let workspace = self.project().filter(|p| !p.closing)?.workspace().filter(|w| w.open())?;
        git::branch(&workspace.path)?;
        Some(Checkout { workspace: workspace.id, dir: workspace.path.clone(), base: workspace.base.clone() })
    }

    pub(super) fn changes_shown(&self) -> bool {
        self.screen.side == Some(Side::Changes)
            && (self.panels.changes.scope() == changes::Scope::Every || self.changes_target().is_some())
    }

    fn every_shown(&self) -> bool {
        self.panels.changes.scope() == changes::Scope::Every && self.changes_shown()
    }

    pub(super) fn refresh_changes(&mut self, now: Instant) {
        self.end_gone_streams();
        let tui_every = self.every_shown();
        let every = tui_every || self.clients.change_streams.wants_every();
        let targets = if every { self.every_targets() } else { Vec::new() };
        let request =
            if tui_every { self.panels.changes.request_every(&targets, now) } else { self.project_request(now) };
        let request = request.or_else(|| self.feed_request(now)).or_else(|| {
            if every && !tui_every { self.panels.changes.request_sweep(&targets, now, false) } else { None }
        });
        if let Some((workspace, generation, request)) = request {
            self.spawn_changes(workspace, generation, request);
        }
        self.publish_change_streams(&targets, now);
    }

    fn project_request(&mut self, now: Instant) -> Option<(u64, u64, changes::git::Request)> {
        let target = self.changes_target()?;
        self.panels.changes.show_workspace(target.workspace);
        self.panels.changes.request(&target, now).map(|(generation, request)| (target.workspace, generation, request))
    }

    fn feed_request(&mut self, now: Instant) -> Option<(u64, u64, changes::git::Request)> {
        let mut followed: Vec<_> = self.clients.change_streams.followed().into_iter().collect();
        followed.sort_by_key(|(workspace, mode)| (*workspace, mode.name()));
        let wanted: Vec<(Checkout, Mode)> = followed
            .into_iter()
            .filter_map(|(workspace, mode)| {
                let ws = self.projects.workspace_by(workspace)?;
                Some((Checkout { workspace, dir: ws.path.clone(), base: ws.base.clone() }, mode))
            })
            .collect();
        self.panels.changes.request_followed(&wanted, now)
    }

    fn spawn_changes(&mut self, workspace: u64, generation: u64, request: changes::git::Request) {
        let kind = if request.expanded.is_some() { "summary" } else { "diff" };
        let job = Job::new(Level::Debug, "changes", kind).with("workspace", workspace);
        let loading = request.clone();
        self.spawn_job(
            job,
            move || changes::git::load(&loading),
            move |result, _| {
                Some(AppEvent::Changes(ChangesEvent::Loaded {
                    workspace,
                    generation,
                    request: Box::new(request),
                    result,
                }))
            },
        );
    }

    pub(super) fn changes_label(&self) -> Option<String> {
        self.changes_files().map(changes_label)
    }

    pub(super) fn changes_button_view(&self) -> Option<ui::ChangesButton> {
        let open = self.screen.side == Some(Side::Changes);
        self.changes_files().map(|files| ui::ChangesButton { label: changes_label(files), files, open })
    }

    pub(super) fn changes_files(&self) -> Option<usize> {
        let target = self.changes_target()?;
        let model = if self.every_shown() {
            self.panels.changes.summary(target.workspace)
        } else {
            self.panels.changes.model(target.workspace, target.base.as_deref())
        };
        Some(model.and_then(changes::Model::diff).map_or(0, |d| d.files.len()))
    }

    pub(super) fn toggle_changes(&mut self) {
        let side = (self.screen.side != Some(Side::Changes)).then_some(Side::Changes);
        self.show_side(side);
        self.screen.nav = None;
    }

    pub(super) fn filtering(&self) -> bool {
        self.overlay.is_none() && self.changes_shown() && self.panels.changes.filter().is_some_and(|f| f.focused)
    }

    pub(super) fn filter_key(&mut self, key: KeyEvent) {
        let Some(filter) = self.panels.changes.filter() else { return };
        let empty = filter.query().is_empty();
        match key.code {
            KeyCode::Esc if empty => {
                self.panels.changes.clear_filter();
                self.close_panel();
            }
            KeyCode::Esc => self.panels.changes.clear_filter(),
            KeyCode::Enter => self.panels.changes.unfocus(),
            KeyCode::Backspace => self.panels.changes.pop_filter(),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                self.panels.changes.push_filter(c);
            }
            _ => return,
        }
        self.panels.changes.scroll_to_top();
    }

    pub(super) fn changed_diff(&self, target: &Checkout) -> Option<Arc<changes::diff::Diff>> {
        self.panels.changes.model(target.workspace, target.base.as_deref()).and_then(|m| m.diff()).cloned()
    }

    pub(super) fn every_targets(&self) -> Vec<(Checkout, bool)> {
        every::targets(
            &self.projects,
            &crate::project::sidebar_order(&self.projects, &self.groups),
            &self.panels.changes,
        )
    }

    pub(super) fn every_body(&self, targets: &[(Checkout, bool)]) -> (changes::Body, Vec<u64>, Vec<every::Header>) {
        let order = crate::project::sidebar_order(&self.projects, &self.groups);
        every::group(&self.projects, &order, &self.panels.changes, targets, self.home.as_deref())
    }

    pub(super) fn panel_view(&self) -> Option<panel::View> {
        let mut view = self.plain_view()?;
        view.selection = self.panels.changes.selection().and_then(|s| panel::selected(&view, s));
        Some(view)
    }

    fn plain_view(&self) -> Option<panel::View> {
        let every = self.panels.changes.scope() == changes::Scope::Every;
        let target = if every { None } else { self.changes_target() };
        let model = target.as_ref().and_then(|t| self.panels.changes.model(t.workspace, t.base.as_deref()));
        let targets = if every { self.every_targets() } else { Vec::new() };
        let (body, owners, headers) =
            if every { self.every_body(&targets) } else { single_body(target.as_ref()?, model) };
        let (folded, viewed, gaps) = self.panel_marks(&body, &owners);
        let filter = self.panel_filter(&body);
        Some(panel::View {
            scope: self.panels.changes.scope(),
            project: self.panel_project(),
            owners,
            headers,
            mode: if every { Mode::Uncommitted } else { self.panels.changes.mode() },
            base: target.and_then(|t| self.panels.changes.label(t.workspace).or(t.base)).filter(|_| !every),
            body,
            folded,
            viewed,
            gaps,
            scroll: self.panels.changes.scroll(),
            live: self.panel_live(every, &targets, model),
            light: self.themes.light(),
            muted: ui::muted(self.themes.host()),
            soft: ui::soft(self.themes.host()),
            tints: Tints::of(self.themes.host()),
            filter,
            selection: None,
            icons: false,
        })
    }

    fn panel_marks(&self, body: &changes::Body, owners: &[u64]) -> Marks {
        let (mut folded, mut viewed, mut gaps) = (Vec::new(), Vec::new(), HashMap::new());
        if let changes::Body::Ready(diff) = body {
            for (i, file) in diff.files.iter().enumerate() {
                let ws = owners[i];
                folded.push(self.panels.changes.folded(ws, diff, file));
                viewed.push(self.panels.changes.viewed(ws, file));
                gaps.extend(self.file_gaps(ws, file).map(|(h, lines)| ((i, h), lines)));
            }
        }
        (folded, viewed, gaps)
    }

    fn file_gaps(&self, ws: u64, file: &ChangedFile) -> impl Iterator<Item = (usize, Arc<Vec<changes::GapLine>>)> {
        (1..file.hunks.len())
            .filter_map(move |h| self.panels.changes.gap(ws, file, h).map(|lines| (h, Arc::clone(lines))))
    }

    fn panel_filter(&self, body: &changes::Body) -> Option<panel::FilterView> {
        self.panels.changes.filter().map(|f| panel::FilterView {
            query: f.query().to_string(),
            focused: f.focused && self.overlay.is_none(),
            kept: match body {
                changes::Body::Ready(diff) => changes::filter::kept(diff, f.query()),
                _ => Vec::new(),
            },
        })
    }

    fn panel_project(&self) -> String {
        let Some(p) = self.project() else { return "Project".into() };
        let name = self.project_label(p);
        if p.workspaces.len() > 1 {
            p.workspace().map_or(name.clone(), |w| format!("{name} › {}", w.label()))
        } else {
            name
        }
    }

    fn panel_live(&self, every: bool, targets: &[(Checkout, bool)], model: Option<&changes::Model>) -> bool {
        if every {
            targets
                .iter()
                .all(|(t, _)| self.panels.changes.summary(t.workspace).is_some_and(|m| m.live(Instant::now())))
        } else {
            model.is_some_and(|m| m.live(Instant::now()))
        }
    }

    pub(super) fn changes_mouse(&mut self, ev: MouseEvent, pos: Position, panel_area: Rect, area: Rect) -> Result<()> {
        let Some(view) = self.panel_view() else { return Ok(()) };
        if let Some(delta) = wheel(ev.kind) {
            let max = panel::max_scroll(panel_area, &view);
            self.panels.changes.scroll_by(delta, max);
            return Ok(());
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return Ok(());
        }
        let hit = panel::hit(panel_area, &view, pos);
        if self.press_selection(&view, hit, panel_area, (pos, area)) {
            return Ok(());
        }
        let chosen = match hit {
            Some(
                PanelHit::File(i)
                | PanelHit::Viewed(i)
                | PanelHit::Stage(i)
                | PanelHit::Discard(i)
                | PanelHit::Gap(i, _)
                | PanelHit::Action(i, _, _),
            ) => self.chosen_file(&view, i),
            _ => None,
        };
        if let (Some(PanelHit::Action(_, index, action)), Some(chosen)) = (hit, &chosen) {
            return self.hunk_action(&chosen.target, &chosen.file, Hunk { index, action }, area);
        }
        self.changes_hit(hit, &view, chosen);
        if self.every_shown() {
            self.refresh_changes(Instant::now());
        }
        Ok(())
    }

    fn press_selection(
        &mut self,
        view: &panel::View,
        hit: Option<PanelHit>,
        panel_area: Rect,
        at: (Position, Rect),
    ) -> bool {
        match hit {
            Some(PanelHit::Line(i, spot)) => self.press_diff_line(view, (i, spot), at.1),
            Some(PanelHit::Text(i, spot)) => self.press_diff_text(view, (i, spot), (at.0, panel_area, at.1)),
            Some(PanelHit::Lines(action)) => self.selected_lines_action(view, action),
            None if panel::parts(panel_area, view).body.contains(at.0) => self.panels.changes.clear_selection(),
            _ => return false,
        }
        true
    }

    fn chosen_file(&self, view: &panel::View, i: usize) -> Option<Chosen> {
        let file = ready(view)?.files.get(i)?.clone();
        let workspace = *view.owners.get(i)?;
        let ws = self.projects.workspace_by(workspace)?;
        Some(Chosen { target: Checkout { workspace, dir: ws.path.clone(), base: ws.base.clone() }, file })
    }

    fn changes_hit(&mut self, hit: Option<PanelHit>, view: &panel::View, chosen: Option<Chosen>) {
        match hit {
            Some(PanelHit::Scope(scope)) => self.panels.changes.set_scope(scope),
            Some(PanelHit::Header(i)) => self.toggle_changes_header(view, i),
            Some(PanelHit::Mode(mode)) => self.panels.changes.set_mode(mode),
            Some(PanelHit::Close) => self.show_side(None),
            Some(PanelHit::Filter | PanelHit::Query) => {
                self.panels.changes.focus_filter();
            }
            Some(PanelHit::ClearFilter) => self.panels.changes.clear_filter(),
            Some(PanelHit::Base) => self.pick_changes_base(),
            Some(PanelHit::FoldAll) => self.fold_all_files(view),
            Some(PanelHit::File(_)) => self.toggle_file_fold(view, chosen),
            Some(PanelHit::Viewed(_)) => {
                if let Some(chosen) = chosen {
                    self.panels.changes.toggle_viewed(chosen.target.workspace, &chosen.file);
                }
            }
            Some(PanelHit::Stage(_)) => self.stage_clicked(chosen),
            Some(PanelHit::Discard(_)) => self.ask_discard(chosen),
            Some(PanelHit::Gap(_, h)) => self.read_gap(view.mode, chosen, h),
            Some(PanelHit::Action(..) | PanelHit::Line(..) | PanelHit::Text(..) | PanelHit::Lines(_)) | None => {}
        }
    }

    fn toggle_changes_header(&mut self, view: &panel::View, i: usize) {
        self.panels.changes.toggle_header(view.headers[i].id);
        for workspace in &view.owners[view.headers[i].files.clone()] {
            self.panels.changes.invalidate_summary(*workspace);
        }
    }

    fn pick_changes_base(&mut self) {
        if let Some(target) = self.changes_target() {
            self.open_branches(&target);
        }
    }

    fn fold_all_files(&mut self, view: &panel::View) {
        let Some(diff) = ready(view) else { return };
        let fold = view.folded.iter().any(|f| !f);
        if !fold {
            self.panels.changes.unfold_headers();
        }
        for (file, workspace) in diff.files.iter().zip(&view.owners) {
            self.panels.changes.set_fold(*workspace, file, fold);
        }
    }

    fn toggle_file_fold(&mut self, view: &panel::View, chosen: Option<Chosen>) {
        if let (Some(diff), Some(chosen)) = (ready(view), chosen) {
            self.panels.changes.toggle_fold(chosen.target.workspace, diff, &chosen.file);
        }
    }

    fn read_gap(&mut self, mode: Mode, chosen: Option<Chosen>, h: usize) {
        let Some(Chosen { target, file }) = chosen else { return };
        let (workspace, dir) = (target.workspace, target.dir);
        let job = Job::new(Level::Debug, "changes", "unchanged lines").with("workspace", workspace);
        self.spawn_job(
            job,
            move || Ok(changes::gap(&dir, mode, &file, h).map(|lines| (file, lines))),
            move |found, _| {
                let (file, lines) = found.ok().flatten()?;
                Some(AppEvent::Changes(ChangesEvent::Gap { workspace, file, hunk: h, lines }))
            },
        );
    }

    pub(super) fn hunk_action(&mut self, target: &Checkout, file: &ChangedFile, hunk: Hunk, area: Rect) -> Result<()> {
        let Some(found) = file.hunks.get(hunk.index) else { return Ok(()) };
        let (first, last) = found.changed();
        match hunk.action {
            HunkAction::Copy => self.copy(&found.patch()),
            HunkAction::Open => return self.open_in_editor(target, &file.path, first, area),
            HunkAction::Ask => {
                let lines = if first == last { first.to_string() } else { format!("{first}-{last}") };
                self.ask_agent(target.workspace, &format!("{}:{lines} ", file.path));
            }
        }
        Ok(())
    }

    pub(super) fn gap_loaded(&mut self, workspace: u64, file: &ChangedFile, hunk: usize, lines: Vec<changes::GapLine>) {
        let current = if self.panels.changes.scope() == changes::Scope::Every {
            self.panels
                .changes
                .summary(workspace)
                .and_then(changes::Model::diff)
                .is_some_and(|d| d.files.iter().any(|f| f.digest == file.digest))
        } else {
            self.changes_target()
                .filter(|t| t.workspace == workspace)
                .and_then(|t| self.changed_diff(&t))
                .is_some_and(|d| d.files.iter().any(|f| f.digest == file.digest))
        };
        if current {
            self.panels.changes.set_gap(workspace, file, hunk, lines);
        }
    }
}

type Marks = (Vec<bool>, Vec<bool>, HashMap<(usize, usize), Arc<Vec<changes::GapLine>>>);

#[derive(Clone, Copy)]
pub(in crate::app) struct Hunk {
    pub(in crate::app) index: usize,
    pub(in crate::app) action: HunkAction,
}

struct Chosen {
    target: Checkout,
    file: Arc<ChangedFile>,
}

fn ready(view: &panel::View) -> Option<&Arc<changes::diff::Diff>> {
    match &view.body {
        changes::Body::Ready(diff) => Some(diff),
        _ => None,
    }
}

fn single_body(target: &Checkout, model: Option<&changes::Model>) -> (changes::Body, Vec<u64>, Vec<every::Header>) {
    let body = match model.map(|m| &m.result) {
        None => changes::Body::Loading,
        Some(Ok(diff)) => changes::Body::Ready(Arc::clone(diff)),
        Some(Err(e)) => changes::Body::Failed(e.clone()),
    };
    let owners = match &body {
        changes::Body::Ready(d) => vec![target.workspace; d.files.len()],
        _ => Vec::new(),
    };
    (body, owners, Vec::new())
}

pub(super) fn changes_label(files: usize) -> String {
    if files > 0 { format!("{CHANGES_LABEL} {files}") } else { CHANGES_LABEL.to_string() }
}
