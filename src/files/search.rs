use std::num::NonZero;
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;

use grep_matcher::Matcher;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::sinks::Lossy;
use grep_searcher::{BinaryDetection, SearcherBuilder};
use ignore::{WalkBuilder, WalkState};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher as Fuzzy, Utf32Str};
use parking_lot::Mutex;

use crate::highlight::syntax;

pub(crate) const MAX_FILES: usize = 200_000;
pub(crate) const MAX_NAMES: usize = 200;
pub(crate) const MAX_MATCHES: usize = 1000;
const MAX_LINE: usize = 400;
const MAX_THREADS: usize = 8;

pub(crate) fn index(root: &Path) -> Vec<String> {
    let (tx, rx) = mpsc::channel();
    let count = AtomicUsize::new(0);
    let walk = WalkBuilder::new(root).hidden(false).filter_entry(|e| e.file_name() != ".git").build_parallel();
    walk.run(|| {
        let (tx, count) = (tx.clone(), &count);
        Box::new(move |entry| {
            let Ok(entry) = entry else { return WalkState::Continue };
            if !entry.path().is_file() {
                return WalkState::Continue;
            }
            if let Ok(path) = entry.path().strip_prefix(root) {
                let _ = tx.send(path.to_string_lossy().into_owned());
            }
            if count.fetch_add(1, Ordering::Relaxed) + 1 >= MAX_FILES { WalkState::Quit } else { WalkState::Continue }
        })
    });
    drop(tx);
    let mut paths: Vec<String> = rx.into_iter().collect();
    paths.sort_unstable();
    paths
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Name {
    pub(crate) path: String,
    pub(crate) indices: Vec<u32>,
}

pub(crate) fn names(paths: &[String], query: &str) -> Vec<Name> {
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut matcher = Fuzzy::new(Config::DEFAULT.match_paths());
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, usize)> = paths
        .iter()
        .enumerate()
        .filter_map(|(i, path)| pattern.score(Utf32Str::new(path, &mut buf), &mut matcher).map(|score| (score, i)))
        .collect();
    let file_name = |i: usize| paths[i].rsplit('/').next().map_or(0, str::len);
    scored.sort_unstable_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| file_name(a.1).cmp(&file_name(b.1)))
            .then_with(|| paths[a.1].len().cmp(&paths[b.1].len()))
            .then_with(|| a.1.cmp(&b.1))
    });
    scored.truncate(MAX_NAMES);
    scored
        .into_iter()
        .map(|(_, i)| {
            let mut indices = Vec::new();
            pattern.indices(Utf32Str::new(&paths[i], &mut buf), &mut matcher, &mut indices);
            indices.sort_unstable();
            indices.dedup();
            Name { path: paths[i].clone(), indices }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Line {
    pub(crate) number: u32,
    pub(crate) text: String,
    pub(crate) ranges: Vec<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) path: String,
    pub(crate) lines: Vec<Line>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Text {
    pub(crate) files: Vec<Found>,
    pub(crate) matches: usize,
    pub(crate) capped: bool,
}

pub(super) fn matcher(query: &str) -> Option<RegexMatcher> {
    RegexMatcherBuilder::new().case_smart(true).fixed_strings(true).build(query).ok()
}

fn chars_before(text: &str, byte: usize) -> usize {
    text.get(..byte).map_or(0, |head| head.chars().count())
}

fn line(matcher: &RegexMatcher, number: u64, raw: &str) -> Line {
    let expanded = syntax::expand(raw.trim_end_matches(['\n', '\r']));
    let text: String = expanded.trim_start().chars().take(MAX_LINE).collect();
    let mut ranges = Vec::new();
    let _ = matcher.find_iter(text.as_bytes(), |m| {
        ranges.push(chars_before(&text, m.start())..chars_before(&text, m.end()));
        true
    });
    Line { number: u32::try_from(number).unwrap_or(u32::MAX), text, ranges }
}

pub(crate) fn text(root: &Path, paths: &[String], query: &str, cancel: &AtomicBool) -> Text {
    let threads = std::thread::available_parallelism().map_or(4, NonZero::get).min(MAX_THREADS);
    text_on(root, paths, query, cancel, threads)
}

struct Scan<'a> {
    root: &'a Path,
    paths: &'a [String],
    cancel: &'a AtomicBool,
    next: AtomicUsize,
    seen: AtomicUsize,
    found: Mutex<Vec<(usize, Found)>>,
}

impl Scan<'_> {
    fn work(&self, finder: &RegexMatcher) {
        let mut searcher = SearcherBuilder::new().binary_detection(BinaryDetection::quit(0)).line_number(true).build();
        while !self.cancel.load(Ordering::Relaxed) && self.seen.load(Ordering::Relaxed) <= MAX_MATCHES {
            let i = self.next.fetch_add(1, Ordering::Relaxed);
            let Some(path) = self.paths.get(i) else { break };
            let mut lines = Vec::new();
            let sink = Lossy(|number, raw| {
                let kept = self.seen.fetch_add(1, Ordering::Relaxed) < MAX_MATCHES;
                let _ = kept.then(|| lines.push(line(finder, number, raw)));
                Ok(kept)
            });
            let _ = searcher.search_path(finder, self.root.join(path), sink);
            if !lines.is_empty() {
                self.found.lock().push((i, Found { path: path.clone(), lines }));
            }
        }
    }
}

fn text_on(root: &Path, paths: &[String], query: &str, cancel: &AtomicBool, threads: usize) -> Text {
    let Some(finder) = matcher(query) else { return Text::default() };
    let scan = Scan {
        root,
        paths,
        cancel,
        next: AtomicUsize::new(0),
        seen: AtomicUsize::new(0),
        found: Mutex::new(Vec::new()),
    };
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| scan.work(&finder));
        }
    });
    let (found, seen) = (scan.found, scan.seen);
    let mut files = found.into_inner();
    files.sort_unstable_by_key(|(i, _)| *i);
    let files: Vec<Found> = files.into_iter().map(|(_, f)| f).collect();
    let matches = files.iter().map(|f| f.lines.len()).sum();
    Text { files, matches, capped: seen.into_inner() > MAX_MATCHES }
}

#[cfg(test)]
mod tests;
