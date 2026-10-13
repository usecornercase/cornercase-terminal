use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;

pub(crate) const SETTLE: Duration = Duration::from_secs(2);

pub(crate) fn save(path: &Path, value: &impl Serialize) -> io::Result<()> {
    write(path, value, false)
}

pub(crate) fn write(path: &Path, value: &impl Serialize, private: bool) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let bytes = if private { serde_json::to_vec(value)? } else { serde_json::to_vec_pretty(value)? };
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(if private { 0o600 } else { 0o666 })
        .open(&tmp)?;
    if private {
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(&bytes)?;
    if private {
        file.sync_all()?;
    }
    std::fs::rename(tmp, path)
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
