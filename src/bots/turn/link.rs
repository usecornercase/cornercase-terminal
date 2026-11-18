use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::os::fs::{canonical, create_private_dir_all, stat};

pub(super) const NAME: &str = if cfg!(windows) { "cornercase.exe" } else { "cornercase" };

static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) fn ensure(folder: &Path, binary: &Path) -> io::Result<()> {
    ensure_through(folder, binary, |from, to| crate::os::fs::link_to(from, to))
}

fn ensure_through(folder: &Path, binary: &Path, make: impl Fn(&Path, &Path) -> io::Result<()>) -> io::Result<()> {
    let link = folder.join(NAME);
    if runs_from(folder, binary) || leads_to(&link, binary) {
        return Ok(());
    }
    create_private_dir_all(folder)?;
    let temp = crate::persist::temporary(&link, std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))?;
    let _ = fs::remove_file(&temp);
    let result = make(binary, &temp).and_then(|()| keep_time(&temp, binary)).and_then(|()| fs::rename(&temp, &link));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn runs_from(folder: &Path, binary: &Path) -> bool {
    binary.parent().is_some_and(|parent| canonical(parent) == canonical(folder))
}

fn leads_to(link: &Path, binary: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(link) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        return fs::read_link(link).is_ok_and(|target| target == binary);
    }
    let (Ok(ours), Ok(theirs)) = (stat(link), stat(binary)) else {
        return false;
    };
    ours.same_file(&theirs) || same_copy(&ours.meta, &theirs.meta)
}

fn same_copy(ours: &fs::Metadata, theirs: &fs::Metadata) -> bool {
    match (ours.modified(), theirs.modified()) {
        (Ok(a), Ok(b)) => ours.len() == theirs.len() && a == b,
        _ => false,
    }
}

fn keep_time(made: &Path, binary: &Path) -> io::Result<()> {
    let (Ok(ours), Ok(theirs)) = (stat(made), stat(binary)) else {
        return Ok(());
    };
    if ours.same_file(&theirs) {
        return Ok(());
    }
    File::options().write(true).open(made)?.set_modified(theirs.meta.modified()?)
}

#[cfg(test)]
mod tests;
