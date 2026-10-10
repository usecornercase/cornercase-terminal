use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::SystemTime;

use super::TAIL;

pub(super) fn head(path: &Path) -> Option<serde_json::Value> {
    let mut bytes = Vec::new();
    BufReader::new(File::open(path).ok()?.take(64 * 1024)).read_until(b'\n', &mut bytes).ok()?;
    let end = bytes.iter().position(|b| *b == b'\n')?;
    serde_json::from_slice(&bytes[..end]).ok()
}

#[derive(Debug, Default)]
pub(super) struct Tail {
    read: u64,
    skipping: bool,
    stamp: Option<(u64, u64, SystemTime)>,
}

pub(super) struct Batch {
    pub reset: bool,
    pub skipped: bool,
    pub bytes: Vec<u8>,
}

impl Tail {
    pub fn read(&mut self, path: &Path) -> std::io::Result<Batch> {
        let info = std::fs::metadata(path)?;
        let stamp = (info.ino(), info.len(), info.modified()?);
        let reset = self.stamp.is_some_and(|s| s.0 != stamp.0 || stamp.1 < s.1 || (stamp.1 == s.1 && stamp.2 != s.2));
        if reset {
            *self = Self::default();
        }
        let mut batch = Batch { reset, skipped: false, bytes: Vec::new() };
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
