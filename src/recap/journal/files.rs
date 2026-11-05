use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use super::Entry;
use crate::os::fs::OpenOptionsExt;

const DAY: i64 = 86_400;

fn day_file(dir: &Path, day: &str) -> PathBuf {
    dir.join(format!("{day}.jsonl"))
}

pub(crate) fn day_of(at: &str) -> Option<&str> {
    at.get(..10).filter(|day| day.as_bytes().get(4) == Some(&b'-') && day.as_bytes().get(7) == Some(&b'-'))
}

pub(crate) fn append(dir: &Path, entries: &[Entry]) -> std::io::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    crate::os::fs::create_private_dir_all(dir)?;
    for entry in entries {
        let Some(day) = day_of(&entry.at) else { continue };
        let mut file = OpenOptions::new().create(true).append(true).mode(0o600).open(day_file(dir, day))?;
        let mut line = serde_json::to_vec(entry)?;
        line.push(b'\n');
        file.write_all(&line)?;
    }
    Ok(())
}

fn days(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(listing) = fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<(String, PathBuf)> = listing
        .flatten()
        .filter_map(|item| {
            let name = item.file_name().to_string_lossy().into_owned();
            let day = name.strip_suffix(".jsonl")?;
            day_of(day).filter(|d| d.len() == day.len())?;
            Some((day.to_string(), item.path()))
        })
        .collect();
    found.sort();
    found
}

fn read_file(path: &Path) -> Vec<Entry> {
    let Ok(file) = fs::File::open(path) else { return Vec::new() };
    BufReader::new(file).lines().map_while(Result::ok).filter_map(|line| serde_json::from_str(&line).ok()).collect()
}

fn day_number(day: &str) -> Option<i64> {
    let at = format!("{day}T00:00:00.000Z");
    crate::time::Stamp::parse(&at)
        .and_then(|time| time.duration_since(std::time::SystemTime::UNIX_EPOCH).ok())
        .and_then(|since| i64::try_from(since.as_secs() / 86_400).ok())
}

pub(crate) fn read(dir: &Path, from: i64, to: i64) -> Vec<Entry> {
    let (first, last) = ((from - DAY).div_euclid(DAY), (to + DAY).div_euclid(DAY));
    let mut entries = Vec::new();
    for (day, path) in days(dir) {
        if day_number(&day).is_some_and(|number| number >= first && number <= last) {
            entries.extend(read_file(&path));
        }
    }
    entries.retain(|entry| entry.seconds().is_some_and(|at| at >= from && at < to));
    entries
}

pub(crate) fn prune(dir: &Path, now: i64, kept_days: i64) -> usize {
    let oldest = now.div_euclid(DAY) - kept_days;
    let mut removed = 0;
    for (day, path) in days(dir) {
        if day_number(&day).is_some_and(|number| number < oldest) && fs::remove_file(path).is_ok() {
            removed += 1;
        }
    }
    removed
}
