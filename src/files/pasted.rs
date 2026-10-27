use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use base64::Engine;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};

use super::disk;
use crate::graphics::decode;
use crate::log;
use crate::time::Stamp;

pub(crate) const MAX_BYTES: u64 = disk::MAX_IMAGE_BYTES;
pub(crate) const KEEP_FOR: Duration = Duration::from_hours(7 * 24);
const MAX_NAME: usize = 100;
const MAX_FOLDERS: u32 = 1000;
const FALLBACK: &str = "pasted";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stored {
    pub(crate) dir: PathBuf,
    pub(crate) path: PathBuf,
    pub(crate) bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Problem {
    Unfit(String),
    Disk(String),
}

fn clean(part: &str) -> String {
    let mut out = String::new();
    for c in part.chars() {
        if c.is_ascii_alphanumeric() || "_+.".contains(c) {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out
}

fn extension_of(head: &[u8]) -> Option<&'static str> {
    Some(match decode::sniff(head)? {
        "PNG" => "png",
        "JPEG" => "jpg",
        "GIF" => "gif",
        "WebP" => "webp",
        "BMP" => "bmp",
        _ => "ico",
    })
}

fn cut(text: &str, max: usize) -> &str {
    let mut end = max.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

pub(crate) fn file_name(asked: &str, head: &[u8]) -> String {
    let last = asked.rsplit('/').next().unwrap_or_default();
    let (stem, extension) = match last.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem, Some(extension)),
        _ => (last, None),
    };
    let stem = clean(stem);
    let stem = stem.trim_matches(['-', '.']);
    let extension = extension.map(clean).map(|e| e.trim_matches('-').to_string()).filter(|e| !e.is_empty());
    let extension = extension.or_else(|| extension_of(head).map(str::to_string));
    let stem = if stem.is_empty() { FALLBACK } else { stem };
    let room = MAX_NAME.saturating_sub(extension.as_ref().map_or(0, |e| e.len() + 1));
    let stem = cut(stem, room).trim_end_matches('-');
    match extension {
        Some(extension) => format!("{stem}.{extension}"),
        None => stem.to_string(),
    }
}

fn decode_data(data: &str) -> Result<Vec<u8>, String> {
    let engine = GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
    );
    if data.bytes().any(|b| b.is_ascii_whitespace()) {
        let squeezed: String = data.chars().filter(|c| !c.is_ascii_whitespace()).collect();
        return engine.decode(squeezed).map_err(|e| e.to_string());
    }
    engine.decode(data).map_err(|e| e.to_string())
}

fn private_dir(path: &Path) -> std::io::Result<()> {
    DirBuilder::new().mode(0o700).create(path)
}

fn fresh_folder(dir: &Path, now: SystemTime) -> std::io::Result<PathBuf> {
    let stamp = Stamp(now).to_string();
    let base = stamp.split('.').next().unwrap_or_default().replace(':', "-");
    for attempt in 1..=MAX_FOLDERS {
        let name = if attempt == 1 { format!("{base}Z") } else { format!("{base}Z-{attempt}") };
        let folder = dir.join(name);
        match private_dir(&folder) {
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            other => return other.map(|()| folder),
        }
    }
    Err(ErrorKind::AlreadyExists.into())
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
    file.write_all(bytes)
}

struct Place<'a> {
    dir: &'a Path,
    keep: &'a [PathBuf],
    cap: u64,
}

pub(crate) fn store(dir: &Path, asked: &str, data: &str, keep: &[PathBuf], now: SystemTime) -> Result<Stored, Problem> {
    store_within(&Place { dir, keep, cap: MAX_BYTES }, asked, data, now)
}

fn store_within(place: &Place, asked: &str, data: &str, now: SystemTime) -> Result<Stored, Problem> {
    let Place { dir, keep, cap } = *place;
    if data.len() as u64 > cap / 3 * 4 + 4 + data.len() as u64 / 76 + 4 {
        let limit = cap >> 20;
        return Err(Problem::Unfit(format!("the file is over the {limit} MiB `paste-file` takes")));
    }
    let bytes = decode_data(data).map_err(|e| Problem::Unfit(format!("the file's `data` is not base64: {e}")))?;
    let len = bytes.len() as u64;
    if len > cap {
        let (mib, limit) = (len.div_ceil(1 << 20), cap >> 20);
        return Err(Problem::Unfit(format!("the file is {mib} MiB, over the {limit} MiB `paste-file` takes")));
    }
    sweep(dir, keep, now.checked_sub(KEEP_FOR).unwrap_or(SystemTime::UNIX_EPOCH));
    let disk = |e: std::io::Error| Problem::Disk(format!("cannot keep the file in {}: {e}", dir.display()));
    if let Err(e) = private_dir(dir).or_else(|e| if e.kind() == ErrorKind::AlreadyExists { Ok(()) } else { Err(e) }) {
        return Err(disk(e));
    }
    let folder = fresh_folder(dir, now).map_err(disk)?;
    let path = folder.join(file_name(asked, &bytes));
    if let Err(e) = write_private(&path, &bytes) {
        remove(std::slice::from_ref(&folder));
        return Err(disk(e));
    }
    Ok(Stored { dir: folder, path, bytes: len })
}

pub(crate) fn sweep(dir: &Path, keep: &[PathBuf], before: SystemTime) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let old = fs::symlink_metadata(&path).and_then(|m| m.modified()).is_ok_and(|at| at < before);
        if old && !keep.contains(&path) {
            remove(&[path]);
        }
    }
}

pub(crate) fn remove(dirs: &[PathBuf]) {
    for dir in dirs {
        let gone = match fs::symlink_metadata(dir) {
            Ok(meta) if meta.is_dir() => fs::remove_dir_all(dir),
            Ok(_) => fs::remove_file(dir),
            Err(e) => Err(e),
        };
        if let Err(e) = gone {
            warn_unless_missing(&e);
        }
    }
}

fn warn_unless_missing(e: &std::io::Error) {
    if e.kind() != ErrorKind::NotFound {
        log::warning!("files", "could not remove a pasted file", error = e);
    }
}

#[cfg(test)]
mod tests;
