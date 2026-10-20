use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{failed, private_folder};
use crate::error::Error;

pub(crate) trait Linker {
    fn link(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::hard_link(from, to)
    }

    fn copy(&self, from: &Path, to: &mut File) -> io::Result<u64> {
        io::copy(&mut File::open(from)?, to)
    }
}

pub(crate) struct Disk;

impl Linker for Disk {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Linked,
    Copied,
    Present,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Tally {
    pub(crate) linked: usize,
    pub(crate) copied: usize,
    pub(crate) present: usize,
}

impl Tally {
    fn add(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Linked => self.linked += 1,
            Outcome::Copied => self.copied += 1,
            Outcome::Present => self.present += 1,
        }
    }
}

pub(crate) fn conversation(home: &Path, files: &[(PathBuf, PathBuf)], linker: &dyn Linker) -> Result<Tally, Error> {
    for apply in [false, true] {
        let mut tally = Tally::default();
        for (source, relative) in files {
            let dest = inside(home, relative)?;
            let meta = fs::symlink_metadata(source).map_err(|e| failed("read", source, &e))?;
            if meta.is_dir() {
                folder(source, &dest, linker, apply, &mut tally)?;
            } else if meta.is_file() {
                tally.add(place(source, &dest, linker, apply)?);
            } else {
                return Err(Error::Account(format!("`{}` is not a file or a folder", source.display())));
            }
        }
        if apply {
            return Ok(tally);
        }
    }
    Ok(Tally::default())
}

fn place(source: &Path, dest: &Path, linker: &dyn Linker, apply: bool) -> Result<Outcome, Error> {
    if apply {
        return file(source, dest, linker);
    }
    Ok(present(source, dest)?.unwrap_or(Outcome::Linked))
}

fn inside(home: &Path, relative: &Path) -> Result<PathBuf, Error> {
    let plain = relative.components().all(|part| matches!(part, Component::Normal(_)));
    if !plain || relative.as_os_str().is_empty() {
        return Err(Error::Account(format!("`{}` is not a path inside the account", relative.display())));
    }
    Ok(home.join(relative))
}

fn folder(source: &Path, dest: &Path, linker: &dyn Linker, apply: bool, tally: &mut Tally) -> Result<(), Error> {
    if apply {
        private_folder(dest).map_err(|e| failed("create", dest, &e))?;
    } else if let Ok(meta) = fs::symlink_metadata(dest)
        && !meta.is_dir()
    {
        let why = if meta.is_symlink() { super::LINKED } else { "it is already there and is not a folder" };
        return Err(Error::Account(format!("`{}`: {why}", dest.display())));
    }
    let entries = fs::read_dir(source).map_err(|e| failed("read", source, &e))?;
    let mut entries: Vec<_> = entries.collect::<io::Result<_>>().map_err(|e| failed("read", source, &e))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let from = entry.path();
        let to = dest.join(entry.file_name());
        let kind = entry.file_type().map_err(|e| failed("read", &from, &e))?;
        if kind.is_dir() {
            folder(&from, &to, linker, apply, tally)?;
        } else if kind.is_file() {
            tally.add(place(&from, &to, linker, apply)?);
        }
    }
    Ok(())
}

fn file(source: &Path, dest: &Path, linker: &dyn Linker) -> Result<Outcome, Error> {
    if let Some(outcome) = present(source, dest)? {
        return Ok(outcome);
    }
    let parent = dest.parent().ok_or_else(|| Error::Account(format!("`{}` has no folder", dest.display())))?;
    private_folder(parent).map_err(|e| failed("create", parent, &e))?;
    match linker.link(source, dest) {
        Ok(()) => return Ok(Outcome::Linked),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            return present(source, dest)?.ok_or_else(|| failed("link", dest, &e));
        }
        Err(_) => {}
    }
    copied(source, dest, parent, linker)
}

fn present(source: &Path, dest: &Path) -> Result<Option<Outcome>, Error> {
    let meta = match fs::symlink_metadata(dest) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(failed("read", dest, &e)),
    };
    if !meta.is_file() {
        return Err(Error::Account(format!("`{}` is already there and is not a file", dest.display())));
    }
    if same(source, dest).map_err(|e| failed("compare", dest, &e))? {
        Ok(Some(Outcome::Present))
    } else {
        Err(Error::Account(format!("`{}` is already there with other contents", dest.display())))
    }
}

fn copied(source: &Path, dest: &Path, parent: &Path, linker: &dyn Linker) -> Result<Outcome, Error> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = dest.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!(".{name}.cornercase-{}-{n}", std::process::id()));
    let result = copy_into(source, dest, &temp, linker);
    let _ = fs::remove_file(&temp);
    result
}

fn copy_into(source: &Path, dest: &Path, temp: &Path, linker: &dyn Linker) -> Result<Outcome, Error> {
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(temp)
        .map_err(|e| failed("create", temp, &e))?;
    linker.copy(source, &mut out).map_err(|e| failed("copy", source, &e))?;
    out.sync_all().map_err(|e| failed("write", temp, &e))?;
    drop(out);
    if !same(source, temp).map_err(|e| failed("compare", temp, &e))? {
        return Err(Error::Account(format!(
            "`{}` changed while it was copied to the other account; try again",
            source.display()
        )));
    }
    match fs::hard_link(temp, dest) {
        Ok(()) => Ok(Outcome::Copied),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            present(source, dest)?.ok_or_else(|| failed("copy", dest, &e))
        }
        Err(e) => Err(failed("copy", dest, &e)),
    }
}

fn same(a: &Path, b: &Path) -> io::Result<bool> {
    let (meta_a, meta_b) = (fs::metadata(a)?, fs::metadata(b)?);
    if meta_a.dev() == meta_b.dev() && meta_a.ino() == meta_b.ino() {
        return Ok(true);
    }
    if meta_a.len() != meta_b.len() {
        return Ok(false);
    }
    let (mut a, mut b) = (File::open(a)?, File::open(b)?);
    let (mut buf_a, mut buf_b) = (vec![0; 64 * 1024], vec![0; 64 * 1024]);
    loop {
        let n = fill(&mut a, &mut buf_a)?;
        let m = fill(&mut b, &mut buf_b)?;
        if n != m || buf_a[..n] != buf_b[..m] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

fn fill(file: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut read = 0;
    while read < buf.len() {
        match file.read(&mut buf[read..]) {
            Ok(0) => break,
            Ok(n) => read += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(read)
}

#[cfg(test)]
mod tests;
