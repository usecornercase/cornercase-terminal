pub mod diff;
pub mod every;
pub mod filter;
pub mod git;
mod summary;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use self::diff::{Diff, File, Segments};
use crate::error::Result;
use crate::host::theme::HostTheme;
use crate::list::Cursor;

const OPEN_EVERY: Duration = Duration::from_secs(1);
const CLOSED_EVERY: Duration = Duration::from_secs(3);
const LIVE_FOR: Duration = Duration::from_secs(5);
const GIVE_UP_AFTER: Duration = Duration::from_secs(30);
const AUTO_FOLD_LINES: usize = 500;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Uncommitted,
    Commits,
    All,
}

impl Mode {
    pub const ALL: [Self; 3] = [Self::Uncommitted, Self::Commits, Self::All];

    pub fn label(self) -> &'static str {
        match self {
            Self::Uncommitted => "Uncommitted",
            Self::Commits => "Commits",
            Self::All => "All",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    #[default]
    Project,
    Every,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GapLine {
    pub old: u32,
    pub new: u32,
    pub text: String,
    pub syntax: Option<Segments>,
}

#[derive(Debug)]
pub struct Model {
    pub mode: Mode,
    pub base: Option<String>,
    pub label: Option<String>,
    pub result: std::result::Result<Arc<Diff>, String>,
    pub digest: Option<u64>,
    pub at: Instant,
}

impl Model {
    pub fn diff(&self) -> Option<&Arc<Diff>> {
        self.result.as_ref().ok()
    }

    pub fn live(&self, now: Instant) -> bool {
        self.result.is_ok() && now.duration_since(self.at) < LIVE_FOR
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FileKey {
    workspace: u64,
    path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Loading,
    Failed(String),
    Ready(Arc<Diff>),
}

#[derive(Debug, Default)]
pub struct Panel {
    pub watched: bool,
    pub mode: Mode,
    pub scope: Scope,
    pub scroll: usize,
    pub workspace: Option<u64>,
    pub filter: Option<filter::Filter>,
    models: HashMap<u64, Model>,
    summaries: HashMap<u64, Model>,
    every_folded: HashMap<FileKey, bool>,
    pub headers: HashSet<u64>,
    next: usize,
    summary_requested: HashMap<u64, Instant>,
    folded: HashMap<FileKey, bool>,
    viewed: HashMap<FileKey, u64>,
    gaps: HashMap<(FileKey, u64, usize), Arc<Vec<GapLine>>>,
    loading: Option<(u64, u64, Instant)>,
    generation: u64,
    requested: HashMap<u64, Instant>,
}

pub struct Checkout {
    pub workspace: u64,
    pub dir: PathBuf,
    pub base: Option<String>,
}

fn key(workspace: u64, file: &File) -> FileKey {
    FileKey { workspace, path: file.path.clone() }
}

impl Panel {
    pub fn model(&self, workspace: u64, base: Option<&str>) -> Option<&Model> {
        self.models.get(&workspace).filter(|m| m.mode == self.mode && m.base.as_deref() == base)
    }

    pub fn request(&mut self, target: &Checkout, now: Instant) -> Option<(u64, git::Request)> {
        if self.busy(now) {
            return None;
        }
        let every = if self.watched { OPEN_EVERY } else { CLOSED_EVERY };
        let current = self.model(target.workspace, target.base.as_deref());
        let recent = self.requested.get(&target.workspace).is_some_and(|at| now.duration_since(*at) < every);
        if current.is_some() && recent {
            return None;
        }
        let request = git::Request {
            dir: target.dir.clone(),
            mode: self.mode,
            base: target.base.clone(),
            last: current.and_then(|m| m.digest),
            previous: current.and_then(Model::diff).map(|d| d.files.clone()).unwrap_or_default(),
            expanded: None,
        };
        self.generation += 1;
        self.loading = Some((target.workspace, self.generation, now));
        self.requested.insert(target.workspace, now);
        Some((self.generation, request))
    }

    pub fn loaded(&mut self, workspace: u64, generation: u64, request: &git::Request, result: Result<git::Loaded>) {
        if self.loading.is_some_and(|(w, g, _)| (w, g) == (workspace, generation)) {
            self.loading = None;
        }
        let models = if request.expanded.is_some() { &mut self.summaries } else { &mut self.models };
        let requested = if request.expanded.is_some() { &self.summary_requested } else { &self.requested };
        if !requested.contains_key(&workspace) && !models.contains_key(&workspace) {
            return;
        }
        let now = Instant::now();
        let model = match result {
            Ok(git::Loaded { diff: None, base, digest }) => {
                if let Some(model) =
                    models.get_mut(&workspace).filter(|m| m.mode == request.mode && m.base == request.base)
                {
                    model.at = now;
                    model.label = base;
                    model.digest = Some(digest);
                }
                return;
            }
            Ok(loaded) => Model {
                mode: request.mode,
                base: request.base.clone(),
                label: loaded.base,
                result: Ok(Arc::new(loaded.diff.unwrap_or_default())),
                digest: Some(loaded.digest),
                at: now,
            },
            Err(e) => Model {
                mode: request.mode,
                base: request.base.clone(),
                label: request.base.clone(),
                result: Err(e.to_string()),
                digest: None,
                at: now,
            },
        };
        models.insert(workspace, model);
    }

    pub fn summary(&self, workspace: u64) -> Option<&Model> {
        self.summaries.get(&workspace)
    }

    pub fn set_scope(&mut self, scope: Scope) {
        if self.scope != scope {
            self.scope = scope;
            self.scroll = 0;
        }
    }

    pub fn request_every(&mut self, targets: &[(Checkout, bool)], now: Instant) -> Option<(u64, u64, git::Request)> {
        if self.busy(now) || targets.is_empty() {
            return None;
        }
        for offset in 0..targets.len() {
            let index = (self.next + offset) % targets.len();
            let (target, hidden) = &targets[index];
            let workspace = target.workspace;
            if self.summary_requested.get(&workspace).is_some_and(|at| now.duration_since(*at) < CLOSED_EVERY) {
                continue;
            }
            let current = self.summary(workspace);
            let previous = current.and_then(Model::diff).map(|d| d.files.clone()).unwrap_or_default();
            let query = filter::Query::parse(self.filter.as_ref().map_or("", |f| f.query()));
            let expanded = previous
                .iter()
                .filter(|file| !hidden && !self.folded(workspace, &Diff::default(), file) && query.keeps(file))
                .map(|file| file.path.clone())
                .collect();
            let request = git::Request {
                dir: target.dir.clone(),
                mode: Mode::Uncommitted,
                base: None,
                last: current.and_then(|m| m.digest),
                previous,
                expanded: Some(expanded),
            };
            self.generation += 1;
            self.loading = Some((workspace, self.generation, now));
            self.summary_requested.insert(workspace, now);
            self.next = (index + 1) % targets.len();
            return Some((workspace, self.generation, request));
        }
        None
    }

    pub fn invalidate_summary(&mut self, workspace: u64) {
        self.summary_requested.remove(&workspace);
    }

    fn busy(&self, now: Instant) -> bool {
        self.loading.is_some_and(|(_, _, at)| now.duration_since(at) < GIVE_UP_AFTER)
    }

    pub fn forget(&mut self, workspace: u64) {
        self.models.remove(&workspace);
        self.summaries.remove(&workspace);
        self.requested.remove(&workspace);
        self.summary_requested.remove(&workspace);
        self.folded.retain(|k, _| k.workspace != workspace);
        self.every_folded.retain(|k, _| k.workspace != workspace);
        self.viewed.retain(|k, _| k.workspace != workspace);
        self.gaps.retain(|(k, _, _), _| k.workspace != workspace);
        self.headers.remove(&workspace);
        if self.workspace == Some(workspace) {
            self.workspace = None;
        }
    }

    pub fn toggle_header(&mut self, id: u64) {
        if !self.headers.remove(&id) {
            self.headers.insert(id);
        }
    }

    pub fn label(&self, workspace: u64) -> Option<String> {
        self.models.get(&workspace).filter(|m| m.mode != Mode::Uncommitted).and_then(|m| m.label.clone())
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.scroll = 0;
        }
    }

    pub fn viewed(&self, workspace: u64, file: &File) -> bool {
        self.viewed.get(&key(workspace, file)) == Some(&file.viewed_digest)
    }

    pub fn toggle_viewed(&mut self, workspace: u64, file: &File) {
        let key = key(workspace, file);
        if self.viewed.get(&key) == Some(&file.viewed_digest) {
            self.viewed.remove(&key);
        } else {
            self.viewed.insert(key.clone(), file.viewed_digest);
        }
        self.folded.remove(&key);
        self.every_folded.remove(&key);
        self.invalidate_summary(workspace);
    }

    fn auto_folded(&self, workspace: u64, diff: &Diff, file: &File) -> bool {
        let big = diff.files.iter().map(|f| f.lines()).sum::<usize>() > AUTO_FOLD_LINES;
        let reviewed_change =
            self.viewed.get(&key(workspace, file)).is_some_and(|digest| *digest != file.viewed_digest);
        (self.scope == Scope::Every && !reviewed_change)
            || file.fold != diff::Fold::Open
            || self.viewed(workspace, file)
            || big
    }

    pub fn folded(&self, workspace: u64, diff: &Diff, file: &File) -> bool {
        if !file.fold.shows_lines() {
            return true;
        }
        self.folds().get(&key(workspace, file)).copied().unwrap_or_else(|| self.auto_folded(workspace, diff, file))
    }

    pub fn toggle_fold(&mut self, workspace: u64, diff: &Diff, file: &File) {
        let folded = self.folded(workspace, diff, file);
        self.folds_mut().insert(key(workspace, file), !folded);
        self.invalidate_summary(workspace);
    }

    pub fn close(&mut self) {
        if let Some(filter) = &mut self.filter {
            filter.focused = false;
        }
    }

    pub fn fold_all(&mut self, workspace: u64, diff: &Diff) {
        let fold = diff.files.iter().any(|f| !self.folded(workspace, diff, f));
        for file in &diff.files {
            self.folds_mut().insert(key(workspace, file), fold);
        }
    }

    fn folds(&self) -> &HashMap<FileKey, bool> {
        if self.scope == Scope::Every { &self.every_folded } else { &self.folded }
    }

    fn folds_mut(&mut self) -> &mut HashMap<FileKey, bool> {
        if self.scope == Scope::Every { &mut self.every_folded } else { &mut self.folded }
    }

    pub fn set_fold(&mut self, workspace: u64, file: &File, folded: bool) {
        self.folds_mut().insert(key(workspace, file), folded);
        self.invalidate_summary(workspace);
    }

    pub fn gap(&self, workspace: u64, file: &File, hunk: usize) -> Option<&Arc<Vec<GapLine>>> {
        self.gaps.get(&(key(workspace, file), file.digest, hunk))
    }

    pub fn set_gap(&mut self, workspace: u64, file: &File, hunk: usize, lines: Vec<GapLine>) {
        self.gaps.retain(|(k, digest, _), _| k.workspace != workspace || k.path != file.path || *digest == file.digest);
        self.gaps.insert((key(workspace, file), file.digest, hunk), Arc::new(lines));
    }
}

pub fn gap_lines(file: &File, hunk: usize, new_side: &[String]) -> Vec<GapLine> {
    let (Some(before), Some(after)) = (hunk.checked_sub(1).and_then(|h| file.hunks.get(h)), file.hunks.get(hunk))
    else {
        return Vec::new();
    };
    let offset = i64::from(before.old_end()) - i64::from(before.new_end());
    let range = before.new_end()..after.new_start;
    let texts: Vec<(u32, &str)> =
        range.filter_map(|n| new_side.get(usize::try_from(n).ok()?.checked_sub(1)?).map(|t| (n, t.as_str()))).collect();
    let code = texts.iter().map(|(_, t)| *t).collect::<Vec<_>>().join("\n");
    let highlighted = crate::highlight::highlight(&code, &diff::language(&file.path));
    texts
        .iter()
        .enumerate()
        .map(|(k, (n, text))| GapLine {
            old: u32::try_from(i64::from(*n) + offset).unwrap_or(0),
            new: *n,
            text: (*text).to_string(),
            syntax: highlighted.as_ref().map(|h| h.get(k).cloned().unwrap_or_default()),
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tints {
    pub removed: Option<Color>,
    pub added: Option<Color>,
    pub removed_word: Color,
    pub added_word: Color,
}

impl Tints {
    pub fn of(theme: &HostTheme) -> Self {
        let light = theme.is_light() == Some(true);
        let Some(bg) = theme.background.filter(|_| theme.truecolor) else {
            return if light {
                Self {
                    removed: Some(Color::Indexed(224)),
                    added: Some(Color::Indexed(194)),
                    removed_word: Color::Indexed(217),
                    added_word: Color::Indexed(157),
                }
            } else {
                Self { removed: None, added: None, removed_word: Color::Indexed(52), added_word: Color::Indexed(22) }
            };
        };
        let bg = (bg.r, bg.g, bg.b);
        let colour = |i: usize, fallback| theme.palette[i].map_or(fallback, |c| (c.r, c.g, c.b));
        let (red, green) = (colour(1, (205, 49, 49)), colour(2, (13, 188, 121)));
        let (line, word) = if light { ((0.12, 0.13), (0.28, 0.30)) } else { ((0.16, 0.14), (0.38, 0.34)) };
        Self {
            removed: Some(mix(bg, red, line.0)),
            added: Some(mix(bg, green, line.1)),
            removed_word: mix(bg, red, word.0),
            added_word: mix(bg, green, word.1),
        }
    }
}

fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> Color {
    let channel = |x: u8, y: u8| {
        let value = f32::from(x) + (f32::from(y) - f32::from(x)) * t;
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to 0..=255")]
        let value = value.round().clamp(0.0, 255.0) as u8;
        value
    };
    Color::Rgb(channel(a.0, b.0), channel(a.1, b.1), channel(a.2, b.2))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchPicker {
    pub workspace: u64,
    branches: Vec<String>,
    default: Option<String>,
    current: Option<String>,
    filter: String,
    cursor: Cursor,
}

impl BranchPicker {
    pub fn new(workspace: u64, branches: Vec<String>, default: Option<String>, current: Option<String>) -> Self {
        let mut picker =
            Self { workspace, branches, default, current, filter: String::new(), cursor: Cursor::default() };
        picker.cursor.selected = picker.current.as_ref().and_then(|c| picker.items().iter().position(|b| b == c));
        picker
    }

    pub fn items(&self) -> Vec<&str> {
        let filter = self.filter.to_lowercase();
        self.branches.iter().map(String::as_str).filter(|b| b.to_lowercase().contains(&filter)).collect()
    }

    pub fn tag(&self, branch: &str) -> Option<&'static str> {
        if self.current.as_deref() == Some(branch) {
            Some("current")
        } else if self.default.as_deref() == Some(branch) {
            Some("default")
        } else {
            None
        }
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn selected(&self) -> Option<usize> {
        self.cursor.selected
    }

    pub fn scroll(&self) -> usize {
        self.cursor.scroll
    }

    pub fn default(&self) -> Option<&str> {
        self.default.as_deref()
    }

    pub fn push(&mut self, c: char) {
        self.filter.push(c);
        self.reset();
    }

    pub fn pop(&mut self) {
        self.filter.pop();
        self.reset();
    }

    fn reset(&mut self) {
        let selected = (!self.items().is_empty() && !self.filter.is_empty()).then_some(0);
        self.cursor = Cursor { selected, scroll: 0 };
    }

    pub fn move_selection(&mut self, delta: isize, rows: usize) {
        let len = self.items().len();
        self.cursor.move_by(delta, len, rows);
    }

    pub fn scroll_by(&mut self, delta: isize, rows: usize) {
        let len = self.items().len();
        self.cursor.scroll_by(delta, len, rows);
    }

    pub fn chosen(&self, index: Option<usize>) -> Option<String> {
        let items = self.items();
        index
            .or(self.cursor.selected)
            .or_else(|| (items.len() == 1).then_some(0))
            .and_then(|i| items.get(i))
            .map(|b| (*b).to_string())
    }
}

#[cfg(test)]
mod tests;
