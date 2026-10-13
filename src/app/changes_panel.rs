use std::collections::HashMap;
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::event::AppEvent;
use super::input::wheel;
use super::layout::Side;
use crate::changes::diff::File as ChangedFile;
use crate::changes::{self, Checkout, Tints, every};
use crate::error::Result;
use crate::git;
use crate::log::{Job, Level};
use crate::project::Hierarchy;
use crate::ui::changes::{self as panel, Action as HunkAction, Hit as PanelHit};
use crate::ui::{self};

mod branches;

const CHANGES_LABEL: &str = "Changes";

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
        file: std::sync::Arc<ChangedFile>,
        hunk: usize,
        lines: Vec<changes::GapLine>,
    },
}

impl ChangesEvent {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Loaded { .. } => "changes",
            Self::Branches { .. } => "branches",
            Self::Gap { .. } => "gap",
        }
    }
}

impl App {
    pub(super) fn changes_answered(&mut self, event: ChangesEvent) {
        match event {
            ChangesEvent::Loaded { workspace, generation, request, result } => {
                self.changes.loaded(workspace, generation, &request, result);
                if self.changes_shown() && self.changes.scope == changes::Scope::Every {
                    self.refresh_changes(Instant::now());
                }
            }
            ChangesEvent::Branches { workspace, branches, default } => {
                self.branches_listed(workspace, branches, default);
            }
            ChangesEvent::Gap { workspace, file, hunk, lines } => self.gap_loaded(workspace, &file, hunk, lines),
        }
    }

    pub(super) fn changes_target(&self) -> Option<Checkout> {
        let workspace = self.project().filter(|p| !p.closing)?.workspace().filter(|w| w.open())?;
        git::branch(&workspace.path)?;
        Some(Checkout { workspace: workspace.id, dir: workspace.path.clone(), base: workspace.base.clone() })
    }

    pub(super) fn changes_shown(&self) -> bool {
        self.side == Some(Side::Changes)
            && (self.changes.scope == changes::Scope::Every || self.changes_target().is_some())
    }

    pub(super) fn refresh_changes(&mut self, now: Instant) {
        let request = if self.changes.scope == changes::Scope::Every && self.changes_shown() {
            let targets = self.every_targets();
            self.changes.request_every(&targets, now)
        } else {
            self.changes_target().and_then(|target| {
                if self.changes.workspace != Some(target.workspace) {
                    self.changes.workspace = Some(target.workspace);
                    self.changes.scroll = 0;
                }
                self.changes.request(&target, now).map(|(generation, request)| (target.workspace, generation, request))
            })
        };
        let Some((workspace, generation, request)) = request else { return };
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
        let target = self.changes_target()?;
        let model = if self.changes.scope == changes::Scope::Every && self.changes_shown() {
            self.changes.summary(target.workspace)
        } else {
            self.changes.model(target.workspace, target.base.as_deref())
        };
        let files = model.and_then(changes::Model::diff).map(|d| d.files.len());
        Some(match files {
            Some(n) if n > 0 => format!("{CHANGES_LABEL} {n}"),
            _ => CHANGES_LABEL.to_string(),
        })
    }

    pub(super) fn toggle_changes(&mut self) {
        let side = (self.side != Some(Side::Changes)).then_some(Side::Changes);
        self.show_side(side);
        self.nav = None;
    }

    pub(super) fn filtering(&self) -> bool {
        self.overlay.is_none() && self.changes_shown() && self.changes.filter.as_ref().is_some_and(|f| f.focused)
    }

    pub(super) fn filter_key(&mut self, key: KeyEvent) {
        let Some(filter) = &mut self.changes.filter else { return };
        let empty = filter.query().is_empty();
        match key.code {
            KeyCode::Esc if empty => {
                self.changes.filter = None;
                self.close_panel();
            }
            KeyCode::Esc => self.changes.filter = None,
            KeyCode::Enter => filter.focused = false,
            KeyCode::Backspace => filter.pop(),
            KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => filter.push(c),
            _ => return,
        }
        self.changes.scroll = 0;
    }

    pub(super) fn changed_diff(&self, target: &Checkout) -> Option<std::sync::Arc<changes::diff::Diff>> {
        self.changes.model(target.workspace, target.base.as_deref()).and_then(|m| m.diff()).cloned()
    }

    pub(super) fn every_targets(&self) -> Vec<(Checkout, bool)> {
        every::targets(&self.projects, &crate::project::sidebar_order(&self.projects, &self.groups), &self.changes)
    }

    pub(super) fn every_body(&self, targets: &[(Checkout, bool)]) -> (changes::Body, Vec<u64>, Vec<every::Header>) {
        let order = crate::project::sidebar_order(&self.projects, &self.groups);
        every::group(&self.projects, &order, &self.changes, targets, self.home.as_deref())
    }

    pub(super) fn panel_view(&self) -> Option<panel::View> {
        let every = self.changes.scope == changes::Scope::Every;
        let target = if every { None } else { self.changes_target() };
        let model = target.as_ref().and_then(|t| self.changes.model(t.workspace, t.base.as_deref()));
        let targets = if every { self.every_targets() } else { Vec::new() };
        let (body, owners, headers) = if every {
            self.every_body(&targets)
        } else {
            let target = target.as_ref()?;
            let body = match model.map(|m| &m.result) {
                None => changes::Body::Loading,
                Some(Ok(diff)) => changes::Body::Ready(std::sync::Arc::clone(diff)),
                Some(Err(e)) => changes::Body::Failed(e.clone()),
            };
            let owners = match &body {
                changes::Body::Ready(d) => vec![target.workspace; d.files.len()],
                _ => Vec::new(),
            };
            (body, owners, Vec::new())
        };
        let (mut folded, mut viewed, mut gaps) = (Vec::new(), Vec::new(), HashMap::new());
        if let changes::Body::Ready(diff) = &body {
            for (i, file) in diff.files.iter().enumerate() {
                let ws = owners[i];
                folded.push(self.changes.folded(ws, diff, file));
                viewed.push(self.changes.viewed(ws, file));
                for h in 1..file.hunks.len() {
                    if let Some(lines) = self.changes.gap(ws, file, h) {
                        gaps.insert((i, h), std::sync::Arc::clone(lines));
                    }
                }
            }
        }
        let filter = self.changes.filter.as_ref().map(|f| panel::FilterView {
            query: f.query().to_string(),
            focused: f.focused && self.overlay.is_none(),
            kept: match &body {
                changes::Body::Ready(diff) => changes::filter::kept(diff, f.query()),
                _ => Vec::new(),
            },
        });
        let project = self.project().map_or_else(
            || "Project".into(),
            |p| {
                let name = self.project_label(p);
                if p.workspaces.len() > 1 {
                    p.workspace().map_or(name.clone(), |w| format!("{name} › {}", w.label()))
                } else {
                    name
                }
            },
        );
        Some(panel::View {
            scope: self.changes.scope,
            project,
            owners,
            headers,
            mode: if every { changes::Mode::Uncommitted } else { self.changes.mode },
            base: target.and_then(|t| self.changes.label(t.workspace).or(t.base)).filter(|_| !every),
            body,
            folded,
            viewed,
            gaps,
            scroll: self.changes.scroll,
            live: if every {
                targets.iter().all(|(t, _)| self.changes.summary(t.workspace).is_some_and(|m| m.live(Instant::now())))
            } else {
                model.is_some_and(|m| m.live(Instant::now()))
            },
            light: self.theme.is_light() == Some(true),
            muted: ui::muted(&self.theme),
            tints: Tints::of(&self.theme),
            filter,
        })
    }

    pub(super) fn changes_mouse(&mut self, ev: MouseEvent, pos: Position, panel_area: Rect, area: Rect) -> Result<()> {
        let Some(view) = self.panel_view() else { return Ok(()) };
        if let Some(delta) = wheel(ev.kind) {
            let max = panel::max_scroll(panel_area, &view);
            self.changes.scroll = self.changes.scroll.min(max).saturating_add_signed(delta).min(max);
            return Ok(());
        }
        if ev.kind != MouseEventKind::Down(MouseButton::Left) {
            return Ok(());
        }
        let diff = match &view.body {
            changes::Body::Ready(d) => Some(d.clone()),
            _ => None,
        };
        let file = |i: usize| {
            let file = diff.as_ref()?.files.get(i)?.clone();
            let workspace = *view.owners.get(i)?;
            let (p, w) = self.projects.workspace_index(workspace)?;
            let ws = &self.projects[p].workspaces[w];
            Some((Checkout { workspace, dir: ws.path.clone(), base: ws.base.clone() }, file))
        };
        let hit = panel::hit(panel_area, &view, pos);
        let selected = match hit {
            Some(PanelHit::File(i) | PanelHit::Viewed(i) | PanelHit::Gap(i, _) | PanelHit::Action(i, _, _)) => file(i),
            _ => None,
        };
        match hit {
            Some(PanelHit::Scope(scope)) => self.changes.set_scope(scope),
            Some(PanelHit::Header(i)) => {
                self.changes.toggle_header(view.headers[i].id);
                for workspace in &view.owners[view.headers[i].files.clone()] {
                    self.changes.invalidate_summary(*workspace);
                }
            }
            Some(PanelHit::Mode(mode)) => self.changes.set_mode(mode),
            Some(PanelHit::Close) => self.show_side(None),
            Some(PanelHit::Filter | PanelHit::Query) => self.changes.filter.get_or_insert_default().focused = true,
            Some(PanelHit::ClearFilter) => self.changes.filter = None,
            Some(PanelHit::Base) => {
                if let Some(target) = self.changes_target() {
                    self.open_branches(&target);
                }
            }
            Some(PanelHit::FoldAll) => {
                if let Some(diff) = &diff {
                    let fold = view.folded.iter().any(|f| !f);
                    if !fold {
                        self.changes.headers.clear();
                    }
                    for (file, workspace) in diff.files.iter().zip(&view.owners) {
                        self.changes.set_fold(*workspace, file, fold);
                    }
                }
            }
            Some(PanelHit::File(_)) => {
                if let (Some(diff), Some((target, f))) = (&diff, selected) {
                    self.changes.toggle_fold(target.workspace, diff, &f);
                }
            }
            Some(PanelHit::Viewed(_)) => {
                if let Some((target, f)) = selected {
                    self.changes.toggle_viewed(target.workspace, &f);
                }
            }
            Some(PanelHit::Gap(_, h)) => {
                if let Some((target, f)) = selected {
                    let (mode, workspace, dir) = (view.mode, target.workspace, target.dir);
                    let job = Job::new(Level::Debug, "changes", "unchanged lines").with("workspace", workspace);
                    let path = f.path.clone();
                    self.spawn_job(
                        job,
                        move || Ok(changes::git::new_side(&dir, mode, &path)),
                        move |new_side, _| {
                            let new_side = new_side.ok().flatten()?;
                            let lines = changes::gap_lines(&f, h, &new_side);
                            Some(AppEvent::Changes(ChangesEvent::Gap { workspace, file: f, hunk: h, lines }))
                        },
                    );
                }
            }
            Some(PanelHit::Action(_, h, action)) => {
                if let Some((target, f)) = selected {
                    return self.hunk_action(&target, &f, h, action, area);
                }
            }
            None => {}
        }
        if self.changes.scope == changes::Scope::Every && self.changes_shown() {
            self.refresh_changes(Instant::now());
        }
        Ok(())
    }

    pub(super) fn hunk_action(
        &mut self,
        target: &Checkout,
        file: &ChangedFile,
        h: usize,
        action: HunkAction,
        area: Rect,
    ) -> Result<()> {
        let Some(hunk) = file.hunks.get(h) else { return Ok(()) };
        let (first, last) = hunk.changed();
        match action {
            HunkAction::Copy => self.copy(&hunk.patch()),
            HunkAction::Open => return self.open_in_editor(target, &file.path, first, area),
            HunkAction::Ask => {
                let lines = if first == last { first.to_string() } else { format!("{first}-{last}") };
                self.ask_agent(target.workspace, &format!("{}:{lines} ", file.path));
            }
        }
        Ok(())
    }

    pub(super) fn gap_loaded(&mut self, workspace: u64, file: &ChangedFile, hunk: usize, lines: Vec<changes::GapLine>) {
        let current = if self.changes.scope == changes::Scope::Every {
            self.changes
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
            self.changes.set_gap(workspace, file, hunk, lines);
        }
    }
}
