use std::fs::{self, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::EntryType;

use crate::text::hex;

const MAX_ENTRIES: usize = 20_000;
const MAX_BYTES: u64 = 1024 * 1024 * 1024;

pub(crate) fn verify(bytes: &[u8], sha256: &str) -> Result<(), String> {
    if hex(&Sha256::digest(bytes)).eq_ignore_ascii_case(sha256) {
        Ok(())
    } else {
        Err("the download does not match its sha256; nothing was unpacked".into())
    }
}

pub(crate) fn unpack(bytes: &[u8], into: &Path) -> Result<usize, String> {
    let failed = |e: &dyn std::fmt::Display| format!("cannot unpack the download: {e}");
    let mut archive = tar::Archive::new(GzDecoder::new(bytes));
    let (mut count, mut total) = (0, 0u64);
    for entry in archive.entries().map_err(|e| failed(&e))? {
        let mut entry = entry.map_err(|e| failed(&e))?;
        count += 1;
        if count > MAX_ENTRIES {
            return Err(failed(&format!("it holds more than {MAX_ENTRIES} files")));
        }
        let name = entry.path().map_err(|e| failed(&e))?.into_owned();
        let relative = inside(&name).ok_or_else(|| failed(&format!("`{}` points outside", name.display())))?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        no_links_above(into, &relative).map_err(|e| failed(&e))?;
        let path = into.join(&relative);
        match entry.header().entry_type() {
            EntryType::Directory => fs::create_dir_all(&path).map_err(|e| failed(&e))?,
            EntryType::Regular | EntryType::Continuous => {
                total = total.saturating_add(entry.size());
                if total > MAX_BYTES {
                    return Err(failed(&format!("it unpacks to more than {} MiB", MAX_BYTES / 1024 / 1024)));
                }
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|e| failed(&e))?;
                }
                if fs::symlink_metadata(&path).is_ok() {
                    remove(&path).map_err(|e| failed(&e))?;
                }
                let executable = entry.header().mode().is_ok_and(|mode| mode & 0o100 != 0);
                let mode = if executable { 0o755 } else { 0o644 };
                let mut file =
                    OpenOptions::new().write(true).create_new(true).mode(mode).open(&path).map_err(|e| failed(&e))?;
                let size = entry.size();
                let copied = io::copy(&mut entry, &mut file).map_err(|e| failed(&e))?;
                if copied != size {
                    return Err(failed(&format!("`{}` ends early", name.display())));
                }
            }
            EntryType::XGlobalHeader | EntryType::XHeader => {}
            other => {
                return Err(failed(&format!(
                    "`{}` is a {}; only files and folders are unpacked",
                    name.display(),
                    kind(other)
                )));
            }
        }
    }
    Ok(count)
}

fn inside(name: &Path) -> Option<PathBuf> {
    let mut relative = PathBuf::new();
    for part in name.components() {
        match part {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(relative)
}

fn no_links_above(root: &Path, relative: &Path) -> io::Result<()> {
    let mut path = root.to_path_buf();
    let parts: Vec<_> = relative.components().collect();
    for part in &parts[..parts.len().saturating_sub(1)] {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(io::Error::other(format!("{} is a symbolic link", path.display())));
            }
            Ok(meta) if !meta.is_dir() => {
                return Err(io::Error::other(format!("{} is not a folder", path.display())));
            }
            _ => {}
        }
    }
    Ok(())
}

fn remove(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path)?.is_dir() { fs::remove_dir_all(path) } else { fs::remove_file(path) }
}

fn kind(entry: EntryType) -> &'static str {
    match entry {
        EntryType::Symlink => "symbolic link",
        EntryType::Link => "hard link",
        EntryType::Char | EntryType::Block => "device",
        EntryType::Fifo => "named pipe",
        _ => "special entry",
    }
}

#[cfg(test)]
mod tests;
