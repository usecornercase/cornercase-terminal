use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use ratatui::style::Style;
use similar::{Algorithm, DiffTag};

use crate::highlight;
use crate::text::TAB;

pub(crate) const MAX_LINES: usize = 3000;
pub(crate) const MAX_UNTRACKED_BYTES: u64 = 1 << 20;
const BINARY_PROBE: usize = 8000;
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
pub(crate) enum Status {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

impl Status {
    pub(crate) fn letter(self) -> &'static str {
        match self {
            Self::Modified => "M",
            Self::Added | Self::Untracked => "A",
            Self::Deleted => "D",
            Self::Renamed => "R",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fold {
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

    pub(crate) fn tag(self) -> Option<&'static str> {
        match self {
            Self::Open => None,
            Self::Lockfile => Some("lockfile"),
            Self::Binary => Some("binary"),
            Self::Large => Some("too large"),
        }
    }

    pub(crate) fn shows_lines(self) -> bool {
        matches!(self, Self::Open | Self::Lockfile)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Context,
    Removed,
    Added,
}

pub(crate) type Segments = Vec<(String, Style)>;
type Emphasis = (Vec<Range<usize>>, Vec<Range<usize>>);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Line {
    pub(crate) kind: Kind,
    pub(crate) old: Option<u32>,
    pub(crate) new: Option<u32>,
    pub(crate) text: String,
    pub(crate) raw: Option<Box<str>>,
    pub(crate) syntax: Option<Segments>,
    pub(crate) emphasis: Vec<Range<usize>>,
}

impl Line {
    fn new(kind: Kind, old: Option<u32>, new: Option<u32>, text: &str) -> Self {
        Self { kind, old, new, text: expand(text), raw: own(text), syntax: None, emphasis: Vec::new() }
    }

    pub(crate) fn source(&self) -> &str {
        self.raw.as_deref().unwrap_or(&self.text)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Hunk {
    pub(crate) old_start: u32,
    pub(crate) new_start: u32,
    pub(crate) context: String,
    pub(crate) lines: Vec<Line>,
}

impl Hunk {
    fn count(&self, skip: Kind) -> u32 {
        u32::try_from(self.lines.iter().filter(|l| l.kind != skip).count()).unwrap_or(u32::MAX)
    }

    pub(crate) fn old_end(&self) -> u32 {
        self.old_start.saturating_add(self.count(Kind::Added))
    }

    pub(crate) fn new_end(&self) -> u32 {
        self.new_start.saturating_add(self.count(Kind::Removed))
    }

    pub(crate) fn changed(&self) -> (u32, u32) {
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

    pub(crate) fn patch(&self) -> String {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Staged {
    No,
    Partly,
    Yes,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct File {
    pub(crate) path: String,
    pub(crate) old_path: Option<String>,
    pub(crate) status: Status,
    pub(crate) added: usize,
    pub(crate) removed: usize,
    pub(crate) fold: Fold,
    pub(crate) hunks: Vec<Hunk>,
    pub(crate) digest: u64,
    pub(crate) viewed_digest: u64,
    pub(crate) staged: Option<Staged>,
}

impl File {
    pub(crate) fn lines(&self) -> usize {
        self.hunks.iter().map(|h| h.lines.len()).sum()
    }

    pub(crate) fn label(&self) -> String {
        match &self.old_path {
            Some(old) => format!("{old} → {}", self.path),
            None => self.path.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Diff {
    pub(crate) files: Vec<Arc<File>>,
}

impl Diff {
    pub(crate) fn added(&self) -> usize {
        self.files.iter().map(|f| f.added).sum()
    }

    pub(crate) fn removed(&self) -> usize {
        self.files.iter().map(|f| f.removed).sum()
    }
}

pub(crate) fn expand(text: &str) -> String {
    text.trim_end_matches('\r').replace('\t', TAB)
}

pub(crate) fn own(text: &str) -> Option<Box<str>> {
    text.contains('\t').then(|| text.into())
}

fn digest(value: impl Hash) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn parse(patch: &str) -> Vec<File> {
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

struct Header {
    status: Status,
    old_path: Option<String>,
    new_path: Option<String>,
    renamed_from: Option<String>,
    binary: bool,
}

impl Header {
    fn read(lines: &[&str]) -> (Self, usize) {
        let mut head =
            Self { status: Status::Modified, old_path: None, new_path: None, renamed_from: None, binary: false };
        let mut i = 1;
        while i < lines.len() && !lines[i].starts_with("@@") {
            head.take(lines[i]);
            i += 1;
        }
        (head, i)
    }

    fn take(&mut self, line: &str) {
        if line.starts_with("new file mode") {
            self.status = Status::Added;
        } else if line.starts_with("deleted file mode") {
            self.status = Status::Deleted;
        } else if let Some(path) = line.strip_prefix("rename from ") {
            self.renamed_from = Some(unquote(path));
            self.status = Status::Renamed;
        } else if let Some(path) = line.strip_prefix("rename to ") {
            self.new_path = Some(unquote(path));
        } else if let Some(path) = line.strip_prefix("--- ") {
            self.old_path = side_path(path, "a/");
        } else if let Some(path) = line.strip_prefix("+++ ") {
            self.new_path = side_path(path, "b/").or(self.new_path.take());
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            self.binary = true;
        }
    }
}

fn parse_file(lines: &[&str]) -> Option<File> {
    let header = lines.first()?.strip_prefix("diff --git ")?;
    let (head, i) = Header::read(lines);
    let Header { status, old_path, new_path, renamed_from, binary } = head;
    let path = new_path.or_else(|| old_path.clone()).or_else(|| header_path(header))?;
    let mut hunks = parse_hunks(&lines[i..]);
    let count = |kind| hunks.iter().flat_map(|h| &h.lines).filter(|l| l.kind == kind).count();
    let (added, removed) = (count(Kind::Added), count(Kind::Removed));
    let fold = Fold::of(&path, binary, added, removed);
    if fold == Fold::Large {
        hunks.clear();
    }
    let old_path = renamed_from.filter(|from| *from != path);
    let digest = digest((&path, lines));
    Some(File { digest, viewed_digest: digest, staged: None, path, old_path, status, added, removed, fold, hunks })
}

fn parse_hunks(lines: &[&str]) -> Vec<Hunk> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let (mut old, mut new) = (0, 0);
    for &line in lines {
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
    hunks
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
            push_char(&mut bytes, c);
            continue;
        }
        match chars.next() {
            Some('n') => bytes.push(b'\n'),
            Some('t') => bytes.push(b'\t'),
            Some(d @ '0'..='7') => bytes.push(octal(d, &mut chars)),
            Some(other) => push_char(&mut bytes, other),
            None => {}
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn push_char(bytes: &mut Vec<u8>, c: char) {
    let mut buf = [0; 4];
    bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
}

fn octal(first: char, chars: &mut std::iter::Peekable<std::str::Chars>) -> u8 {
    let mut value = first.to_digit(8).unwrap_or(0);
    for _ in 0..2 {
        if let Some(next) = chars.peek().and_then(|c| c.to_digit(8)) {
            value = value * 8 + next;
            chars.next();
        }
    }
    u8::try_from(value).unwrap_or(b'?')
}

fn is_lockfile(path: &str) -> bool {
    Path::new(path).file_name().and_then(|n| n.to_str()).is_some_and(|name| LOCKFILES.contains(&name))
}

pub(crate) fn untracked(path: &str, size: u64, bytes: Option<&[u8]>) -> File {
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
        staged: None,
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

pub(crate) fn finish(file: &mut File) {
    if !file.fold.shows_lines() {
        return;
    }
    let language = language(&file.path);
    for hunk in &mut file.hunks {
        emphasize(&mut hunk.lines);
        highlight_hunk(hunk, &language);
    }
}

pub(crate) fn language(path: &str) -> String {
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
mod tests;
