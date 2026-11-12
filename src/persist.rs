use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

use crate::os::fs::{OpenOptionsExt, Stat, set_file_mode, stat, stat_file};

const NAME_LIMIT: usize = 255;
const TEMPORARY_END: &str = ".tmp";
const PID_DIGITS: usize = u32::MAX.ilog10() as usize + 1;
const COUNT_DIGITS: usize = u64::MAX.ilog10() as usize + 1;

pub(crate) const MAX_NAME: usize =
    NAME_LIMIT - (".".len() + PID_DIGITS + ".".len() + COUNT_DIGITS + TEMPORARY_END.len());

const TRIES: u32 = 16;

pub(crate) const SETTLE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Public,
    Synced,
    Private,
}

impl Kind {
    fn mode(self) -> u32 {
        if self == Self::Private { 0o600 } else { 0o666 }
    }
}

pub(crate) type Mark = Option<((u64, u64), SystemTime, u64)>;

pub(crate) fn mark(path: &Path) -> Mark {
    stat(path).ok().as_ref().and_then(mark_of)
}

fn mark_of(stat: &Stat) -> Mark {
    Some((stat.id(), stat.meta.modified().ok()?, stat.meta.len()))
}

pub(crate) fn save(path: &Path, value: &impl Serialize) -> io::Result<()> {
    write(path, value, false)
}

pub(crate) fn write(path: &Path, value: &impl Serialize, private: bool) -> io::Result<()> {
    write_marked(path, value, private).map(drop)
}

pub(crate) fn write_marked(path: &Path, value: &impl Serialize, private: bool) -> io::Result<Mark> {
    let bytes = if private { serde_json::to_vec(value)? } else { serde_json::to_vec_pretty(value)? };
    write_bytes(path, &bytes, if private { Kind::Private } else { Kind::Public })
}

pub(crate) fn write_bytes(path: &Path, bytes: &[u8], kind: Kind) -> io::Result<Mark> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let (temp, mut file) = create_temporary(path, kind, &NEXT)?;
    let filled = fill(&mut file, bytes, kind);
    drop(file);
    let written = filled.and_then(|mark| std::fs::rename(&temp, path).map(|()| mark));
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

fn create_temporary(path: &Path, kind: Kind, next: &AtomicU64) -> io::Result<(PathBuf, File)> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(kind.mode());
    for _ in 0..TRIES {
        let temp = temporary(path, std::process::id(), next.fetch_add(1, Ordering::Relaxed))?;
        match options.open(&temp) {
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            opened => return opened.map(|file| (temp, file)),
        }
    }
    let message = format!("no free temporary name beside {}", path.display());
    Err(io::Error::new(io::ErrorKind::AlreadyExists, message))
}

fn temporary(path: &Path, pid: u32, n: u64) -> io::Result<PathBuf> {
    let Some(name) = path.file_name() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "a file to write needs a name"));
    };
    let mut name = name.to_os_string();
    name.push(format!(".{pid}.{n}{TEMPORARY_END}"));
    Ok(path.with_file_name(name))
}

fn fill(file: &mut File, bytes: &[u8], kind: Kind) -> io::Result<Mark> {
    if kind == Kind::Private {
        set_file_mode(file, 0o600)?;
    }
    file.write_all(bytes)?;
    if kind != Kind::Public {
        file.sync_all()?;
    }
    Ok(mark_of(&stat_file(file)?))
}

pub(crate) fn back_up(path: &Path) -> io::Result<PathBuf> {
    let backup = path.with_extension("json.bak");
    if backup.exists() {
        std::fs::rename(&backup, path.with_extension("json.bak.1"))?;
    }
    std::fs::copy(path, &backup)?;
    Ok(backup)
}

#[derive(Debug)]
pub(crate) struct Saver<T> {
    path: PathBuf,
    saved: Option<T>,
    pending: Option<(T, Instant)>,
}

impl<T: PartialEq + Serialize> Saver<T> {
    pub(crate) fn new(path: PathBuf, saved: Option<T>) -> Self {
        Self { path, saved, pending: None }
    }

    pub(crate) fn observe(&mut self, state: T, now: Instant) -> io::Result<bool> {
        if self.saved.as_ref() == Some(&state) {
            self.pending = None;
            return Ok(false);
        }
        match &self.pending {
            Some((pending, since)) if *pending == state => {
                if now.duration_since(*since) >= SETTLE {
                    save(&self.path, &state)?;
                    self.saved = Some(state);
                    self.pending = None;
                    return Ok(true);
                }
            }
            _ => self.pending = Some((state, now)),
        }
        Ok(false)
    }

    pub(crate) fn flush(&mut self) -> io::Result<()> {
        if let Some((state, _)) = self.pending.take() {
            save(&self.path, &state)?;
            self.saved = Some(state);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
