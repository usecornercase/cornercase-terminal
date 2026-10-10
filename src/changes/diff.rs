use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use ratatui::style::Style;
use similar::{Algorithm, DiffTag};

use crate::highlight;

pub const MAX_LINES: usize = 3000;
pub const MAX_UNTRACKED_BYTES: u64 = 1 << 20;
const BINARY_PROBE: usize = 8000;
const TAB: &str = "    ";
const LOCKFILES: [&str; 16] = [
    "Cargo.lock",
    "package-lock.json",
    "npm-shrinkwrap.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
    "Gemfile.lock",
    "poetry.lock",
    "uv.lock",
    "Pipfile.lock",
    "composer.lock",
    "go.sum",
    "flake.lock",
    "pubspec.lock",
    "Podfile.lock",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

impl Status {
    pub fn letter(self) -> &'static str {
        match self {
            Self::Modified => "M",
            Self::Added | Self::Untracked => "A",
            Self::Deleted => "D",
            Self::Renamed => "R",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fold {
    Open,
    Lockfile,
    Binary,
    Large,
}

impl Fold {
    pub(super) fn of(path: &str, binary: bool, added: usize, removed: usize) -> Self {
        if binary {
            Self::Binary
        } else if added.saturating_add(removed) > MAX_LINES {
            Self::Large
        } else if is_lockfile(path) {
            Self::Lockfile
        } else {
            Self::Open
        }
    }

    pub fn tag(self) -> Option<&'static str> {
        match self {
            Self::Open => None,
            Self::Lockfile => Some("lockfile"),
            Self::Binary => Some("binary"),
            Self::Large => Some("too large"),
        }
    }

    pub fn shows_lines(self) -> bool {
        matches!(self, Self::Open | Self::Lockfile)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Context,
    Removed,
    Added,
}

pub type Segments = Vec<(String, Style)>;
type Emphasis = (Vec<Range<usize>>, Vec<Range<usize>>);

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub kind: Kind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
    pub syntax: Option<Segments>,
    pub emphasis: Vec<Range<usize>>,
}

impl Line {
    fn new(kind: Kind, old: Option<u32>, new: Option<u32>, text: &str) -> Self {
        Self { kind, old, new, text: expand(text), syntax: None, emphasis: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hunk {
    pub old_start: u32,
    pub new_start: u32,
    pub context: String,
    pub lines: Vec<Line>,
}

impl Hunk {
    fn count(&self, skip: Kind) -> u32 {
        u32::try_from(self.lines.iter().filter(|l| l.kind != skip).count()).unwrap_or(u32::MAX)
    }

    pub fn old_end(&self) -> u32 {
        self.old_start.saturating_add(self.count(Kind::Added))
    }

    pub fn new_end(&self) -> u32 {
        self.new_start.saturating_add(self.count(Kind::Removed))
    }

    pub fn changed(&self) -> (u32, u32) {
        let added: Vec<u32> = self.lines.iter().filter(|l| l.kind == Kind::Added).filter_map(|l| l.new).collect();
        if let (Some(first), Some(last)) = (added.first(), added.last()) {
            return (*first, *last);
        }
        let mut new = self.new_start.max(1);
        for line in &self.lines {
            match line.kind {
                Kind::Removed => break,
                Kind::Context | Kind::Added => new = line.new.map_or(new, |n| n + 1),
            }
        }
        (new, new)
    }

    pub fn patch(&self) -> String {
        let old = self.count(Kind::Added);
        let new = self.count(Kind::Removed);
        let mut text = format!("@@ -{},{old} +{},{new} @@", self.old_start, self.new_start);
        if !self.context.is_empty() {
            text.push(' ');
            text.push_str(&self.context);
        }
        for line in &self.lines {
            let sign = match line.kind {
                Kind::Context => ' ',
                Kind::Removed => '-',
                Kind::Added => '+',
            };
            text.push('\n');
            text.push(sign);
            text.push_str(&line.text);
        }
        text.push('\n');
        text
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub path: String,
    pub old_path: Option<String>,
    pub status: Status,
    pub added: usize,
    pub removed: usize,
    pub fold: Fold,
    pub hunks: Vec<Hunk>,
    pub digest: u64,
    pub viewed_digest: u64,
}

impl File {
    pub fn lines(&self) -> usize {
        self.hunks.iter().map(|h| h.lines.len()).sum()
    }

    pub fn label(&self) -> String {
        match &self.old_path {
            Some(old) => format!("{old} → {}", self.path),
            None => self.path.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Diff {
    pub files: Vec<Arc<File>>,
}

impl Diff {
    pub fn added(&self) -> usize {
        self.files.iter().map(|f| f.added).sum()
    }

    pub fn removed(&self) -> usize {
        self.files.iter().map(|f| f.removed).sum()
    }
}

pub fn expand(text: &str) -> String {
    text.trim_end_matches('\r').replace('\t', TAB)
}

fn digest(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

pub fn parse(patch: &str) -> Vec<File> {
    let mut files = Vec::new();
    let mut section: Vec<&str> = Vec::new();
    for line in patch.lines() {
        if line.starts_with("diff --git ") && !section.is_empty() {
            files.extend(parse_file(&section));
            section.clear();
        }
        section.push(line);
    }
    files.extend(parse_file(&section));
    files
}

fn parse_file(lines: &[&str]) -> Option<File> {
    let header = lines.first()?.strip_prefix("diff --git ")?;
    let mut status = Status::Modified;
    let (mut old_path, mut new_path, mut renamed_from) = (None, None, None);
    let mut binary = false;
    let mut i = 1;
    while i < lines.len() && !lines[i].starts_with("@@") {
        let line = lines[i];
        if line.starts_with("new file mode") {
            status = Status::Added;
        } else if line.starts_with("deleted file mode") {
            status = Status::Deleted;
        } else if let Some(path) = line.strip_prefix("rename from ") {
            renamed_from = Some(unquote(path));
            status = Status::Renamed;
        } else if let Some(path) = line.strip_prefix("rename to ") {
            new_path = Some(unquote(path));
        } else if let Some(path) = line.strip_prefix("--- ") {
            old_path = side_path(path, "a/");
        } else if let Some(path) = line.strip_prefix("+++ ") {
            new_path = side_path(path, "b/").or(new_path);
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            binary = true;
        }
        i += 1;
    }
    let path = new_path.or_else(|| old_path.clone()).or_else(|| header_path(header))?;
    let mut hunks: Vec<Hunk> = Vec::new();
    let (mut old, mut new) = (0, 0);
    for &line in &lines[i..] {
        if let Some((old_start, new_start, context)) = hunk_header(line) {
            (old, new) = (old_start, new_start);
            hunks.push(Hunk { old_start, new_start, context, lines: Vec::new() });
            continue;
        }
        let Some(hunk) = hunks.last_mut() else { continue };
        let (kind, text) = match line.chars().next() {
            Some('+') => (Kind::Added, &line[1..]),
            Some('-') => (Kind::Removed, &line[1..]),
            Some('\\') => continue,
            Some(' ') => (Kind::Context, &line[1..]),
            _ => (Kind::Context, line),
        };
        let numbers = match kind {
            Kind::Context => (Some(old), Some(new)),
            Kind::Removed => (Some(old), None),
            Kind::Added => (None, Some(new)),
        };
        old += u32::from(kind != Kind::Added);
        new += u32::from(kind != Kind::Removed);
        hunk.lines.push(Line::new(kind, numbers.0, numbers.1, text));
    }
    let count = |kind| hunks.iter().flat_map(|h| &h.lines).filter(|l| l.kind == kind).count();
    let (added, removed) = (count(Kind::Added), count(Kind::Removed));
    let fold = Fold::of(&path, binary, added, removed);
    if fold == Fold::Large {
        hunks.clear();
    }
    let old_path = renamed_from.filter(|from| *from != path);
    let digest = digest((&path, lines));
    Some(File { digest, viewed_digest: digest, path, old_path, status, added, removed, fold, hunks })
}

fn hunk_header(line: &str) -> Option<(u32, u32, String)> {
    let rest = line.strip_prefix("@@ -")?;
    let (ranges, context) = rest.split_once(" @@")?;
    let (old, new) = ranges.split_once(" +")?;
    let start = |range: &str| range.split(',').next()?.parse::<u32>().ok();
    Some((start(old)?, start(new)?, context.trim().to_string()))
}

fn side_path(raw: &str, prefix: &str) -> Option<String> {
    let path = unquote(raw.trim_end_matches('\t'));
    if path == "/dev/null" {
        return None;
    }
    Some(path.strip_prefix(prefix).map_or_else(|| path.clone(), str::to_string))
}

fn header_path(header: &str) -> Option<String> {
    let len = header.chars().count().checked_sub(5)? / 2;
    let path: String = header.chars().skip(2).take(len).collect();
    header.starts_with("a/").then_some(path)
}

fn unquote(raw: &str) -> String {
    let Some(inner) = raw.strip_prefix('"').and_then(|r| r.strip_suffix('"')) else { return raw.to_string() };
    let mut bytes = Vec::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buf = [0; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            continue;
        }
        match chars.next() {
            Some('n') => bytes.push(b'\n'),
            Some('t') => bytes.push(b'\t'),
            Some(d @ '0'..='7') => {
                let mut value = d.to_digit(8).unwrap_or(0);
                for _ in 0..2 {
                    if let Some(next) = chars.peek().and_then(|c| c.to_digit(8)) {
                        value = value * 8 + next;
                        chars.next();
                    }
                }
                bytes.push(u8::try_from(value).unwrap_or(b'?'));
            }
            Some(other) => {
                let mut buf = [0; 4];
                bytes.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
            }
            None => {}
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn is_lockfile(path: &str) -> bool {
    Path::new(path).file_name().and_then(|n| n.to_str()).is_some_and(|name| LOCKFILES.contains(&name))
}

pub fn untracked(path: &str, size: u64, bytes: Option<&[u8]>) -> File {
    untracked_file(path, size, bytes, true)
}

pub(super) fn untracked_file(path: &str, size: u64, bytes: Option<&[u8]>, materialize: bool) -> File {
    let mut file = File {
        path: path.to_string(),
        old_path: None,
        status: Status::Untracked,
        added: 0,
        removed: 0,
        fold: Fold::Large,
        hunks: Vec::new(),
        digest: digest((path, size, bytes)),
        viewed_digest: digest((path, size, bytes)),
    };
    let Some(bytes) = bytes.filter(|_| size <= MAX_UNTRACKED_BYTES) else { return file };
    if bytes[..bytes.len().min(BINARY_PROBE)].contains(&0) {
        file.fold = Fold::Binary;
        return file;
    }
    let text = String::from_utf8_lossy(bytes);
    file.added = text.lines().count();
    file.fold = Fold::of(path, false, file.added, 0);
    if !materialize || !file.fold.shows_lines() {
        return file;
    }
    let lines: Vec<Line> = text.lines().zip(1..).map(|(line, n)| Line::new(Kind::Added, None, Some(n), line)).collect();
    if !lines.is_empty() {
        file.hunks.push(Hunk { old_start: 0, new_start: 1, context: String::new(), lines });
    }
    file
}

pub fn finish(file: &mut File) {
    if !file.fold.shows_lines() {
        return;
    }
    let language = language(&file.path);
    for hunk in &mut file.hunks {
        emphasize(&mut hunk.lines);
        highlight_hunk(hunk, &language);
    }
}

pub fn language(path: &str) -> String {
    let path = Path::new(path);
    path.extension().or_else(|| path.file_name()).map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn emphasize(lines: &mut [Line]) {
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind != Kind::Removed {
            i += 1;
            continue;
        }
        let removed_start = i;
        while i < lines.len() && lines[i].kind == Kind::Removed {
            i += 1;
        }
        let added_start = i;
        while i < lines.len() && lines[i].kind == Kind::Added {
            i += 1;
        }
        pair(lines, removed_start..added_start, added_start..i);
    }
}

fn pair(lines: &mut [Line], removed: Range<usize>, added: Range<usize>) {
    let mut next = added.start;
    for r in removed {
        for a in next..added.end {
            if let Some((old, new)) = words(&lines[r].text, &lines[a].text) {
                lines[r].emphasis = old;
                lines[a].emphasis = new;
                next = a + 1;
                break;
            }
        }
    }
}

fn tokens(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut previous: Option<u8> = None;
    for (i, c) in text.char_indices() {
        let class = if c.is_alphanumeric() || c == '_' {
            0
        } else if c.is_whitespace() {
            1
        } else {
            2
        };
        if i > start && (previous != Some(class) || class == 2) {
            out.push(&text[start..i]);
            start = i;
        }
        previous = Some(class);
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

fn words(old: &str, new: &str) -> Option<Emphasis> {
    let (a, b) = (tokens(old), tokens(new));
    let offsets = |tokens: &[&str]| {
        let mut at = vec![0];
        for token in tokens {
            at.push(at.last().copied().unwrap_or(0) + token.chars().count());
        }
        at
    };
    let (oa, ob) = (offsets(&a), offsets(&b));
    let (mut old_ranges, mut new_ranges) = (Vec::new(), Vec::new());
    for op in similar::capture_diff_slices(Algorithm::Myers, &a, &b) {
        let (tag, ra, rb) = op.as_tag_tuple();
        if matches!(tag, DiffTag::Delete | DiffTag::Replace) {
            push_range(&mut old_ranges, oa[ra.start]..oa[ra.end]);
        }
        if matches!(tag, DiffTag::Insert | DiffTag::Replace) {
            push_range(&mut new_ranges, ob[rb.start]..ob[rb.end]);
        }
    }
    let changed: usize = old_ranges.iter().chain(&new_ranges).map(ExactSizeIterator::len).sum();
    let total = oa.last().copied().unwrap_or(0) + ob.last().copied().unwrap_or(0);
    (total > 0 && changed * 10 <= total * 6).then_some((old_ranges, new_ranges))
}

fn push_range(ranges: &mut Vec<Range<usize>>, range: Range<usize>) {
    if range.is_empty() {
        return;
    }
    match ranges.last_mut() {
        Some(last) if last.end == range.start => last.end = range.end,
        _ => ranges.push(range),
    }
}

fn highlight_hunk(hunk: &mut Hunk, language: &str) {
    for skip in [Kind::Added, Kind::Removed] {
        let side: Vec<usize> = (0..hunk.lines.len()).filter(|&i| hunk.lines[i].kind != skip).collect();
        if side.is_empty() {
            continue;
        }
        let code = side.iter().map(|&i| hunk.lines[i].text.as_str()).collect::<Vec<_>>().join("\n");
        let Some(highlighted) = highlight::highlight(&code, language) else { return };
        for (k, &i) in side.iter().enumerate() {
            if skip == Kind::Added && hunk.lines[i].kind == Kind::Context {
                continue;
            }
            hunk.lines[i].syntax = Some(highlighted.get(k).cloned().unwrap_or_default());
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;
    use rstest::rstest;

    use super::*;

    const PATCH: &str = "diff --git a/src/address.rs b/src/address.rs
index 1111111..2222222 100644
--- a/src/address.rs
+++ b/src/address.rs
@@ -9,4 +9,4 @@ fn first_line
 /// The first line of the address.
-pub fn first_line(lines: &[String]) -> &str {
-    lines.first().unwrap()
+pub fn first_line(lines: &[String]) -> Option<&str> {
+    lines.first().map(String::as_str)
 }
diff --git a/old name.txt b/new name.txt
similarity index 90%
rename from old name.txt
rename to new name.txt
index 3333333..4444444 100644
--- a/old name.txt\t
+++ b/new name.txt\t
@@ -1 +1,2 @@
 kept
+added
diff --git a/logo.png b/logo.png
index 5555555..6666666 100644
Binary files a/logo.png and b/logo.png differ
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
index 7777777..0000000
--- a/gone.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-one
-two
";

    fn parsed() -> Vec<File> {
        parse(PATCH)
    }

    mod parse {
        use std::fmt::Write as _;

        use super::*;

        #[test]
        fn reads_every_file_of_the_patch() {
            let paths: Vec<(String, Status)> = parsed().into_iter().map(|f| (f.path, f.status)).collect();
            assert_eq!(
                paths,
                [
                    ("src/address.rs".to_string(), Status::Modified),
                    ("new name.txt".to_string(), Status::Renamed),
                    ("logo.png".to_string(), Status::Modified),
                    ("gone.txt".to_string(), Status::Deleted),
                ]
            );
        }

        #[test]
        fn counts_added_and_removed_lines() {
            let counts: Vec<(usize, usize)> = parsed().iter().map(|f| (f.added, f.removed)).collect();
            assert_eq!(counts, [(2, 2), (1, 0), (0, 0), (0, 2)]);
        }

        #[test]
        fn numbers_lines_on_both_sides() {
            let numbers: Vec<(Option<u32>, Option<u32>)> =
                parsed()[0].hunks[0].lines.iter().map(|l| (l.old, l.new)).collect();
            assert_eq!(
                numbers,
                [
                    (Some(9), Some(9)),
                    (Some(10), None),
                    (Some(11), None),
                    (None, Some(10)),
                    (None, Some(11)),
                    (Some(12), Some(12))
                ]
            );
        }

        #[test]
        fn keeps_the_function_of_each_hunk() {
            assert_eq!(parsed()[0].hunks[0].context, "fn first_line");
        }

        #[test]
        fn a_renamed_file_keeps_its_old_name() {
            assert_eq!(parsed()[1].old_path.as_deref(), Some("old name.txt"));
        }

        #[test]
        fn binary_files_are_folded() {
            assert_eq!(parsed()[2].fold, Fold::Binary);
        }

        #[test]
        fn lockfiles_are_folded_but_keep_their_lines() {
            let patch =
                "diff --git a/Cargo.lock b/Cargo.lock\n--- a/Cargo.lock\n+++ b/Cargo.lock\n@@ -1 +1 @@\n-a\n+b\n";
            let file = &parse(patch)[0];
            assert_eq!((file.fold, file.hunks.len()), (Fold::Lockfile, 1));
        }

        #[test]
        fn huge_files_drop_their_lines() {
            let lines = (0..=MAX_LINES).fold(String::new(), |mut lines, i| {
                let _ = writeln!(lines, "+{i}");
                lines
            });
            let patch =
                format!("diff --git a/big b/big\n--- /dev/null\n+++ b/big\n@@ -0,0 +1,{} @@\n{lines}", MAX_LINES + 1);
            let file = &parse(&patch)[0];
            assert_eq!((file.fold, file.added, file.hunks.len()), (Fold::Large, MAX_LINES + 1, 0));
        }

        #[test]
        fn expands_tabs_and_drops_carriage_returns() {
            let patch = "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-\tx\r\n+\ty\r\n";
            assert_eq!(parse(patch)[0].hunks[0].lines[1].text, "    y");
        }

        #[test]
        fn reads_quoted_paths() {
            let patch = "diff --git \"a/caf\\303\\251.txt\" \"b/caf\\303\\251.txt\"\n--- \"a/caf\\303\\251.txt\"\n+++ \"b/caf\\303\\251.txt\"\n@@ -1 +1 @@\n-a\n+b\n";
            assert_eq!(parse(patch)[0].path, "café.txt");
        }

        #[test]
        fn takes_the_path_from_the_header_when_there_are_no_lines() {
            let patch = "diff --git a/run.sh b/run.sh\nold mode 100644\nnew mode 100755\n";
            assert_eq!(parse(patch)[0].path, "run.sh");
        }

        #[test]
        fn the_same_section_has_the_same_digest() {
            assert_eq!(parse(PATCH)[0].digest, parsed()[0].digest);
            assert_ne!(parsed()[0].digest, parsed()[1].digest);
        }
    }

    mod untracked {
        use super::*;

        #[test]
        fn every_line_is_added() {
            let file = untracked("notes.md", 4, Some(b"a\nb\n"));
            let numbers: Vec<Option<u32>> = file.hunks[0].lines.iter().map(|l| l.new).collect();
            assert_eq!((file.status, file.added, numbers), (Status::Untracked, 2, vec![Some(1), Some(2)]));
        }

        #[test]
        fn a_file_with_nul_bytes_is_binary() {
            assert_eq!(untracked("blob", 3, Some(b"a\0b")).fold, Fold::Binary);
        }

        #[test]
        fn a_big_file_is_not_read() {
            assert_eq!(untracked("dump.sql", MAX_UNTRACKED_BYTES + 1, None).fold, Fold::Large);
        }
    }

    mod words {
        use super::*;

        #[rstest]
        #[case::return_type("fn f() -> &str {", "fn f() -> Option<&str> {", vec![], vec![10..17, 21..22])]
        #[case::method("lines.first().unwrap()", "lines.first().map(as_str)", vec![14..20], vec![14..17, 18..24])]
        fn marks_the_words_that_changed(
            #[case] old: &str,
            #[case] new: &str,
            #[case] old_ranges: Vec<Range<usize>>,
            #[case] new_ranges: Vec<Range<usize>>,
        ) {
            let (o, n) = words(old, new).expect("similar lines");
            assert_eq!((o, n), (old_ranges, new_ranges));
        }

        #[test]
        fn different_lines_are_not_paired() {
            assert_eq!(words("let total = 1;", "fn render(frame: &mut Frame) {}"), None);
        }

        #[test]
        fn pairs_each_removed_line_with_a_similar_added_line() {
            let mut file = parsed().remove(0);
            finish(&mut file);
            let emphasis: Vec<bool> = file.hunks[0].lines.iter().map(|l| !l.emphasis.is_empty()).collect();
            assert_eq!(emphasis, [false, false, true, true, true, false]);
        }
    }

    #[test]
    fn highlights_both_sides_of_a_hunk() {
        let mut file = parsed().remove(0);
        finish(&mut file);
        let keyword = |line: &Line| {
            line.syntax.as_ref().and_then(|s| s.iter().find(|(t, _)| t == "fn")).and_then(|(_, style)| style.fg)
        };
        let lines = &file.hunks[0].lines;
        assert_eq!((keyword(&lines[1]), keyword(&lines[3])), (Some(Color::Magenta), Some(Color::Magenta)));
    }

    mod hunk {
        use super::*;

        #[test]
        fn knows_where_it_ends_on_each_side() {
            let hunk = &parsed()[0].hunks[0];
            assert_eq!((hunk.old_end(), hunk.new_end()), (13, 13));
        }

        #[test]
        fn changed_lines_are_the_added_ones() {
            assert_eq!(parsed()[0].hunks[0].changed(), (10, 11));
        }

        #[test]
        fn a_pure_removal_points_at_where_the_lines_were() {
            assert_eq!(parsed()[3].hunks[0].changed(), (1, 1));
        }

        #[test]
        fn copies_as_a_patch() {
            let hunk = &parse(PATCH)[1].hunks[0];
            assert_eq!(hunk.patch(), "@@ -1,1 +1,2 @@\n kept\n+added\n");
        }
    }
}
