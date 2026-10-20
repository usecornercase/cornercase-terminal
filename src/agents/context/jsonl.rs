use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::SystemTime;

use super::TAIL;

pub(in crate::agents) fn head(path: &Path) -> Option<serde_json::Value> {
    let mut bytes = Vec::new();
    BufReader::new(File::open(path).ok()?.take(64 * 1024)).read_until(b'\n', &mut bytes).ok()?;
    let end = bytes.iter().position(|b| *b == b'\n')?;
    serde_json::from_slice(&bytes[..end]).ok()
}

pub(in crate::agents) fn values(bytes: &[u8]) -> Vec<serde_json::Value> {
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_slice(line).ok())
        .collect()
}

#[derive(Debug, Default)]
pub(in crate::agents) struct Tail {
    read: u64,
    skipping: bool,
    stamp: Option<(u64, u64, SystemTime)>,
}

pub(in crate::agents) struct Batch {
    pub(crate) reset: bool,
    pub(crate) skipped: bool,
    pub(crate) bytes: Vec<u8>,
}

impl Tail {
    pub(in crate::agents) fn read(&mut self, path: &Path) -> std::io::Result<Batch> {
        self.read_since(path, None)
    }

    fn skip_to_end(&mut self, path: &Path, len: u64) -> std::io::Result<()> {
        self.read = len;
        if len > 0 {
            let mut file = File::open(path)?;
            file.seek(SeekFrom::End(-1))?;
            let mut last = [0];
            file.read_exact(&mut last)?;
            self.skipping = last[0] != b'\n';
        }
        Ok(())
    }

    pub(in crate::agents) fn read_since(&mut self, path: &Path, since: Option<SystemTime>) -> std::io::Result<Batch> {
        let info = std::fs::metadata(path)?;
        let stamp = (info.ino(), info.len(), info.modified()?);
        let reset = self.stamp.is_some_and(|s| s.0 != stamp.0 || stamp.1 < s.1 || (stamp.1 == s.1 && stamp.2 != s.2));
        if reset {
            *self = Self::default();
        }
        let mut batch = Batch { reset, skipped: false, bytes: Vec::new() };
        if self.stamp.is_none() && since.is_some_and(|since| stamp.2 < since) {
            self.skip_to_end(path, info.len())?;
            self.stamp = Some(stamp);
            return Ok(batch);
        }
        if self.stamp == Some(stamp) && self.read == info.len() {
            return Ok(batch);
        }
        let start = if self.read == 0 { info.len().saturating_sub(TAIL) } else { self.read };
        let mut file = File::open(path)?;
        file.seek(SeekFrom::Start(start))?;
        let mut bytes = Vec::new();
        file.take((info.len() - start).min(TAIL)).read_to_end(&mut bytes)?;
        if let Some(end) = bytes.iter().rposition(|b| *b == b'\n').map(|i| i + 1) {
            let from = if self.skipping || (self.read == 0 && start > 0) {
                bytes.iter().position(|b| *b == b'\n').map_or(end, |i| i + 1)
            } else {
                0
            };
            batch.bytes.extend_from_slice(&bytes[from..end]);
            self.read = start + end as u64;
            self.skipping = false;
        } else if bytes.len() as u64 == TAIL {
            self.read = start + TAIL;
            self.skipping = true;
            batch.skipped = true;
        }
        self.stamp = Some(stamp);
        Ok(batch)
    }
}

#[cfg(test)]
mod tests;
