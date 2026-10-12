use std::collections::{HashMap, HashSet};
use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::App;
use super::event::AppEvent;
use super::input::wheel;
use super::toast::copy;
use crate::changes::diff::File as ChangedFile;
use crate::changes::{self, Checkout, Tints};
use crate::error::Result;
use crate::git;
use crate::log::{Job, Level};
use crate::panics;
use crate::ui::changes::{self as panel, Action as HunkAction, Hit as PanelHit};
use crate::ui::{self, SidebarRow};

mod branches;

const CHANGES_LABEL: &str = "Changes";

impl App {
    pub(super) fn changes_target(&self) -> Option<Checkout> {
        let workspace = self.project().filter(|p| !p.closing)?.workspace().filter(|w| w.open())?;
        git::branch(&workspace.path)?;
        Some(Checkout { workspace: workspace.id, dir: workspace.path.clone(), base: workspace.base.clone() })
    }

    pub(super) fn changes_shown(&self) -> bool {
        self.changes.open && (self.changes.scope == changes::Scope::Every || self.changes_target().is_some())
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
        let tx = self.tx.clone();
        let kind = if request.expanded.is_some() { "summary" } else { "diff" };
        let job = Job::new(Level::Debug, "changes", kind).with("workspace", workspace).begin();
        std::thread::spawn(move || {
            let result = panics::job(|| changes::git::load(&request));
            job.finish(&result);
            let _ = tx.send(AppEvent::Changes { workspace, generation, request: Box::new(request), result });
        });
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
        if self.changes.open {
            self.changes.close();
        } else {
            self.close_todo();
            self.files.close();
            self.changes.open = true;
        }
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

    fn changes_project_order(&self) -> Vec<usize> {
        ui::sidebar_rows(&self.project_groups(), &vec![false; self.groups.len()])
            .into_iter()
            .filter_map(|row| if let SidebarRow::Project(p) = row { Some(p) } else { None })
            .collect()
    }

    pub(super) fn every_targets(&self) -> Vec<(Checkout, bool)> {
        self.changes_project_order()
            .into_iter()
            .flat_map(|p| {
                let project = &self.projects[p];
                let workspaces: Vec<_> = project
                    .workspaces
                    .iter()
                    .filter(|w| !project.closing && w.open() && git::branch(&w.path).is_some())
                    .collect();
                let several = workspaces
                    .iter()
                    .filter(|w| {
                        self.changes.summary(w.id).and_then(changes::Model::diff).is_some_and(|d| !d.files.is_empty())
                    })
                    .count()
                    > 1;
                workspaces
                    .into_iter()
                    .map(|w| {
                        let hidden = self.changes.headers.contains(&project.id)
                            || (several && self.changes.headers.contains(&w.id));
                        (Checkout { workspace: w.id, dir: w.path.clone(), base: None }, hidden)
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    pub(super) fn every_body(&self, targets: &[(Checkout, bool)]) -> (panel::Body, Vec<u64>, Vec<panel::Header>) {
        let mut diff = changes::diff::Diff::default();
        let mut owners = Vec::new();
        let mut headers = Vec::new();
        let mut error = None;
        let workspaces: HashSet<_> = targets.iter().map(|(t, _)| t.workspace).collect();
        for p in self.changes_project_order() {
            let project = &self.projects[p];
            if error.is_none() {
                error = project.workspaces.iter().filter(|w| workspaces.contains(&w.id)).find_map(|w| {
                    let failure = self.changes.summary(w.id)?.result.as_ref().err()?;
                    Some(format!("{}: {failure}", self.project_label(project)))
                });
            }
            let workspaces: Vec<_> = project
                .workspaces
                .iter()
                .filter(|w| workspaces.contains(&w.id))
                .filter_map(|w| {
                    let diff = self.changes.summary(w.id)?.diff()?;
                    (!diff.files.is_empty()).then_some((w, diff))
                })
                .collect();
            if workspaces.is_empty() {
                continue;
            }
            let start = diff.files.len();
            let count = workspaces.iter().map(|(_, d)| d.files.len()).sum::<usize>();
            headers.push(panel::Header {
                id: project.id,
                label: self.project_label(project),
                files: start..start + count,
                workspace: false,
                folded: self.changes.headers.contains(&project.id),
            });
            let several = workspaces.len() > 1;
            for (workspace, changes) in workspaces {
                let start = diff.files.len();
                if several {
                    headers.push(panel::Header {
                        id: workspace.id,
                        label: workspace.label(),
                        files: start..start + changes.files.len(),
                        workspace: true,
                        folded: self.changes.headers.contains(&workspace.id),
                    });
                }
                owners.extend(std::iter::repeat_n(workspace.id, changes.files.len()));
                diff.files.extend(changes.files.iter().cloned());
            }
        }
        let body = if diff.files.is_empty() && targets.iter().any(|(t, _)| self.changes.summary(t.workspace).is_none())
        {
            panel::Body::Loading
        } else if diff.files.is_empty() && error.is_some() {
            panel::Body::Failed(error.unwrap_or_default())
        } else {
            panel::Body::Ready(std::sync::Arc::new(diff))
        };
        (body, owners, headers)
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
                None => panel::Body::Loading,
                Some(Ok(diff)) => panel::Body::Ready(std::sync::Arc::clone(diff)),
                Some(Err(e)) => panel::Body::Failed(e.clone()),
            };
            let owners = match &body {
                panel::Body::Ready(d) => vec![target.workspace; d.files.len()],
                _ => Vec::new(),
            };
            (body, owners, Vec::new())
        };
        let (mut folded, mut viewed, mut gaps) = (Vec::new(), Vec::new(), HashMap::new());
        if let panel::Body::Ready(diff) = &body {
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
                panel::Body::Ready(diff) => changes::filter::kept(diff, f.query()),
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
            panel::Body::Ready(d) => Some(d.clone()),
            _ => None,
        };
        let file = |i: usize| {
            let file = diff.as_ref()?.files.get(i)?.clone();
            let workspace = *view.owners.get(i)?;
            let (p, w) = self.workspace_position(workspace)?;
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
            Some(PanelHit::Close) => self.changes.close(),
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
                    let (tx, mode, workspace, dir) = (self.tx.clone(), view.mode, target.workspace, target.dir);
                    let job = Job::new(Level::Debug, "changes", "unchanged lines").with("workspace", workspace).begin();
                    std::thread::spawn(move || {
                        let new_side = changes::git::new_side(&dir, mode, &f.path);
                        job.done();
                        let Some(new_side) = new_side else { return };
                        let lines = changes::gap_lines(&f, h, &new_side);
                        let _ = tx.send(AppEvent::Gap { workspace, file: f, hunk: h, lines });
                    });
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
            HunkAction::Copy => copy(&mut self.host_writes, &mut self.toast, &hunk.patch()),
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
