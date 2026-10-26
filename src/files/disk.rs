use std::cmp::Ordering;
use std::fs;
use std::io;
use std::io::Read as _;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{self, AtomicU64};
use std::time::SystemTime;

use ignore::WalkBuilder;

use crate::graphics::Picture;
use crate::graphics::decode;
use crate::highlight::syntax::{self, Segments};
use crate::text::stretch::{Point, Stretch};

pub(crate) const MAX_ENTRIES: usize = 10_000;
pub(crate) const MAX_BYTES: u64 = 8 << 20;
pub(crate) const MAX_IMAGE_BYTES: u64 = 32 << 20;
const BINARY_PROBE: usize = 8000;
const SNIFF_BYTES: u64 = 64;

static PICTURES: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) dir: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    File,
    Folder,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) name: String,
    pub(crate) kind: Kind,
    pub(crate) link: bool,
    pub(crate) bytes: Option<u64>,
    pub(crate) modified: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Folder {
    pub(crate) entries: Vec<Listed>,
    pub(crate) more: bool,
}

fn order(a: &Listed, b: &Listed) -> Ordering {
    let folder = |entry: &Listed| entry.kind == Kind::Folder;
    folder(b)
        .cmp(&folder(a))
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then_with(|| a.name.cmp(&b.name))
}

fn listed(item: &ignore::DirEntry) -> Listed {
    let meta = fs::metadata(item.path()).ok();
    let kind = match &meta {
        Some(meta) if meta.is_dir() => Kind::Folder,
        Some(meta) if meta.is_file() => Kind::File,
        _ => Kind::Other,
    };
    Listed {
        name: item.file_name().to_string_lossy().into_owned(),
        kind,
        link: item.path_is_symlink(),
        bytes: meta.as_ref().filter(|_| kind == Kind::File).map(fs::Metadata::len),
        modified: meta.and_then(|meta| meta.modified().ok()),
    }
}

pub(crate) fn folder(dir: &Path) -> io::Result<Folder> {
    folder_up_to(dir, MAX_ENTRIES)
}

fn folder_up_to(dir: &Path, most: usize) -> io::Result<Folder> {
    if !dir.is_dir() {
        let kind = if dir.exists() { io::ErrorKind::NotADirectory } else { io::ErrorKind::NotFound };
        return Err(io::Error::from(kind));
    }
    let walk = WalkBuilder::new(dir).max_depth(Some(1)).hidden(false).filter_entry(|e| e.file_name() != ".git").build();
    let mut entries: Vec<Listed> =
        walk.filter_map(Result::ok).filter(|item| item.depth() == 1).take(most + 1).map(|item| listed(&item)).collect();
    let more = entries.len() > most;
    entries.truncate(most);
    entries.sort_by(order);
    Ok(Folder { entries, more })
}

pub(crate) fn list(root: &Path, folder_name: &str) -> io::Result<Vec<Entry>> {
    let dir = if folder_name.is_empty() { root.to_path_buf() } else { root.join(folder_name) };
    let listed = folder(&dir).map_err(|_| io::Error::from(io::ErrorKind::NotFound))?;
    Ok(listed.entries.into_iter().map(|e| Entry { dir: e.kind == Kind::Folder, name: e.name }).collect())
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
    found: bool,
}

const GONE: Stamp = Stamp { modified: None, len: 0, found: false };

impl Stamp {
    pub(crate) fn len(self) -> Option<u64> {
        self.found.then_some(self.len)
    }

    pub(crate) fn modified(self) -> Option<SystemTime> {
        self.modified
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Body {
    Text { lines: Vec<String>, styles: Option<Vec<Segments>>, source: Arc<str>, starts: Vec<usize> },
    Image(Arc<Picture>),
    Unreadable { format: &'static str, bytes: u64, reason: String },
    Binary,
    TooLarge(u64),
    Missing,
}

#[cfg(test)]
impl Body {
    pub(crate) fn text(lines: Vec<String>, styles: Option<Vec<Segments>>, source: Arc<str>) -> Self {
        let starts = syntax::lines(&source).map(|(start, _)| start).collect();
        Self::Text { lines, styles, source, starts }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Content {
    pub(crate) stamp: Stamp,
    pub(crate) language: Option<&'static str>,
    pub(crate) body: Body,
}

impl Content {
    pub(crate) fn is_image(&self) -> bool {
        matches!(self.body, Body::Image(_) | Body::Unreadable { .. })
    }

    pub(crate) fn lines(&self) -> &[String] {
        match &self.body {
            Body::Text { lines, .. } => lines,
            _ => &[],
        }
    }

    pub(crate) fn styles(&self) -> Option<&[Segments]> {
        match &self.body {
            Body::Text { styles, .. } => styles.as_deref(),
            _ => None,
        }
    }

    pub(crate) fn text(&self, lines: Option<(u32, u32)>) -> Option<String> {
        let Body::Text { source, .. } = &self.body else { return None };
        let Some((first, last)) = lines else { return Some(source.to_string()) };
        let skip = usize::try_from(first.saturating_sub(1)).unwrap_or(usize::MAX);
        let take = usize::try_from(last.saturating_sub(first)).map_or(usize::MAX, |n| n.saturating_add(1));
        Some(syntax::lines(source).skip(skip).take(take).map(|(_, line)| line).collect::<Vec<_>>().join("\n"))
    }

    pub(crate) fn source_line(&self, n: u32) -> Option<&str> {
        let Body::Text { source, starts, .. } = &self.body else { return None };
        let index = usize::try_from(n).ok()?.checked_sub(1)?;
        let start = *starts.get(index)?;
        let rest = source.get(start..)?;
        let end = rest.find('\n').unwrap_or(rest.len());
        Some(rest[..end].trim_end_matches('\r'))
    }

    pub(crate) fn clamped(&self, stretch: Stretch<u32>) -> Option<Stretch<u32>> {
        let last = u32::try_from(self.lines().len()).ok()?;
        if stretch.from.line == 0 || stretch.from.line > last {
            return None;
        }
        if stretch.to.line <= last {
            return Some(stretch);
        }
        let end = self.source_line(last)?.chars().count();
        Some(Stretch { to: Point { line: last, offset: end }, ..stretch })
    }

    pub(crate) fn cut(&self, stretch: Stretch<u32>) -> Option<String> {
        let lines = (stretch.from.line..=stretch.to.line).map(|n| self.source_line(n));
        Some(stretch.text(lines.collect::<Option<Vec<_>>>()?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Raw {
    Text(String),
    Image { format: &'static str, bytes: Vec<u8> },
    Binary,
    TooLarge(u64),
    Folder,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Loaded {
    pub(crate) stamp: Stamp,
    pub(crate) raw: Raw,
}

pub(crate) fn load(path: &Path, previous: Option<Stamp>) -> Option<Loaded> {
    let meta = fs::metadata(path).ok();
    let stamp =
        meta.as_ref().map_or(GONE, |meta| Stamp { modified: meta.modified().ok(), len: meta.len(), found: true });
    if previous == Some(stamp) {
        return None;
    }
    Some(Loaded { stamp, raw: raw(path, meta.as_ref(), stamp.len) })
}

fn raw(path: &Path, meta: Option<&fs::Metadata>, len: u64) -> Raw {
    let Some(meta) = meta else { return Raw::Missing };
    if !meta.is_file() {
        return if meta.is_dir() { Raw::Folder } else { Raw::Binary };
    }
    let limit = if len > MAX_BYTES && sniff(path).is_some() { MAX_IMAGE_BYTES } else { MAX_BYTES };
    if len > limit {
        return Raw::TooLarge(len);
    }
    let Ok(bytes) = head(path, limit + 1) else { return Raw::Missing };
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > limit {
        return Raw::TooLarge(size);
    }
    if let Some(format) = decode::sniff(&bytes) {
        return Raw::Image { format, bytes };
    }
    if size > MAX_BYTES {
        return Raw::TooLarge(size);
    }
    if bytes[..bytes.len().min(BINARY_PROBE)].contains(&0) {
        return Raw::Binary;
    }
    Raw::Text(String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
}

pub(crate) fn read(path: &Path, previous: Option<Stamp>) -> Option<Content> {
    let Loaded { stamp, raw } = load(path, previous)?;
    let (language, body) = match raw {
        Raw::Text(source) => text_body(path, source),
        Raw::Image { format, bytes } => (None, image(format, &bytes)),
        Raw::Binary | Raw::Folder => (None, Body::Binary),
        Raw::TooLarge(size) => (None, Body::TooLarge(size)),
        Raw::Missing => (None, Body::Missing),
    };
    Some(Content { stamp, language, body })
}

fn text_body(path: &Path, source: String) -> (Option<&'static str>, Body) {
    let (starts, lines): (Vec<usize>, Vec<String>) =
        syntax::lines(&source).map(|(start, line)| (start, syntax::expand(line))).unzip();
    let first = syntax::lines(&source).next().map_or("", |(_, line)| line);
    let language = syntax::language(path, first);
    (language, Body::Text { lines, styles: None, source: Arc::from(source), starts })
}

fn head(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(limit).read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn sniff(path: &Path) -> Option<&'static str> {
    decode::sniff(&head(path, SNIFF_BYTES).ok()?)
}

fn image(format: &'static str, bytes: &[u8]) -> Body {
    match decode::decode(bytes, PICTURES.fetch_add(1, atomic::Ordering::Relaxed)) {
        Ok(picture) => Body::Image(Arc::new(picture)),
        Err(reason) => Body::Unreadable { format, bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX), reason },
    }
}

pub(crate) fn highlighted(content: Content) -> Option<Content> {
    let Body::Text { lines, source, starts, .. } = content.body else { return None };
    let styles = syntax::highlight(&source, content.language?)?;
    Some(Content { body: Body::Text { lines, styles: Some(styles), source, starts }, ..content })
}

#[cfg(test)]
mod tests;
