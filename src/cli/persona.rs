use std::fmt::Write as _;
use std::io::IsTerminal as _;
use std::path::Path;

use super::args::persona::{PersonaAction, PersonaEditArgs, PersonaIdArgs, PersonaImportArgs, PersonaNewArgs};
use super::{note, print_json, print_out, read_limited};
use crate::client::{answer, ask, confirm_on_stderr};
use crate::control::Command;
use crate::control::persona::{
    CHECK, PersonaImport, PersonaInfo, PersonaList, PersonaRemove, PersonaRemoved, PersonaSave, PersonaSaved,
    PersonaShow, PersonaShown, PersonaText, Personas, PersonasImported,
};
use crate::error::{Error, Result};
use crate::flows::persona::draft::{self, Draft};
use crate::flows::persona::source::{self, Gathered, Hosts, Source};
use crate::flows::persona::{MAX_BYTES, parse};
use crate::flows::share::visible;

pub(super) fn run(action: PersonaAction) -> Result<()> {
    match action {
        PersonaAction::List(print) => {
            let value = ask("persona list", Command::PersonaList(PersonaList::default()))?;
            if print.json {
                return print_json(&value);
            }
            print_out(&listing(&answer(value)?))
        }
        PersonaAction::Import(args) => import(&args),
        PersonaAction::Remove(args) => remove(&args),
        PersonaAction::Show(args) => show(&args),
        PersonaAction::New(args) => new(&args),
        PersonaAction::Edit(args) => edit(&args),
    }
}

fn show(args: &PersonaIdArgs) -> Result<()> {
    let value = ask("persona show", Command::PersonaShow(PersonaShow { id: args.id.clone() }))?;
    if args.print.json {
        return print_json(&value);
    }
    let shown: PersonaShown = answer(value)?;
    print_out(&visible_text(&shown.text))
}

fn visible_text(text: &str) -> String {
    text.lines().map(|line| format!("{}\n", visible(line))).collect()
}

fn instructions(file: &Path) -> Result<String> {
    read_limited(file, MAX_BYTES, "is too long for a persona (at most 256 KiB)")
}

fn new(args: &PersonaNewArgs) -> Result<()> {
    let draft = Draft {
        name: args.name.clone(),
        description: args.description.clone().unwrap_or_default(),
        emoji: args.emoji.clone().unwrap_or_default(),
        body: instructions(&args.instructions)?,
    };
    let save =
        PersonaSave { id: args.id.clone().unwrap_or_default(), text: draft::compose(&draft, None), replace: false };
    saved(save, args.print.json)
}

fn edit(args: &PersonaEditArgs) -> Result<()> {
    let value = ask("persona show", Command::PersonaShow(PersonaShow { id: args.id.clone() }))?;
    let shown: PersonaShown = answer(value)?;
    let was = parse(&shown.persona.id, &shown.text).map_err(|e| Error::Control(visible(&e)))?;
    let body = match &args.instructions {
        Some(file) => instructions(file)?,
        None => draft::body(&shown.text).to_string(),
    };
    let draft = Draft {
        name: args.name.clone().unwrap_or(was.name),
        description: args.description.clone().unwrap_or(was.description),
        emoji: args.emoji.clone().unwrap_or(was.emoji),
        body,
    };
    let text = draft::compose(&draft, Some(&shown.text));
    saved(PersonaSave { id: shown.persona.id, text, replace: true }, args.print.json)
}

fn saved(save: PersonaSave, json: bool) -> Result<()> {
    let value = ask("persona save", Command::PersonaSave(save))?;
    if json {
        return print_json(&value);
    }
    let saved: PersonaSaved = answer(value)?;
    let path = saved.persona.path.as_ref().map(|path| format!(": {}", path.display())).unwrap_or_default();
    print_out(&format!("saved the persona {} as {}{path}\n", visible(&saved.persona.name), saved.persona.id))
}

fn remove(args: &PersonaIdArgs) -> Result<()> {
    let value = ask("persona remove", Command::PersonaRemove(PersonaRemove { id: args.id.clone() }))?;
    if args.print.json {
        return print_json(&value);
    }
    let removed: PersonaRemoved = answer(value)?;
    print_out(&format!("removed the persona {}: {}\n", visible(&removed.id), removed.path.display()))
}

fn gathered(source: &Source) -> Result<Gathered> {
    if *source == Source::Stdin {
        let text = read_limited(Path::new("-"), MAX_BYTES, "is too long for a persona (at most 256 KiB)")?;
        return source::single("", text, None, "standard input").map_err(Error::Control);
    }
    if source.is_web() {
        note(&format!("fetching {}\n", source.label()));
    }
    source::gather(source, &Hosts::from_env()).map_err(|e| Error::Control(visible(&e)))
}

fn import(args: &PersonaImportArgs) -> Result<()> {
    let source = source::source(&args.source, source::on_disk).map_err(Error::Control)?;
    let gathered = gathered(&source)?;
    if gathered.skipped > 0 {
        let files = if gathered.skipped == 1 {
            "1 file that is not a persona".to_string()
        } else {
            format!("{} files that are not personas", gathered.skipped)
        };
        note(&format!("skipped {files}\n"));
    }
    let personas = gathered.found.into_iter().map(|found| PersonaText {
        file: found.file,
        text: found.text,
        source: found.source,
    });
    let mut request = PersonaImport { personas: personas.collect(), replace: args.replace, check: true };
    let preview = ask("persona import", Command::PersonaImport(request.clone()))?;
    if args.check {
        let preview = with_warning(preview, &source);
        return if args.print.json { print_json(&preview) } else { print_out(&checked(&answer(preview)?)) };
    }
    if source.is_web() && !args.yes && !confirmed(&answer(preview.clone())?, &source)? {
        return if args.print.json { print_json(&with_warning(preview, &source)) } else { print_out("not imported\n") };
    }
    request.check = false;
    let value = ask("persona import", Command::PersonaImport(request))?;
    if args.print.json {
        return print_json(&value);
    }
    print_out(&imported(&answer(value)?))
}

fn warning(source: &Source, check: &str) -> String {
    if !source.is_web() {
        return visible(check);
    }
    visible(&format!("from {} (see {}), which nobody here reviewed: {check}", source.label(), source.page()))
}

fn with_warning(mut value: serde_json::Value, source: &Source) -> serde_json::Value {
    let check = value.get("check").and_then(serde_json::Value::as_str).unwrap_or(CHECK).to_string();
    value["check"] = warning(source, &check).into();
    value
}

fn confirmed(preview: &PersonasImported, source: &Source) -> Result<bool> {
    note(&lines(&preview.personas));
    let risks = warning(source, &preview.check);
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(Error::Control(format!("{risks}; pass --yes to import them")));
    }
    let count = preview.personas.len();
    let them = if count == 1 { "it" } else { "them" };
    Ok(confirm_on_stderr(&format!("{risks}\nImport {them} into your personas? [y/N] ")))
}

fn persona_line(persona: &PersonaInfo) -> String {
    let emoji = if persona.emoji.is_empty() { String::new() } else { format!("{} ", persona.emoji) };
    let kib = persona.bytes.div_ceil(1024);
    let replaced = if persona.replaced { ", replaces yours" } else { "" };
    let mut out =
        format!("{}: {}{} · {kib} KiB{replaced}\n", visible(&persona.id), visible(&emoji), visible(&persona.name));
    if !persona.description.is_empty() {
        let _ = writeln!(out, "  {}", visible(&persona.description));
    }
    out
}

fn lines(personas: &[PersonaInfo]) -> String {
    personas.iter().map(persona_line).collect()
}

fn checked(preview: &PersonasImported) -> String {
    let count = preview.personas.len();
    let what = if count == 1 { "1 persona".to_string() } else { format!("{count} personas") };
    format!("{}{}\nwould import {what}; nothing was imported\n", lines(&preview.personas), preview.check)
}

fn imported(answer: &PersonasImported) -> String {
    let mut out = String::new();
    for persona in &answer.personas {
        let path = persona.path.as_ref().map(|path| format!(": {}", path.display())).unwrap_or_default();
        let replaced = if persona.replaced { " (replaced)" } else { "" };
        let _ = writeln!(out, "imported {} as {}{replaced}{path}", visible(&persona.name), persona.id);
    }
    out
}

pub(super) fn listing(personas: &Personas) -> String {
    let mut out = String::new();
    for persona in &personas.personas {
        out.push_str(&persona_line(persona));
        if let Some(source) = &persona.source {
            let _ = writeln!(out, "  from {}", visible(source));
        }
    }
    for broken in &personas.broken {
        let _ = writeln!(out, "cannot read {}: {}", broken.path.display(), visible(&broken.error));
    }
    if out.is_empty() {
        let _ = writeln!(
            out,
            "no personas in {} yet: `cornercase persona import OWNER/REPO/PATH` imports some",
            personas.dir.display()
        );
    }
    out
}

#[cfg(test)]
mod tests;
