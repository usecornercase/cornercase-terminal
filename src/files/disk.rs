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

fn order(a: &Entry, b: &Entry) -> Ordering {
    b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())).then_with(|| a.name.cmp(&b.name))
}

pub(crate) fn list(root: &Path, folder: &str) -> io::Result<Vec<Entry>> {
    let dir = if folder.is_empty() { root.to_path_buf() } else { root.join(folder) };
    if !dir.is_dir() {
        return Err(io::Error::from(io::ErrorKind::NotFound));
    }
    let walk =
        WalkBuilder::new(&dir).max_depth(Some(1)).hidden(false).filter_entry(|e| e.file_name() != ".git").build();
    let mut entries: Vec<Entry> = walk
        .filter_map(Result::ok)
        .filter(|item| item.depth() == 1)
        .take(MAX_ENTRIES)
        .map(|item| Entry { name: item.file_name().to_string_lossy().into_owned(), dir: item.path().is_dir() })
        .collect();
    entries.sort_by(order);
    Ok(entries)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
    found: bool,
}

const GONE: Stamp = Stamp { modified: None, len: 0, found: false };

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Body {
    Text { lines: Vec<String>, styles: Option<Vec<Segments>> },
    Image(Arc<Picture>),
    Unreadable { format: &'static str, bytes: u64, reason: String },
    Binary,
    TooLarge(u64),
    Missing,
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
}

pub(crate) struct Read {
    pub(crate) content: Content,
    pub(crate) source: Option<String>,
}

pub(crate) fn read(path: &Path, previous: Option<Stamp>) -> Option<Read> {
    let meta = fs::metadata(path).ok();
    let stamp =
        meta.as_ref().map_or(GONE, |meta| Stamp { modified: meta.modified().ok(), len: meta.len(), found: true });
    if previous == Some(stamp) {
        return None;
    }
    let content = |language, body| Content { stamp, language, body };
    let plain = |body| Some(Read { content: content(None, body), source: None });
    let Some(meta) = meta else { return plain(Body::Missing) };
    if !meta.is_file() {
        return plain(Body::Binary);
    }
    let limit = if stamp.len > MAX_BYTES && sniff(path).is_some() { MAX_IMAGE_BYTES } else { MAX_BYTES };
    if stamp.len > limit {
        return plain(Body::TooLarge(stamp.len));
    }
    let Ok(bytes) = head(path, limit + 1) else { return plain(Body::Missing) };
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > limit {
        return plain(Body::TooLarge(size));
    }
    if let Some(format) = decode::sniff(&bytes) {
        return plain(image(format, &bytes));
    }
    if size > MAX_BYTES {
        return plain(Body::TooLarge(size));
    }
    if bytes[..bytes.len().min(BINARY_PROBE)].contains(&0) {
        return plain(Body::Binary);
    }
    let source = String::from_utf8_lossy(&bytes).into_owned();
    let lines = syntax::lines(&source).map(|(_, line)| syntax::expand(line)).collect();
    let first = syntax::lines(&source).next().map_or("", |(_, line)| line);
    let language = syntax::language(path, first);
    Some(Read { content: content(language, Body::Text { lines, styles: None }), source: Some(source) })
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

pub(crate) fn highlighted(read: Read) -> Option<Content> {
    let (Some(language), Some(source)) = (read.content.language, read.source.as_deref()) else { return None };
    let styles = syntax::highlight(source, language)?;
    let Body::Text { lines, .. } = read.content.body else { return None };
    Some(Content { body: Body::Text { lines, styles: Some(styles) }, ..read.content })
}

#[cfg(test)]
mod tests;
