pub(crate) mod act;
pub(crate) mod diff;
pub(crate) mod discard;
pub(crate) mod every;
pub(crate) mod filter;
pub(crate) mod git;
mod porcelain;
pub(crate) mod selection;
pub(crate) mod stage;
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
use crate::text::stretch::Stretch;

pub(crate) const OPEN_EVERY: Duration = Duration::from_secs(1);
const CLOSED_EVERY: Duration = Duration::from_secs(3);
const LIVE_FOR: Duration = Duration::from_secs(5);
const GIVE_UP_AFTER: Duration = Duration::from_secs(30);
const AUTO_FOLD_LINES: usize = 500;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Mode {
    #[default]
    Uncommitted,
    Commits,
    All,
}

impl Mode {
    pub(crate) const ALL: [Self; 3] = [Self::Uncommitted, Self::Commits, Self::All];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Uncommitted => "uncommitted",
            Self::Commits => "commits",
            Self::All => "all",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Uncommitted => "Uncommitted",
            Self::Commits => "Commits",
            Self::All => "All",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Scope {
    #[default]
    Project,
    Every,
}

impl Scope {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Every => "every",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GapLine {
    pub(crate) old: u32,
    pub(crate) new: u32,
    pub(crate) text: String,
    pub(crate) raw: Option<Box<str>>,
    pub(crate) syntax: Option<Segments>,
}

impl GapLine {
    pub(crate) fn source(&self) -> &str {
        self.raw.as_deref().unwrap_or(&self.text)
    }
}

#[derive(Debug)]
pub(crate) struct Model {
    pub(crate) mode: Mode,
    pub(crate) base: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) result: std::result::Result<Arc<Diff>, String>,
    pub(crate) digest: Option<u64>,
    pub(crate) at: Instant,
}

impl Model {
    pub(crate) fn diff(&self) -> Option<&Arc<Diff>> {
        self.result.as_ref().ok()
    }

    pub(crate) fn live(&self, now: Instant) -> bool {
        self.result.is_ok() && now.duration_since(self.at) < LIVE_FOR
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FileKey {
    workspace: u64,
    path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Body {
    Loading,
    Failed(String),
    Ready(Arc<Diff>),
}

#[derive(Debug, Default)]
pub(crate) struct Panel {
    watched: bool,
    mode: Mode,
    scope: Scope,
    scroll: usize,
    workspace: Option<u64>,
    filter: Option<filter::Filter>,
    models: HashMap<(u64, Mode), Model>,
    followed: HashSet<(u64, Mode)>,
    summaries: HashMap<u64, Model>,
    every_folded: HashMap<FileKey, bool>,
    headers: HashSet<u64>,
    next: usize,
    summary_requested: HashMap<u64, Instant>,
    folded: HashMap<FileKey, bool>,
    viewed: HashMap<FileKey, u64>,
    gaps: HashMap<(FileKey, u64, usize), Arc<Vec<GapLine>>>,
    loading: Option<(u64, u64, Instant)>,
    generation: u64,
    superseded: HashMap<u64, u64>,
    requested: HashMap<(u64, Mode), Instant>,
    selection: Option<selection::Selection>,
}

pub(crate) struct Checkout {
    pub(crate) workspace: u64,
    pub(crate) dir: PathBuf,
    pub(crate) base: Option<String>,
}

fn key(workspace: u64, file: &File) -> FileKey {
    FileKey { workspace, path: file.path.clone() }
}

impl Panel {
    pub(crate) fn mode(&self) -> Mode {
        self.mode
    }

    pub(crate) fn scope(&self) -> Scope {
        self.scope
    }

    pub(crate) fn scroll(&self) -> usize {
        self.scroll
    }

    pub(crate) fn filter(&self) -> Option<&filter::Filter> {
        self.filter.as_ref()
    }

    pub(crate) fn set_watched(&mut self, watched: bool) {
        self.watched = watched;
    }

    pub(crate) fn show_workspace(&mut self, workspace: u64) {
        if self.workspace != Some(workspace) {
            self.workspace = Some(workspace);
            self.scroll = 0;
        }
    }

    pub(crate) fn scroll_by(&mut self, delta: isize, max: usize) {
        self.scroll = self.scroll.min(max).saturating_add_signed(delta).min(max);
    }

    pub(crate) fn selection(&self) -> Option<&selection::Selection> {
        self.selection.as_ref()
    }

    pub(crate) fn select(&mut self, selection: selection::Selection) {
        self.selection = Some(selection);
    }

    pub(crate) fn select_to(&mut self, end: selection::Spot) {
        if let Some(selection) = &mut self.selection {
            selection.end = end;
        }
    }

    pub(crate) fn set_stretch(&mut self, stretch: Option<Stretch<selection::Spot>>) {
        if let Some(selection) = &mut self.selection {
            selection.stretch = stretch;
        }
    }

    pub(crate) fn clear_selection(&mut self) {
        self.selection = None;
    }

    fn drop_selection(&mut self, workspace: u64, path: &str) {
        self.selection.take_if(|s| s.workspace == workspace && s.path == path);
    }

    pub(crate) fn scroll_to_top(&mut self) {
        self.scroll = 0;
    }

    pub(crate) fn focus_filter(&mut self) {
        self.filter.get_or_insert_default().focused = true;
    }

    pub(crate) fn clear_filter(&mut self) {
        self.filter = None;
    }

    pub(crate) fn push_filter(&mut self, c: char) {
        if let Some(filter) = &mut self.filter {
            filter.push(c);
        }
    }

    pub(crate) fn pop_filter(&mut self) {
        if let Some(filter) = &mut self.filter {
            filter.pop();
        }
    }

    pub(crate) fn unfocus(&mut self) {
        if let Some(filter) = &mut self.filter {
            filter.focused = false;
        }
    }

    pub(crate) fn unfold_headers(&mut self) {
        self.headers.clear();
    }

    pub(crate) fn forget_header(&mut self, id: u64) {
        self.headers.remove(&id);
    }

    pub(crate) fn header_folded(&self, id: u64) -> bool {
        self.headers.contains(&id)
    }

    pub(crate) fn model(&self, workspace: u64, base: Option<&str>) -> Option<&Model> {
        self.model_in(workspace, self.mode, base)
    }

    pub(crate) fn model_in(&self, workspace: u64, mode: Mode, base: Option<&str>) -> Option<&Model> {
        self.models.get(&(workspace, mode)).filter(|m| m.base.as_deref() == base)
    }

    pub(crate) fn request(&mut self, target: &Checkout, now: Instant) -> Option<(u64, git::Request)> {
        let every = if self.watched { OPEN_EVERY } else { CLOSED_EVERY };
        self.request_in(target, self.mode, every, now)
    }

    pub(crate) fn request_in(
        &mut self,
        target: &Checkout,
        mode: Mode,
        every: Duration,
        now: Instant,
    ) -> Option<(u64, git::Request)> {
        if self.busy(now) {
            return None;
        }
        let current = self.model_in(target.workspace, mode, target.base.as_deref());
        let recent = self.requested.get(&(target.workspace, mode)).is_some_and(|at| now.duration_since(*at) < every);
        if current.is_some() && recent {
            return None;
        }
        let request = git::Request {
            dir: target.dir.clone(),
            mode,
            base: target.base.clone(),
            last: current.and_then(|m| m.digest),
            previous: current.and_then(Model::diff).map(|d| d.files.clone()).unwrap_or_default(),
            expanded: None,
        };
        self.generation += 1;
        self.loading = Some((target.workspace, self.generation, now));
        self.requested.insert((target.workspace, mode), now);
        Some((self.generation, request))
    }

    pub(crate) fn request_followed(
        &mut self,
        wanted: &[(Checkout, Mode)],
        now: Instant,
    ) -> Option<(u64, u64, git::Request)> {
        if self.busy(now) {
            return None;
        }
        let due = wanted
            .iter()
            .filter(|(target, mode)| {
                let current = self.model_in(target.workspace, *mode, target.base.as_deref());
                let at = self.requested.get(&(target.workspace, *mode));
                current.is_none() || at.is_none_or(|at| now.duration_since(*at) >= OPEN_EVERY)
            })
            .min_by_key(|(target, mode)| self.requested.get(&(target.workspace, *mode)).copied())?;
        self.request_in(&due.0, due.1, OPEN_EVERY, now)
            .map(|(generation, request)| (due.0.workspace, generation, request))
    }

    pub(crate) fn follow(&mut self, wanted: HashSet<(u64, Mode)>) {
        let mode = self.mode;
        let gone = |key: &(u64, Mode)| key.1 != mode && !wanted.contains(key);
        self.models.retain(|key, _| !gone(key));
        self.requested.retain(|key, _| !gone(key));
        self.followed = wanted;
    }

    pub(crate) fn loaded(
        &mut self,
        workspace: u64,
        generation: u64,
        request: &git::Request,
        result: Result<git::Loaded>,
    ) {
        if self.loading.is_some_and(|(w, g, _)| (w, g) == (workspace, generation)) {
            self.loading = None;
        }
        if self.superseded.get(&workspace).is_some_and(|floor| generation <= *floor) {
            return;
        }
        let summary = request.expanded.is_some();
        let key = (workspace, request.mode);
        if !self.expects(key, request, summary) {
            return;
        }
        let current = if summary { self.summaries.get_mut(&workspace) } else { self.models.get_mut(&key) };
        let Some(model) = model_from(result, request, current, Instant::now()) else { return };
        if summary {
            self.summaries.insert(workspace, model);
        } else {
            self.models.insert(key, model);
            let (mode, followed) = (self.mode, &self.followed);
            self.models
                .retain(|&(w, m), _| w != workspace || m == request.mode || m == mode || followed.contains(&(w, m)));
        }
    }

    fn expects(&self, key: (u64, Mode), request: &git::Request, summary: bool) -> bool {
        let workspace = key.0;
        if !summary && request.mode != self.mode && !self.followed.contains(&key) {
            return false;
        }
        if summary {
            self.summary_requested.contains_key(&workspace) || self.summaries.contains_key(&workspace)
        } else {
            self.requested.contains_key(&key) || self.models.contains_key(&key)
        }
    }

    pub(crate) fn summary(&self, workspace: u64) -> Option<&Model> {
        self.summaries.get(&workspace)
    }

    pub(crate) fn set_scope(&mut self, scope: Scope) {
        if self.scope != scope {
            self.scope = scope;
            self.scroll = 0;
        }
    }

    pub(crate) fn request_every(
        &mut self,
        targets: &[(Checkout, bool)],
        now: Instant,
    ) -> Option<(u64, u64, git::Request)> {
        self.request_sweep(targets, now, true)
    }

    pub(crate) fn request_sweep(
        &mut self,
        targets: &[(Checkout, bool)],
        now: Instant,
        expand: bool,
    ) -> Option<(u64, u64, git::Request)> {
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
                .filter(|file| {
                    expand && !hidden && !self.folded(workspace, &Diff::default(), file) && query.keeps(file)
                })
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

    pub(crate) fn invalidate_summary(&mut self, workspace: u64) {
        self.summary_requested.remove(&workspace);
    }

    fn busy(&self, now: Instant) -> bool {
        self.loading.is_some_and(|(_, _, at)| now.duration_since(at) < GIVE_UP_AFTER)
    }

    pub(crate) fn forget(&mut self, workspace: u64) {
        self.models.retain(|(w, _), _| *w != workspace);
        self.summaries.remove(&workspace);
        self.requested.retain(|(w, _), _| *w != workspace);
        self.summary_requested.remove(&workspace);
        self.folded.retain(|k, _| k.workspace != workspace);
        self.every_folded.retain(|k, _| k.workspace != workspace);
        self.viewed.retain(|k, _| k.workspace != workspace);
        self.gaps.retain(|(k, _, _), _| k.workspace != workspace);
        self.headers.remove(&workspace);
        self.superseded.remove(&workspace);
        self.selection.take_if(|s| s.workspace == workspace);
        if self.workspace == Some(workspace) {
            self.workspace = None;
        }
    }

    pub(crate) fn toggle_header(&mut self, id: u64) {
        if !self.headers.remove(&id) {
            self.headers.insert(id);
        }
    }

    pub(crate) fn label(&self, workspace: u64) -> Option<String> {
        self.label_in(workspace, self.mode)
    }

    pub(crate) fn label_in(&self, workspace: u64, mode: Mode) -> Option<String> {
        self.models.get(&(workspace, mode)).filter(|m| m.mode != Mode::Uncommitted).and_then(|m| m.label.clone())
    }

    pub(crate) fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.mode = mode;
            self.scroll = 0;
        }
    }

    pub(crate) fn viewed(&self, workspace: u64, file: &File) -> bool {
        self.viewed.get(&key(workspace, file)) == Some(&file.viewed_digest)
    }

    pub(crate) fn toggle_viewed(&mut self, workspace: u64, file: &File) {
        let digest = (!self.viewed(workspace, file)).then_some(file.viewed_digest);
        self.set_viewed(workspace, &file.path, digest);
    }

    pub(crate) fn set_viewed(&mut self, workspace: u64, path: &str, digest: Option<u64>) {
        let key = FileKey { workspace, path: path.to_string() };
        match digest {
            Some(digest) => drop(self.viewed.insert(key.clone(), digest)),
            None => drop(self.viewed.remove(&key)),
        }
        self.folded.remove(&key);
        self.every_folded.remove(&key);
        self.drop_selection(workspace, path);
        self.invalidate_summary(workspace);
    }

    pub(crate) fn discarded(&mut self, workspace: u64, path: &str) {
        self.set_viewed(workspace, path, None);
        self.gaps.retain(|(k, _, _), _| k.workspace != workspace || k.path != path);
        self.rescan(workspace);
    }

    pub(crate) fn rescan(&mut self, workspace: u64) {
        self.invalidate_summary(workspace);
        self.requested.retain(|(w, _), _| *w != workspace);
        self.superseded.insert(workspace, self.generation);
        if self.loading.is_some_and(|(w, _, _)| w == workspace) {
            self.loading = None;
        }
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

    pub(crate) fn folded(&self, workspace: u64, diff: &Diff, file: &File) -> bool {
        if !file.fold.shows_lines() {
            return true;
        }
        self.folds().get(&key(workspace, file)).copied().unwrap_or_else(|| self.auto_folded(workspace, diff, file))
    }

    pub(crate) fn toggle_fold(&mut self, workspace: u64, diff: &Diff, file: &File) {
        let folded = self.folded(workspace, diff, file);
        self.folds_mut().insert(key(workspace, file), !folded);
        self.drop_selection(workspace, &file.path);
        self.invalidate_summary(workspace);
    }

    pub(crate) fn close(&mut self) {
        self.unfocus();
    }

    #[cfg(test)]
    pub(crate) fn fold_all(&mut self, workspace: u64, diff: &Diff) {
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

    pub(crate) fn set_fold(&mut self, workspace: u64, file: &File, folded: bool) {
        self.folds_mut().insert(key(workspace, file), folded);
        self.drop_selection(workspace, &file.path);
        self.invalidate_summary(workspace);
    }

    pub(crate) fn gap(&self, workspace: u64, file: &File, hunk: usize) -> Option<&Arc<Vec<GapLine>>> {
        self.gaps.get(&(key(workspace, file), file.digest, hunk))
    }

    pub(crate) fn set_gap(&mut self, workspace: u64, file: &File, hunk: usize, lines: Vec<GapLine>) {
        self.gaps.retain(|(k, digest, _), _| k.workspace != workspace || k.path != file.path || *digest == file.digest);
        self.gaps.insert((key(workspace, file), file.digest, hunk), Arc::new(lines));
    }
}

fn model_from(
    result: Result<git::Loaded>,
    request: &git::Request,
    current: Option<&mut Model>,
    now: Instant,
) -> Option<Model> {
    match result {
        Ok(git::Loaded { diff: None, base, digest }) => {
            if let Some(model) = current.filter(|m| m.mode == request.mode && m.base == request.base) {
                model.at = now;
                model.label = base;
                model.digest = Some(digest);
            }
            None
        }
        Ok(loaded) => Some(Model {
            mode: request.mode,
            base: request.base.clone(),
            label: loaded.base,
            result: Ok(Arc::new(loaded.diff.unwrap_or_default())),
            digest: Some(loaded.digest),
            at: now,
        }),
        Err(e) => Some(Model {
            mode: request.mode,
            base: request.base.clone(),
            label: request.base.clone(),
            result: Err(e.to_string()),
            digest: None,
            at: now,
        }),
    }
}

pub(crate) fn gap(dir: &std::path::Path, mode: Mode, file: &File, hunk: usize) -> Option<Vec<GapLine>> {
    git::new_side(dir, mode, &file.path).map(|side| gap_lines(file, hunk, &side))
}

pub(crate) fn gap_lines(file: &File, hunk: usize, new_side: &[String]) -> Vec<GapLine> {
    let (Some(before), Some(after)) = (hunk.checked_sub(1).and_then(|h| file.hunks.get(h)), file.hunks.get(hunk))
    else {
        return Vec::new();
    };
    let offset = i64::from(before.old_end()) - i64::from(before.new_end());
    let range = before.new_end()..after.new_start;
    let texts: Vec<(u32, &str)> =
        range.filter_map(|n| new_side.get(usize::try_from(n).ok()?.checked_sub(1)?).map(|t| (n, t.as_str()))).collect();
    let code = texts.iter().map(|(_, t)| diff::expand(t)).collect::<Vec<_>>().join("\n");
    let highlighted = crate::highlight::highlight(&code, &diff::language(&file.path));
    texts
        .iter()
        .enumerate()
        .map(|(k, (n, text))| GapLine {
            old: u32::try_from(i64::from(*n) + offset).unwrap_or(0),
            new: *n,
            text: diff::expand(text),
            raw: diff::own(text),
            syntax: highlighted.as_ref().map(|h| h.get(k).cloned().unwrap_or_default()),
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tints {
    pub(crate) removed: Option<Color>,
    pub(crate) added: Option<Color>,
    pub(crate) removed_word: Color,
    pub(crate) added_word: Color,
}

impl Tints {
    pub(crate) fn of(theme: &HostTheme) -> Self {
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
pub(crate) struct BranchPicker {
    pub(crate) workspace: u64,
    branches: Vec<String>,
    default: Option<String>,
    current: Option<String>,
    filter: String,
    cursor: Cursor,
}

impl BranchPicker {
    pub(crate) fn new(workspace: u64, branches: Vec<String>, default: Option<String>, current: Option<String>) -> Self {
        let mut picker =
            Self { workspace, branches, default, current, filter: String::new(), cursor: Cursor::default() };
        picker.cursor.selected = picker.current.as_ref().and_then(|c| picker.items().iter().position(|b| b == c));
        picker
    }

    pub(crate) fn items(&self) -> Vec<&str> {
        let filter = self.filter.to_lowercase();
        self.branches.iter().map(String::as_str).filter(|b| b.to_lowercase().contains(&filter)).collect()
    }

    pub(crate) fn tag(&self, branch: &str) -> Option<&'static str> {
        if self.current.as_deref() == Some(branch) {
            Some("current")
        } else if self.default.as_deref() == Some(branch) {
            Some("default")
        } else {
            None
        }
    }

    pub(crate) fn filter(&self) -> &str {
        &self.filter
    }

    pub(crate) fn selected(&self) -> Option<usize> {
        self.cursor.selected
    }

    pub(crate) fn scroll(&self) -> usize {
        self.cursor.scroll
    }

    pub(crate) fn default(&self) -> Option<&str> {
        self.default.as_deref()
    }

    pub(crate) fn push(&mut self, c: char) {
        self.filter.push(c);
        self.reset();
    }

    pub(crate) fn pop(&mut self) {
        self.filter.pop();
        self.reset();
    }

    fn reset(&mut self) {
        let selected = (!self.items().is_empty() && !self.filter.is_empty()).then_some(0);
        self.cursor = Cursor { selected, scroll: 0 };
    }

    pub(crate) fn move_selection(&mut self, delta: isize, rows: usize) {
        let len = self.items().len();
        self.cursor.move_by(delta, len, rows);
    }

    pub(crate) fn scroll_by(&mut self, delta: isize, rows: usize) {
        let len = self.items().len();
        self.cursor.scroll_by(delta, len, rows);
    }

    pub(crate) fn chosen(&self, index: Option<usize>) -> Option<String> {
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
