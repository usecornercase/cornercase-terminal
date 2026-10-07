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

use crate::syntax;

pub const MAX_FILES: usize = 200_000;
pub const MAX_NAMES: usize = 200;
pub const MAX_MATCHES: usize = 1000;
const MAX_LINE: usize = 400;
const MAX_THREADS: usize = 8;

pub fn index(root: &Path) -> Vec<String> {
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
pub struct Name {
    pub path: String,
    pub indices: Vec<u32>,
}

pub fn names(paths: &[String], query: &str) -> Vec<Name> {
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut matcher = Fuzzy::new(Config::DEFAULT.match_paths());
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, usize)> = paths
        .iter()
        .enumerate()
        .filter_map(|(i, path)| pattern.score(Utf32Str::new(path, &mut buf), &mut matcher).map(|score| (score, i)))
        .collect();
    scored.sort_unstable_by(|a, b| {
        b.0.cmp(&a.0).then_with(|| paths[a.1].len().cmp(&paths[b.1].len())).then_with(|| a.1.cmp(&b.1))
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
pub struct Line {
    pub number: u32,
    pub text: String,
    pub ranges: Vec<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: String,
    pub lines: Vec<Line>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Text {
    pub files: Vec<Found>,
    pub matches: usize,
    pub capped: bool,
}

fn matcher(query: &str) -> Option<RegexMatcher> {
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

pub fn text(root: &Path, paths: &[String], query: &str, cancel: &AtomicBool) -> Text {
    let threads = std::thread::available_parallelism().map_or(4, NonZero::get).min(MAX_THREADS);
    text_on(root, paths, query, cancel, threads)
}

fn text_on(root: &Path, paths: &[String], query: &str, cancel: &AtomicBool, threads: usize) -> Text {
    let Some(finder) = matcher(query) else { return Text::default() };
    let (next, seen) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let found = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                let mut searcher =
                    SearcherBuilder::new().binary_detection(BinaryDetection::quit(0)).line_number(true).build();
                while !cancel.load(Ordering::Relaxed) && seen.load(Ordering::Relaxed) <= MAX_MATCHES {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = paths.get(i) else { break };
                    let mut lines = Vec::new();
                    let sink = Lossy(|number, raw| {
                        let kept = seen.fetch_add(1, Ordering::Relaxed) < MAX_MATCHES;
                        if kept {
                            lines.push(line(&finder, number, raw));
                        }
                        Ok(kept)
                    });
                    let _ = searcher.search_path(&finder, root.join(path), sink);
                    if !lines.is_empty() {
                        found.lock().push((i, Found { path: path.clone(), lines }));
                    }
                }
            });
        }
    });
    let mut files = found.into_inner();
    files.sort_unstable_by_key(|(i, _)| *i);
    let files: Vec<Found> = files.into_iter().map(|(_, f)| f).collect();
    let matches = files.iter().map(|f| f.lines.len()).sum();
    Text { files, matches, capped: seen.into_inner() > MAX_MATCHES }
}

pub fn occurrences(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    let fold = !query.chars().any(char::is_uppercase);
    let norm = |c: char| if fold { c.to_lowercase().next().unwrap_or(c) } else { c };
    let needle: Vec<char> = query.chars().map(norm).collect();
    let hay: Vec<char> = text.chars().map(norm).collect();
    let mut ranges = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i..i + needle.len()] == needle[..] {
            ranges.push(i..i + needle.len());
            i += needle.len();
        } else {
            i += 1;
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::test_util::git_repo;

    fn paths(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_string()).collect()
    }

    #[test]
    fn indexes_the_files_git_does_not_ignore() {
        let repo = git_repo(&[(".gitignore", "target/\n"), ("src/main.rs", ""), ("README.md", "")]);
        fs::create_dir_all(repo.path().join("target/debug")).expect("mkdir");
        fs::write(repo.path().join("target/debug/app"), "").expect("write");
        assert_eq!(index(repo.path()), [".gitignore", "README.md", "src/main.rs"]);
    }

    #[test]
    fn finds_names_by_their_letters_best_first() {
        let paths = paths(&["src/relay/pty-handler.test.ts", "src/relay/pty-handler.ts", "docs/handbook.md"]);
        let found: Vec<String> = names(&paths, "ptyhandler").into_iter().map(|n| n.path).collect();
        assert_eq!(found, ["src/relay/pty-handler.ts", "src/relay/pty-handler.test.ts"]);
    }

    #[test]
    fn marks_the_letters_that_matched() {
        let found = names(&paths(&["src/main.rs"]), "main");
        assert_eq!(found[0].indices, [4, 5, 6, 7]);
    }

    #[test]
    fn finds_text_with_its_line_and_where_it_matched() {
        let repo = git_repo(&[("a.rs", "fn main() {\n    run_all();\n}\n"), ("b.md", "nothing")]);
        let text = text(repo.path(), &paths(&["a.rs", "b.md"]), "run", &AtomicBool::new(false));
        assert_eq!(text.matches, 1);
        assert_eq!(text.files[0].path, "a.rs");
        let ranges = std::iter::once(0..3).collect();
        assert_eq!(text.files[0].lines, [Line { number: 2, text: "run_all();".into(), ranges }]);
    }

    #[test]
    fn lower_case_finds_any_case_and_upper_case_only_itself() {
        let repo = git_repo(&[("a.txt", "Order\norder\n")]);
        let count = |q| text(repo.path(), &paths(&["a.txt"]), q, &AtomicBool::new(false)).matches;
        assert_eq!((count("order"), count("Order")), (2, 1));
    }

    #[test]
    fn skips_binary_files() {
        let repo = git_repo(&[("a.txt", "needle\n")]);
        fs::write(repo.path().join("b.bin"), b"needle\0\x01").expect("write");
        let text = text(repo.path(), &paths(&["a.txt", "b.bin"]), "needle", &AtomicBool::new(false));
        assert_eq!(text.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["a.txt"]);
    }

    #[test]
    fn stops_at_the_cap() {
        let repo = git_repo(&[("a.txt", &"x\n".repeat(MAX_MATCHES + 50))]);
        let text = text(repo.path(), &paths(&["a.txt"]), "x", &AtomicBool::new(false));
        assert_eq!((text.matches, text.capped), (MAX_MATCHES, true));
    }

    #[test]
    fn exactly_the_cap_is_not_cut() {
        let repo = git_repo(&[("a.txt", &"x\n".repeat(MAX_MATCHES))]);
        let text = text(repo.path(), &paths(&["a.txt"]), "x", &AtomicBool::new(false));
        assert_eq!((text.matches, text.capped), (MAX_MATCHES, false));
    }

    #[test]
    fn a_match_past_the_cap_in_the_next_file_counts_as_cut() {
        let repo = git_repo(&[("a.txt", &"x\n".repeat(MAX_MATCHES)), ("b.txt", "x\n")]);
        let text = text_on(repo.path(), &paths(&["a.txt", "b.txt"]), "x", &AtomicBool::new(false), 1);
        assert_eq!((text.matches, text.capped), (MAX_MATCHES, true));
    }

    #[test]
    fn threads_never_keep_more_than_the_cap() {
        let names: Vec<String> = (0..32).map(|i| format!("{i:02}.txt")).collect();
        let body = "x\n".repeat(100);
        let files: Vec<(&str, &str)> = names.iter().map(|n| (n.as_str(), body.as_str())).collect();
        let repo = git_repo(&files);
        let text = text_on(repo.path(), &names, "x", &AtomicBool::new(false), MAX_THREADS);
        let kept: usize = text.files.iter().map(|f| f.lines.len()).sum();
        assert_eq!((kept, text.capped), (MAX_MATCHES, true));
    }

    #[test]
    fn a_cancelled_search_finds_nothing_more() {
        let repo = git_repo(&[("a.txt", "needle\n")]);
        let text = text(repo.path(), &paths(&["a.txt"]), "needle", &AtomicBool::new(true));
        assert_eq!(text.matches, 0);
    }

    #[test]
    fn occurrences_follow_smart_case() {
        assert_eq!(occurrences("Run run RUN", "run"), [0..3, 4..7, 8..11]);
        assert_eq!(occurrences("Run run RUN", "Run"), std::iter::once(0..3).collect::<Vec<_>>());
    }
}
