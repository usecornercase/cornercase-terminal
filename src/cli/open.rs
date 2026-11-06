use serde_json::Value;

use super::args::OpenArgs;
use super::{id, note, partial_unless, print_json, print_out, reach, say};
use crate::client::{answer, ask, use_open_into};
use crate::control::{Command, ImportedWorkspace, Open};
use crate::error::Result;

pub(super) fn run(open: &OpenArgs) -> Result<()> {
    if open.into.is_some() {
        use_open_into()?;
    }
    let path = reach::open_path(&open.path)?;
    let value = ask("open", Command::Open(Open { path, focus: open.create.focus, into: open.into }))?;
    if value.get("projects").is_some() {
        return say_imported(&value, open.create.print.json);
    }
    say(value, open.create.print.json, |done| id(done.ids.project))
}

fn say_imported(value: &Value, json: bool) -> Result<()> {
    let imported: ImportedWorkspace = answer(value.clone())?;
    if json {
        print_json(value)?;
    } else {
        print_imported(&imported)?;
    }
    partial_unless(imported.whole()).map(drop)
}

fn print_imported(imported: &ImportedWorkspace) -> Result<()> {
    if let Some(group) = imported.group {
        print_out(&format!("{group}\n"))?;
    }
    if !imported.whole() {
        note(&format!("warning: {}\n", imported.message()));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
