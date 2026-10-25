use std::fmt::Write as _;
use std::fs::{OpenOptions, Permissions};
use std::io::{IsTerminal as _, Write as _};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::Path;

use serde_json::Value;

use super::render;
use crate::cli::{print_json, print_out, read_limited};
use crate::client::{answer, ask, confirm};
use crate::control::Command;
use crate::control::config::{
    ConfigExport, ConfigImport, Described, NoteKind, SetupChange, SetupExported, SetupNote, SetupPreview,
};
use crate::error::{Error, Result};
use crate::settings::share::{self, MAX_BYTES};

pub(super) struct Import {
    pub(super) source: String,
    pub(super) only: Vec<String>,
    pub(super) yes: bool,
    pub(super) preview: bool,
}

pub(super) fn export(to: Option<&Path>, json: bool) -> Result<()> {
    let value = ask("config export", Command::ConfigExport(ConfigExport {}))?;
    let exported: SetupExported = answer(value.clone())?;
    if !exported.left_out.is_empty() {
        eprintln!("left out: {}", exported.left_out.join("; "));
    }
    for warning in &exported.warnings {
        eprintln!("! {warning}");
    }
    if let Some(file) = to {
        written(file, &exported.text)?;
    }
    match (json, to) {
        (true, _) => print_json(&value),
        (false, None) => print_out(&exported.text),
        (false, Some(file)) => print_out(&wrote(&exported, file)),
    }
}

fn wrote(exported: &SetupExported, file: &Path) -> String {
    let (settings, repositories) = share::counts(exported.settings, exported.repositories);
    format!("exported {settings} and {repositories} to {}\n", file.display())
}

fn written(file: &Path, text: &str) -> Result<()> {
    if file.is_dir() {
        return Err(Error::WrongUsage(format!("`{}` is a folder: give the file to write", file.display())));
    }
    let only_mine = |mut out: std::fs::File| {
        out.set_permissions(Permissions::from_mode(0o600))?;
        out.set_len(0)?;
        out.write_all(text.as_bytes())
    };
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(file)
        .and_then(only_mine)
        .map_err(|e| Error::Control(format!("cannot write `{}`: {e}", file.display())))
}

pub(super) fn import(import: &Import, json: bool) -> Result<()> {
    let text = read_limited(Path::new(&import.source), MAX_BYTES, &format!("is {}", share::too_large()))?;
    let sections = (!import.only.is_empty()).then(|| import.only.clone());
    let mut request = ConfigImport { text, sections, apply: false };
    let value = ask("config import", Command::ConfigImport(request.clone()))?;
    if import.preview {
        return show_preview(&value, json);
    }
    let preview: SetupPreview = answer(value.clone())?;
    eprint!("{}", preview_text(&preview));
    if !changes_anything(&preview) {
        return stopped(&value, json, "nothing to import");
    }
    let refused = |_: &Error| {
        if json {
            let _ = print_json(&value);
        }
    };
    if !import.yes && !confirmed(&import.source).inspect_err(refused)? {
        return stopped(&value, json, "not imported");
    }
    request.apply = true;
    let value = ask("config import", Command::ConfigImport(request))?;
    if json {
        return print_json(&value);
    }
    print_out(&render(&answer::<Described>(value)?, true))
}

fn show_preview(value: &Value, json: bool) -> Result<()> {
    if json {
        return print_json(value);
    }
    print_out(&preview_text(&answer(value.clone())?))
}

fn confirmed(source: &str) -> Result<bool> {
    if source == "-" || !std::io::stdin().is_terminal() {
        return Err(Error::Control("nothing was imported: add --yes to import without asking".into()));
    }
    Ok(confirm("Import these settings? [y/N] "))
}

fn stopped(preview: &Value, json: bool, line: &str) -> Result<()> {
    if json {
        return print_json(preview);
    }
    print_out(&format!("{line}\n"))
}

fn changes_anything(preview: &SetupPreview) -> bool {
    preview.sections.iter().any(|section| !section.changes.is_empty())
}

pub(super) fn preview_text(preview: &SetupPreview) -> String {
    let mut out = String::new();
    for section in &preview.sections {
        if section.changes.is_empty() {
            let _ = writeln!(out, "{}: no changes", section.id);
        } else {
            let _ = writeln!(out, "{}", section.id);
        }
        for change in &section.changes {
            write_change(&mut out, change);
        }
        write_notes(&mut out, &section.notes, "  ");
    }
    write_notes(&mut out, &preview.notes, "");
    out
}

fn write_change(out: &mut String, change: &SetupChange) {
    let text = share::preview::change_text(change);
    let _ = writeln!(out, "  {}", text.head);
    for line in &text.script {
        let _ = writeln!(out, "      {line}");
    }
}

fn write_notes(out: &mut String, notes: &[SetupNote], indent: &str) {
    for note in notes {
        let mark = match note.kind {
            NoteKind::Risk | NoteKind::Secret => "!",
            NoteKind::Missing | NoteKind::Connect | NoteKind::Unknown | NoteKind::Repository | NoteKind::Other => "-",
        };
        let _ = writeln!(out, "{indent}{mark} {}", note.text);
    }
}

#[cfg(test)]
mod tests;
