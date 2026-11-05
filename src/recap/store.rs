use std::path::Path;

use super::{Choice, NOW_FRESH, Recap};
use crate::persist;

pub(crate) fn load(folder: &Path, choice: Choice) -> Option<Recap> {
    let text = std::fs::read_to_string(folder.join(choice.name()).join("recap.json")).ok()?;
    serde_json::from_str(&text).ok()
}

pub(crate) fn save(folder: &Path, choice: Choice, recap: &Recap) -> std::io::Result<()> {
    let dir = folder.join(choice.name());
    crate::os::fs::create_private_dir_all(&dir)?;
    persist::write(&dir.join("recap.json"), recap, true)?;
    let md = dir.join("recap.md");
    let tmp = dir.join("recap.md.tmp");
    write_private(&tmp, recap.text.as_bytes())?;
    std::fs::rename(tmp, md)
}

pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    use crate::os::fs::OpenOptionsExt;

    let mut file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    file.write_all(bytes)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Moment<'a> {
    pub(crate) now: i64,
    pub(crate) today: &'a str,
    pub(crate) last_day: Option<&'a str>,
}

pub(crate) fn due(choice: Choice, cached: Option<&Recap>, at: Moment<'_>) -> bool {
    let Some(cached) = cached else { return true };
    match choice {
        Choice::Now => at.now - cached.written_at >= i64::try_from(NOW_FRESH.as_secs()).unwrap_or(i64::MAX),
        Choice::Yesterday => at.last_day != cached.day.as_deref(),
        Choice::Week => cached.day.as_deref() != Some(at.today),
    }
}

#[cfg(test)]
mod tests;
