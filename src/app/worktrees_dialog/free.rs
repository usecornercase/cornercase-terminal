use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::Outside;
use super::confirm::{INDENT, wrap};
use crate::app::App;
use crate::app::event::AppEvent;
use crate::app::overlay::Overlay;
use crate::app::toast::Toast;
use crate::app::worktrees::WorktreeEvent;
use crate::error::Result;
use crate::log::{Job, Level};
use crate::text::{self, plural};
use crate::ui;
use crate::worktrees::builds::{self, Freed, Size};
use crate::worktrees::{Dialog, Row, Step};

struct FreeTarget {
    path: PathBuf,
    repo: PathBuf,
    chosen: Vec<PathBuf>,
    name: String,
}

#[derive(Default)]
pub(in crate::app) struct Frees {
    pending: HashMap<u64, String>,
    bytes: u64,
    worktrees: usize,
    problems: Vec<String>,
}

impl App {
    pub(super) fn free_view(&self, dialog: &Dialog, area: Rect, paths: &[PathBuf]) -> ui::ConfirmView {
        let mut lines = Vec::new();
        let mut total = 0;
        for row in dialog.freeable(paths) {
            total += row.builds_bytes().unwrap_or(0);
            lines.extend(self.free_lines(row));
        }
        let checking = self.confirm_checking();
        let (_, body) = ui::worktrees_room(area, area.width < ui::COMPACT_WIDTH);
        let lines = wrap(lines, body.width);
        let most = lines.len().saturating_sub(usize::from(body.height));
        let kind = ui::Purpose::Free { label: format!("free {}", text::size(total)) };
        ui::ConfirmView { lines, scroll: dialog.body.min(most), branches: false, checking, kind }
    }

    fn free_lines(&self, row: &Row) -> Vec<Line<'static>> {
        let Some(Ok(size)) = &row.size else { return Vec::new() };
        let warnings = self.free_warnings(row, size);
        let name = row.branch().map_or_else(|| row.found.path.display().to_string(), str::to_string);
        let header = Style::default().add_modifier(Modifier::BOLD);
        let header = if warnings.is_empty() { header } else { header.fg(Color::Yellow) };
        let mut lines = vec![Line::from(Span::styled(format!("{} › {name}", row.found.project), header))];
        let folders: Vec<String> = size
            .builds
            .iter()
            .map(|folder| format!("{} {}", folder.path.display(), text::size(folder.bytes)))
            .collect();
        lines.push(Line::from(format!("{INDENT}frees {}: {}", text::size(size.builds_bytes()), folders.join(", "))));
        let yellow = Style::default().fg(Color::Yellow);
        lines
            .extend(warnings.into_iter().map(|warning| Line::from(Span::styled(format!("{INDENT}{warning}"), yellow))));
        lines
    }

    fn free_warnings(&self, row: &Row, size: &Size) -> Vec<String> {
        let mut warnings =
            Self::presence_risks(row, row.live.as_ref(), true, matches!(self.git.scans.outside, Outside::Failed));
        warnings.extend(
            size.nested.iter().map(|path| {
                format!("{} is a git checkout; its changes go too", path.parent().unwrap_or(path).display())
            }),
        );
        warnings
    }

    pub(super) fn free_confirmed(&mut self) {
        let Some(Overlay::Worktrees(dialog)) = &self.overlay else { return };
        let Step::Free { paths } = &dialog.step else { return };
        if !dialog.can_free_now() || self.confirm_checking() {
            return;
        }
        let targets: Vec<FreeTarget> = dialog
            .freeable(paths)
            .filter_map(|row| match &row.size {
                Some(Ok(size)) => {
                    let chosen = size.builds.iter().map(|folder| folder.path.clone()).collect();
                    let name = row.branch().map_or_else(|| row.found.path.display().to_string(), str::to_string);
                    let name = format!("{} › {name}", row.found.project);
                    Some(FreeTarget { path: row.found.path.clone(), repo: row.found.repo.clone(), chosen, name })
                }
                _ => None,
            })
            .collect();
        if let Some(Overlay::Worktrees(dialog)) = &mut self.overlay {
            dialog.step = Step::List;
            for row in
                dialog.list.rows.iter_mut().filter(|row| targets.iter().any(|target| target.path == row.found.path))
            {
                row.freeing = true;
                row.ticked = false;
            }
        }
        for target in targets {
            self.spawn_free(target);
        }
    }

    fn spawn_free(&mut self, FreeTarget { path, repo, chosen, name }: FreeTarget) {
        let key = self.take_id();
        let names = self.builds_of(&repo);
        self.git.frees.pending.insert(key, name);
        let job = Job::new(Level::Info, "worktrees", "free builds").with("path", path.display());
        let at = path.clone();
        self.spawn_job(
            job,
            move || builds::free(&at, &names, &chosen),
            move |result, job| {
                if let Ok(freed) = &result {
                    job.note("bytes", freed.bytes());
                }
                Some(AppEvent::Worktree(WorktreeEvent::Freed { key, path, result: Box::new(result) }))
            },
        );
    }

    pub(in crate::app) fn builds_freed(&mut self, key: u64, path: &Path, result: Result<Freed>) {
        let Some(name) = self.git.frees.pending.remove(&key) else { return };
        let after = match &result {
            Ok(freed) => freed.after.clone().map(Ok),
            Err(_) => None,
        };
        self.settle_freed_row(path, after);
        let tally = &mut self.git.frees;
        match result {
            Ok(freed) => {
                tally.bytes += freed.bytes();
                tally.worktrees += usize::from(!freed.folders.is_empty());
                tally.problems.extend(
                    freed
                        .failed
                        .iter()
                        .map(|(folder, why)| format!("could not free {name}: {}: {why}", folder.display())),
                );
                tally.problems.extend(freed.kept.iter().map(|folder| {
                    format!(
                        "kept {name}: {} is not freed: no longer an ignored folder here, or on another filesystem",
                        folder.display()
                    )
                }));
            }
            Err(e) => tally.problems.push(format!("could not free {name}: {e}")),
        }
        self.finish_frees();
    }

    fn settle_freed_row(&mut self, path: &Path, after: Option<std::result::Result<Size, String>>) {
        let Some(Overlay::Worktrees(dialog)) = &mut self.overlay else { return };
        let Some(row) = dialog.list.rows.iter_mut().find(|row| row.found.path == path) else { return };
        row.freeing = false;
        if let Some(after) = after {
            row.size = Some(after);
        }
    }

    fn finish_frees(&mut self) {
        if !self.git.frees.pending.is_empty() {
            return;
        }
        let Frees { bytes, worktrees, problems, .. } = std::mem::take(&mut self.git.frees);
        let mut text = if bytes == 0 && worktrees == 0 {
            "nothing was freed".to_string()
        } else {
            format!("freed {} in {}", text::size(bytes), plural(worktrees, "worktree", "worktrees"))
        };
        let icon = if problems.is_empty() { ui::ToastIcon::Check } else { ui::ToastIcon::Bug };
        if !problems.is_empty() {
            text = format!("{text}; {}, see server.log", problems.join("; "));
        }
        self.messages.toast = Some(Toast::new(text, icon));
    }
}
