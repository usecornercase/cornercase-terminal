pub mod diff;
pub mod filter;
pub mod git;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use self::diff::{Diff, File, Segments};
use crate::error::Result;
use crate::host_theme::HostTheme;
use crate::picker::Cursor;

const OPEN_EVERY: Duration = Duration::from_secs(1);
const CLOSED_EVERY: Duration = Duration::from_secs(3);
const LIVE_FOR: Duration = Duration::from_secs(5);
const AUTO_FOLD_LINES: usize = 500;
const GIVE_UP_AFTER: Duration = Duration::from_secs(30);

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
            Self::Uncommitted => "uncommitted",
            Self::Commits => "commits",
            Self::All => "all",
        }
    }
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

#[derive(Debug, Default)]
pub struct Panel {
    pub open: bool,
    pub watched: bool,
    pub mode: Mode,
    pub scroll: usize,
    pub workspace: Option<u64>,
    pub filter: Option<filter::Filter>,
    models: HashMap<u64, Model>,
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
        if self.loading.is_some_and(|(_, _, at)| now.duration_since(at) < GIVE_UP_AFTER) {
            return None;
        }
        let every = if self.open || self.watched { OPEN_EVERY } else { CLOSED_EVERY };
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
        let now = Instant::now();
        let model = match result {
            Ok(git::Loaded { diff: None, base, digest }) => {
                if let Some(model) =
                    self.models.get_mut(&workspace).filter(|m| m.mode == request.mode && m.base == request.base)
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
        self.models.insert(workspace, model);
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
        self.viewed.get(&key(workspace, file)) == Some(&file.digest)
    }

    pub fn toggle_viewed(&mut self, workspace: u64, file: &File) {
        let key = key(workspace, file);
        if self.viewed.get(&key) == Some(&file.digest) {
            self.viewed.remove(&key);
        } else {
            self.viewed.insert(key.clone(), file.digest);
        }
        self.folded.remove(&key);
    }

    fn auto_folded(&self, workspace: u64, diff: &Diff, file: &File) -> bool {
        let big = diff.files.iter().map(|f| f.lines()).sum::<usize>() > AUTO_FOLD_LINES;
        file.fold != diff::Fold::Open || self.viewed(workspace, file) || big
    }

    pub fn folded(&self, workspace: u64, diff: &Diff, file: &File) -> bool {
        if !file.fold.shows_lines() {
            return true;
        }
        self.folded.get(&key(workspace, file)).copied().unwrap_or_else(|| self.auto_folded(workspace, diff, file))
    }

    pub fn toggle_fold(&mut self, workspace: u64, diff: &Diff, file: &File) {
        let folded = self.folded(workspace, diff, file);
        self.folded.insert(key(workspace, file), !folded);
    }

    pub fn close(&mut self) {
        self.open = false;
        if let Some(filter) = &mut self.filter {
            filter.focused = false;
        }
    }

    pub fn fold_all(&mut self, workspace: u64, diff: &Diff) {
        let fold = diff.files.iter().any(|f| !self.folded(workspace, diff, f));
        for file in &diff.files {
            self.folded.insert(key(workspace, file), fold);
        }
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
mod tests {
    use super::*;
    use crate::changes::diff::{Fold, Hunk, Kind, Line, Status};

    fn file(path: &str, digest: u64, lines: usize) -> File {
        let line = |n| Line {
            kind: Kind::Added,
            old: None,
            new: Some(n),
            text: String::new(),
            syntax: None,
            emphasis: Vec::new(),
        };
        let lines: Vec<Line> = (1..=u32::try_from(lines).expect("lines")).map(line).collect();
        File {
            path: path.into(),
            old_path: None,
            status: Status::Modified,
            added: lines.len(),
            removed: 0,
            fold: Fold::Open,
            hunks: vec![Hunk { old_start: 1, new_start: 1, context: String::new(), lines }],
            digest,
        }
    }

    fn diff(files: Vec<File>) -> Diff {
        Diff { files: files.into_iter().map(Arc::new).collect() }
    }

    fn target(workspace: u64) -> Checkout {
        Checkout { workspace, dir: PathBuf::from("/repo"), base: None }
    }

    fn loaded(diff: Option<Diff>, digest: u64) -> git::Loaded {
        git::Loaded { base: None, digest, diff }
    }

    mod refresh {
        use super::*;

        #[test]
        fn one_request_at_a_time() {
            let mut panel = Panel::default();
            let now = Instant::now();
            assert!(panel.request(&target(1), now).is_some());
            assert!(panel.request(&target(1), now + Duration::from_secs(10)).is_none());
        }

        #[test]
        fn waits_a_second_while_open_and_three_while_closed() {
            let mut panel = Panel { open: true, ..Panel::default() };
            let now = Instant::now();
            let (generation, request) = panel.request(&target(1), now).expect("first");
            panel.loaded(1, generation, &request, Ok(loaded(Some(Diff::default()), 7)));
            assert!(panel.request(&target(1), now + Duration::from_millis(500)).is_none());
            let (generation, request) = panel.request(&target(1), now + OPEN_EVERY).expect("open");
            panel.loaded(1, generation, &request, Ok(loaded(None, 7)));
            panel.open = false;
            assert!(panel.request(&target(1), now + OPEN_EVERY * 2).is_none());
            assert!(panel.request(&target(1), now + OPEN_EVERY + CLOSED_EVERY).is_some());
        }

        #[test]
        fn passes_the_last_digest_and_files_along() {
            let mut panel = Panel::default();
            let now = Instant::now();
            let (generation, request) = panel.request(&target(1), now).expect("first");
            panel.loaded(1, generation, &request, Ok(loaded(Some(diff(vec![file("a", 1, 1)])), 42)));
            let (_, request) = panel.request(&target(1), now + CLOSED_EVERY).expect("again");
            assert_eq!((request.last, request.previous.len()), (Some(42), 1));
        }

        #[test]
        fn a_new_mode_asks_at_once() {
            let mut panel = Panel::default();
            let now = Instant::now();
            let (generation, request) = panel.request(&target(1), now).expect("first");
            panel.loaded(1, generation, &request, Ok(loaded(Some(Diff::default()), 1)));
            panel.set_mode(Mode::Commits);
            let (_, request) = panel.request(&target(1), now).expect("new mode");
            assert_eq!((request.mode, request.last), (Mode::Commits, None));
        }

        #[test]
        fn an_old_answer_does_not_replace_a_newer_mode() {
            let mut panel = Panel::default();
            let (generation, request) = panel.request(&target(1), Instant::now()).expect("first");
            panel.set_mode(Mode::All);
            panel.loaded(1, generation, &request, Ok(loaded(Some(Diff::default()), 1)));
            assert!(panel.model(1, None).is_none());
        }

        #[test]
        fn a_job_that_never_answers_is_given_up() {
            let mut panel = Panel::default();
            let now = Instant::now();
            panel.request(&target(1), now).expect("first");
            assert!(panel.request(&target(1), now + GIVE_UP_AFTER).is_some());
        }

        #[test]
        fn an_unchanged_answer_without_a_model_is_ignored() {
            let mut panel = Panel::default();
            let (generation, request) = panel.request(&target(1), Instant::now()).expect("first");
            panel.loaded(1, generation, &request, Ok(loaded(None, 1)));
            assert!(panel.model(1, None).is_none());
        }

        #[test]
        fn an_error_is_kept_as_the_model() {
            let mut panel = Panel::default();
            let (generation, request) = panel.request(&target(1), Instant::now()).expect("first");
            panel.loaded(1, generation, &request, Err(crate::error::Error::Git("boom".into())));
            assert_eq!(panel.model(1, None).map(|m| m.result.clone().err()), Some(Some("boom".into())));
        }
    }

    mod folding {
        use super::*;

        #[test]
        fn small_diffs_start_open() {
            let d = diff(vec![file("a", 1, 3)]);
            assert!(!Panel::default().folded(1, &d, &d.files[0]));
        }

        #[test]
        fn big_diffs_start_folded() {
            let d = diff(vec![file("a", 1, AUTO_FOLD_LINES + 1)]);
            assert!(Panel::default().folded(1, &d, &d.files[0]));
        }

        #[test]
        fn a_viewed_file_folds_until_it_changes() {
            let d = diff(vec![file("a", 1, 3)]);
            let mut panel = Panel::default();
            panel.toggle_viewed(1, &d.files[0]);
            let changed = diff(vec![file("a", 2, 3)]);
            assert_eq!((panel.folded(1, &d, &d.files[0]), panel.folded(1, &changed, &changed.files[0])), (true, false));
        }

        #[test]
        fn fold_all_folds_then_unfolds() {
            let d = diff(vec![file("a", 1, 3), file("b", 2, 3)]);
            let mut panel = Panel::default();
            panel.fold_all(1, &d);
            let folded = |panel: &Panel| d.files.iter().map(|f| panel.folded(1, &d, f)).collect::<Vec<_>>();
            assert_eq!(folded(&panel), [true, true]);
            panel.fold_all(1, &d);
            assert_eq!(folded(&panel), [false, false]);
        }

        #[test]
        fn binary_files_never_open() {
            let d = diff(vec![File { fold: Fold::Binary, ..file("a", 1, 0) }]);
            let mut panel = Panel::default();
            panel.toggle_fold(1, &d, &d.files[0]);
            assert!(panel.folded(1, &d, &d.files[0]));
        }
    }

    #[test]
    fn gap_lines_come_from_the_new_side_between_two_hunks() {
        let mut f = file("a.txt", 1, 0);
        let line = |kind, old, new| Line { kind, old, new, text: "x".into(), syntax: None, emphasis: Vec::new() };
        f.hunks = vec![
            Hunk { old_start: 1, new_start: 1, context: String::new(), lines: vec![line(Kind::Added, None, Some(1))] },
            Hunk {
                old_start: 4,
                new_start: 5,
                context: String::new(),
                lines: vec![line(Kind::Context, Some(4), Some(5))],
            },
        ];
        let new_side: Vec<String> = (1..=6).map(|n| format!("line {n}")).collect();
        let gap: Vec<(u32, u32, String)> =
            gap_lines(&f, 1, &new_side).into_iter().map(|g| (g.old, g.new, g.text)).collect();
        assert_eq!(gap, [(1, 2, "line 2".into()), (2, 3, "line 3".into()), (3, 4, "line 4".into())]);
    }

    mod branch_picker {
        use super::*;

        fn picker() -> BranchPicker {
            let branches = ["main", "feature/login", "origin/main"].map(String::from).to_vec();
            BranchPicker::new(1, branches, Some("origin/main".into()), Some("main".into()))
        }

        #[test]
        fn starts_on_the_current_base() {
            assert_eq!(picker().chosen(None).as_deref(), Some("main"));
        }

        #[test]
        fn filters_as_you_type() {
            let mut p = picker();
            "orig".chars().for_each(|c| p.push(c));
            assert_eq!((p.items(), p.chosen(None).as_deref()), (vec!["origin/main"], Some("origin/main")));
        }

        #[test]
        fn tags_the_current_and_default_branches() {
            let p = picker();
            assert_eq!(
                (p.tag("main"), p.tag("origin/main"), p.tag("feature/login")),
                (Some("current"), Some("default"), None)
            );
        }
    }

    #[test]
    fn tints_use_the_palette_without_truecolor() {
        let tints = Tints::of(&HostTheme::default());
        assert_eq!((tints.removed, tints.removed_word), (None, Color::Indexed(52)));
    }

    #[test]
    fn tints_blend_with_the_background_in_truecolor() {
        let background = Some(libghostty_vt::style::RgbColor { r: 0, g: 0, b: 0 });
        let tints = Tints::of(&HostTheme { background, truecolor: true, ..HostTheme::default() });
        assert_eq!(tints.added, Some(Color::Rgb(2, 26, 17)));
    }
}
