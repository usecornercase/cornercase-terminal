use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::act::UNREADABLE;
use super::link;
use crate::changes::diff::MAX_LINES;
use crate::control::changes::{FileInfo, Gap, GapLineInfo, HunkInfo, LineInfo, Patch, Segment, Summary};
use crate::control::contract::{Code, Refusal};
use crate::control::files::{EntryInfo, FolderInfo, Found, FoundFile, Listing, NameInfo, Read};
use crate::files::disk::MAX_ENTRIES;
use crate::files::search::{MAX_MATCHES, MAX_NAMES};
use crate::text;

const CONTEXT_LINES: usize = 3;
const LINES_KEPT: usize = MAX_LINES * (1 + 2 * CONTEXT_LINES);

pub(crate) fn summary(text: &str) -> Result<Summary, Refusal> {
    let mut summary: Summary = typed(text)?;
    summary.boot = link::clean(&summary.boot);
    let content = &mut summary.content;
    content.error = content.error.as_deref().map(link::clean);
    for workspace in &mut content.workspaces {
        workspace.base = workspace.base.as_deref().map(link::clean);
        workspace.files.iter_mut().for_each(clean_file);
    }
    Ok(summary)
}

pub(crate) fn patch(text: &str) -> Result<Patch, Refusal> {
    let mut patch: Patch = typed(text)?;
    clean_file(&mut patch.file);
    patch.hunks.truncate(MAX_LINES);
    let mut left = LINES_KEPT;
    for hunk in &mut patch.hunks {
        hunk.lines.truncate(left);
        left -= hunk.lines.len();
        clean_hunk(hunk);
    }
    Ok(patch)
}

pub(crate) fn gap(text: &str) -> Result<Gap, Refusal> {
    let mut gap: Gap = typed(text)?;
    gap.path = link::clean_path(&gap.path);
    gap.lines.truncate(MAX_LINES);
    gap.lines.iter_mut().for_each(clean_gap_line);
    Ok(gap)
}

pub(crate) fn listing(text: &str) -> Result<Listing, Refusal> {
    let mut listing: Listing = typed(text)?;
    listing.folders.truncate(MAX_ENTRIES);
    listing.folders.iter_mut().for_each(clean_folder);
    Ok(listing)
}

pub(crate) fn read(text: &str) -> Result<Read, Refusal> {
    let mut read: Read = typed(text)?;
    read.path = link::clean_path(&read.path);
    read.file = clean_buf(&read.file);
    read.modified = read.modified.as_deref().map(link::clean);
    read.language = read.language.as_deref().map(link::clean);
    read.text = read.text.as_deref().map(clean_code);
    if let Some(image) = &mut read.image {
        image.format = link::clean(&image.format);
    }
    Ok(read)
}

pub(crate) fn found(text: &str) -> Result<Found, Refusal> {
    let mut found: Found = typed(text)?;
    found.query = link::clean(&found.query);
    found.names.truncate(MAX_NAMES);
    found.names.iter_mut().for_each(clean_name);
    found.files.truncate(MAX_MATCHES);
    let mut left = MAX_MATCHES;
    for file in &mut found.files {
        file.lines.truncate(left);
        left -= file.lines.len();
        clean_found_file(file);
    }
    Ok(found)
}

fn clean_code(text: &str) -> String {
    let mut chars = text.chars().peekable();
    let mut kept = String::with_capacity(text.len());
    while let Some(c) = chars.next() {
        let breaks = c == '\r' && chars.peek() == Some(&'\n');
        if matches!(c, '\n' | '\t') || breaks || !text::hidden(c) {
            kept.push(c);
        }
    }
    kept
}

fn clean_code_line(text: &str) -> String {
    text.chars().filter(|&c| c == '\t' || !text::hidden(c)).collect()
}

fn typed<T: DeserializeOwned>(text: &str) -> Result<T, Refusal> {
    serde_json::from_value(answer(text)?).map_err(|_| unreadable())
}

fn answer(text: &str) -> Result<Value, Refusal> {
    link::replied(text).unwrap_or_else(|| Err(unreadable()))
}

fn unreadable() -> Refusal {
    Refusal::new(Code::Malformed, UNREADABLE)
}

fn clean_buf(path: &Path) -> PathBuf {
    PathBuf::from(link::clean_path(&path.to_string_lossy()))
}

fn clean_file(file: &mut FileInfo) {
    file.path = link::clean_path(&file.path);
    file.old_path = file.old_path.as_deref().map(link::clean_path);
    file.digest = link::clean(&file.digest);
}

fn clean_hunk(hunk: &mut HunkInfo) {
    hunk.context = link::clean(&hunk.context);
    hunk.lines.iter_mut().for_each(clean_line);
}

fn clean_line(line: &mut LineInfo) {
    line.text = clean_code_line(&line.text);
    line.emphasis.truncate(line.text.len());
    clean_segments(line.syntax.as_mut(), line.text.len());
}

fn clean_gap_line(line: &mut GapLineInfo) {
    line.text = clean_code_line(&line.text);
    clean_segments(line.syntax.as_mut(), line.text.len());
}

fn clean_segments(segments: Option<&mut Vec<Segment>>, length: usize) {
    if let Some(segments) = segments {
        segments.truncate(length.saturating_add(1));
        for segment in segments.iter_mut() {
            segment.text = clean_code_line(&segment.text);
        }
    }
}

fn clean_folder(folder: &mut FolderInfo) {
    folder.path = link::clean_path(&folder.path);
    folder.dir = clean_buf(&folder.dir);
    folder.error = folder.error.as_deref().map(link::clean);
    folder.entries.truncate(MAX_ENTRIES);
    folder.entries.iter_mut().for_each(clean_entry);
}

fn clean_entry(entry: &mut EntryInfo) {
    entry.name = link::clean_path(&entry.name);
    entry.modified = entry.modified.as_deref().map(link::clean);
}

fn clean_name(name: &mut NameInfo) {
    name.path = link::clean_path(&name.path);
    name.indices.truncate(name.path.chars().count());
}

fn clean_found_file(file: &mut FoundFile) {
    file.path = link::clean_path(&file.path);
    for line in &mut file.lines {
        line.text = clean_code_line(&line.text);
        line.ranges.truncate(line.text.len());
    }
}

#[cfg(test)]
mod tests;
