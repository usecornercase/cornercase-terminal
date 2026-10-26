use std::fmt::Write as _;

use super::args::changes::Where;
use super::args::files::{FilesAction, FilesArgs};
use super::{print_json, print_out, stream_json, workspace};
use crate::client::{answer, ask};
use crate::control::changes::FileStatus;
use crate::control::files::{EntryKind, Found, Line, Listing, Read, ReadKind, SearchMode};
use crate::control::{Command, FilesList, FilesRead, FilesSearch};
use crate::error::{Error, Result};
use crate::files::disk::MAX_ENTRIES;
use crate::files::search::{MAX_MATCHES, MAX_NAMES};

pub(super) fn run(args: FilesArgs) -> Result<()> {
    match args.action {
        FilesAction::List { folders, place, follow, print } => list(folders, &place, follow, print.json),
        FilesAction::Read { path, place, follow, print } => read(path, &place, follow, print.json),
        FilesAction::Search { query, names, place, print, .. } => search(query, names, &place, print.json),
    }
}

fn place(command: &'static str, at: &Where) -> Result<Option<u64>> {
    workspace(command, at.workspace, at.worktree.as_deref())
}

fn list(folders: Vec<String>, at: &Where, follow: bool, json: bool) -> Result<()> {
    let command = Command::FilesList(FilesList { workspace: place("files list", at)?, folders, follow });
    if follow {
        return stream_json("files", command);
    }
    let value = ask("files-list", command)?;
    let line: Line<Listing> = answer(value.clone())?;
    if json {
        print_json(&value)?;
    } else {
        print_out(&list_text(&line.content))?;
    }
    notes(&line.content)
}

fn notes(listing: &Listing) -> Result<()> {
    let mut failed = false;
    for folder in &listing.folders {
        if folder.more {
            eprintln!("{}: only the first {MAX_ENTRIES} entries are listed", shown(&folder.path));
        }
        if let Some(error) = &folder.error {
            eprintln!("cannot list {}: {error}", shown(&folder.path));
            failed = true;
        }
    }
    if failed { Err(Error::Partial) } else { Ok(()) }
}

fn shown(path: &str) -> &str {
    if path.is_empty() { "." } else { path }
}

fn plain(text: &str) -> String {
    text.replace(|c: char| c.is_control(), " ")
}

fn status_name(status: FileStatus) -> &'static str {
    match status {
        FileStatus::Modified => "modified",
        FileStatus::Added => "added",
        FileStatus::Deleted => "deleted",
        FileStatus::Renamed => "renamed",
        FileStatus::Untracked => "untracked",
    }
}

fn kind_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "file",
        EntryKind::Folder => "folder",
        EntryKind::Other => "other",
    }
}

pub(super) fn list_text(listing: &Listing) -> String {
    let mut out = String::from("folder\tname\tkind\tstatus\tbytes\tmodified\n");
    for folder in &listing.folders {
        let path = plain(shown(&folder.path));
        for entry in &folder.entries {
            let status = entry.status.map_or("-", status_name);
            let bytes = entry.bytes.map_or_else(|| "-".to_string(), |bytes| bytes.to_string());
            let modified = entry.modified.as_deref().unwrap_or("-");
            let _ = writeln!(
                out,
                "{path}\t{}\t{}\t{status}\t{bytes}\t{modified}",
                plain(&entry.name),
                kind_name(entry.kind)
            );
        }
    }
    out
}

fn read(path: String, at: &Where, follow: bool, json: bool) -> Result<()> {
    let command = Command::FilesRead(FilesRead { workspace: place("files read", at)?, path, follow });
    if follow {
        return stream_json("files", command);
    }
    let value = ask("files-read", command)?;
    let line: Line<Read> = answer(value.clone())?;
    if json {
        print_json(&value)?;
        return read_text(&line.content).map(drop);
    }
    print_out(&read_text(&line.content)?)
}

pub(super) fn read_text(read: &Read) -> Result<String> {
    let path = &plain(&read.path);
    let bytes = read.bytes.unwrap_or_default();
    Ok(match (read.kind, &read.text, &read.image) {
        (ReadKind::Text, Some(text), _) => text.clone(),
        (ReadKind::Image, _, Some(image)) => {
            let size = image.width.zip(image.height).map_or_else(String::new, |(w, h)| format!(", {w}×{h}"));
            format!("{path}: {} image{size}, {bytes} bytes; --json gives its data\n", image.format)
        }
        (ReadKind::TooLarge, ..) => format!("{path}: too large to read, {bytes} bytes\n"),
        (ReadKind::Missing, ..) => return Err(Error::Control(format!("cannot read {path}: no such file"))),
        (ReadKind::Folder, ..) => {
            return Err(Error::Control(format!("{path} is a folder; `cornercase files list {path}` lists it")));
        }
        _ => format!("{path}: binary, {bytes} bytes\n"),
    })
}

fn search(query: String, names: bool, at: &Where, json: bool) -> Result<()> {
    let mode = if names { SearchMode::Names } else { SearchMode::Text };
    let value =
        ask("files-search", Command::FilesSearch(FilesSearch { workspace: place("files search", at)?, query, mode }))?;
    let found: Found = answer(value.clone())?;
    if found.capped {
        match mode {
            SearchMode::Names => eprintln!("only the best {MAX_NAMES} files are listed"),
            SearchMode::Text => eprintln!("only the first {MAX_MATCHES} matches are listed"),
        }
    }
    if json {
        return print_json(&value);
    }
    print_out(&search_text(&found))
}

pub(super) fn search_text(found: &Found) -> String {
    let mut out = String::new();
    for name in &found.names {
        let _ = writeln!(out, "{}", plain(&name.path));
    }
    for file in &found.files {
        for line in &file.lines {
            let _ = writeln!(out, "{}:{}:{}", plain(&file.path), line.number, plain(&line.text));
        }
    }
    if out.is_empty() { "nothing found\n".into() } else { out }
}

#[cfg(test)]
mod tests;
