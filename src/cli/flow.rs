use std::fmt::Write as _;
use std::io::{IsTerminal as _, Read as _};
use std::path::Path;

use super::args::{FlowAction, FlowDeleteArgs, FlowImportArgs, ImportTo};
use super::{print_json, print_out, read_prompt};
use crate::client::{self, answer, ask, confirm};
use crate::control::{self, Exported, FlowInfo, FlowRemoved, Flows, Imported, RunInfo};
use crate::error::{Error, Result};
use crate::flows::Worktree;
use crate::flows::registry::{self, Source};
use crate::flows::share;

pub(super) fn run(action: FlowAction) -> Result<()> {
    match action {
        FlowAction::List(list) => {
            let value = ask("flow list", control::Command::FlowList(control::FlowList { project: list.project }))?;
            if list.print.json {
                return print_json(&value);
            }
            print_out(&listing(&answer(value)?))
        }
        FlowAction::Run(run) => {
            if run.issue.key.is_some() {
                client::use_issues()?;
            }
            let request = control::FlowRun {
                name: run.name,
                prompt: read_prompt(run.prompt, run.prompt_file)?.unwrap_or_default(),
                worktree: run.worktree,
                project: run.project,
                issue: run.issue.key,
                source: run.issue.source,
                focus: run.create.focus,
            };
            let value = ask("flow run", control::Command::FlowRun(request))?;
            if run.create.print.json {
                return print_json(&value);
            }
            let run: RunInfo = answer(value)?;
            if let Some(note) = &run.note {
                eprintln!("{note}");
            }
            if let Some(note) = folder_note(&run) {
                eprintln!("{note}");
            }
            print_out(&started(&run))
        }
        FlowAction::Export(export) => {
            let request = control::FlowExport { name: export.name, project: export.project };
            let value = ask("flow export", control::Command::FlowExport(request))?;
            if export.print.json {
                return print_json(&value);
            }
            let exported: Exported = answer(value)?;
            if let Some(note) = &exported.note {
                eprintln!("{note}");
            }
            print_out(&exported.text)
        }
        FlowAction::Import(import) => import_flow(import),
        FlowAction::Delete(delete) => delete_flow(delete),
        FlowAction::Hide(hide) => {
            let request = control::FlowHide { name: hide.name };
            removed("flow hide", control::Command::FlowHide(request), hide.print.json, hidden_line)
        }
        FlowAction::Restore(restore) => {
            let request = control::FlowRestore { name: restore.name };
            removed("flow restore", control::Command::FlowRestore(request), restore.print.json, restored_line)
        }
    }
}

fn delete_flow(args: FlowDeleteArgs) -> Result<()> {
    let request = control::FlowDelete {
        name: args.name.unwrap_or_default(),
        place: args.place.map(|place| place.name().to_string()).unwrap_or_default(),
        project: args.project,
        path: args.path.map(std::path::absolute).transpose()?,
    };
    removed("flow delete", control::Command::FlowDelete(request), args.print.json, deleted_line)
}

fn removed(
    name: &'static str,
    command: control::Command,
    json: bool,
    line: impl Fn(&FlowRemoved) -> String,
) -> Result<()> {
    let value = ask(name, command)?;
    if json {
        return print_json(&value);
    }
    print_out(&line(&answer(value)?))
}

pub(super) fn deleted_line(removed: &FlowRemoved) -> String {
    let place = match removed.origin.as_deref() {
        Some("project") => " (this project's)",
        Some(_) => " (yours)",
        None => "",
    };
    let path = removed.deleted.as_ref().map(|path| format!(": {}", path.display())).unwrap_or_default();
    format!("deleted {}{place}{path}\n", share::visible(&removed.name))
}

pub(super) fn hidden_line(removed: &FlowRemoved) -> String {
    let name = share::visible(&removed.name);
    if !removed.changed {
        return format!("the built-in flow {name} was already deleted\n");
    }
    let edit =
        removed.deleted.as_ref().map(|path| format!(", and your version {}", path.display())).unwrap_or_default();
    format!("deleted the built-in flow {name}{edit}; `cornercase flow restore {name}` brings it back\n")
}

pub(super) fn restored_line(removed: &FlowRemoved) -> String {
    let name = share::visible(&removed.name);
    if !removed.changed {
        return format!("the built-in flow {name} is shown and not edited: nothing to restore\n");
    }
    let edit = removed
        .deleted
        .as_ref()
        .map(|path| format!("; your version {} is deleted", path.display()))
        .unwrap_or_default();
    format!("restored the built-in flow {name}{edit}\n")
}

fn import_flow(args: FlowImportArgs) -> Result<()> {
    let source = registry::source(&args.source, Path::exists).map_err(Error::Control)?;
    let from = args.source.clone();
    let text = match &source {
        Source::Stdin => read_shared(Path::new("-"))?,
        Source::File(path) => read_shared(path)?,
        remote => {
            eprintln!("fetching {}", remote.label());
            registry::fetch(remote).map_err(|e| Error::Control(share::visible(&e)))?
        }
    };
    let flow =
        share::from_share(&text).map_err(|e| Error::Control(share::visible(&format!("cannot read `{from}`: {e}"))))?;
    eprint!("{}", share::summary(&flow));
    let risks = share::risks(&flow, "it");
    let asked = match &source {
        Source::Url(_) | Source::Repo(_) => Some(fetched(&source, risks)),
        _ => risks,
    };
    let mut request = control::FlowImport {
        text,
        name: args.name,
        project: args.project,
        place: args.to.name().to_string(),
        replace: args.replace,
        check: false,
    };
    if let Some(risks) = asked.filter(|_| !args.yes) {
        request.check = true;
        ask("flow import", control::Command::FlowImport(request.clone()))?;
        request.check = false;
        let asking = source != Source::Stdin && std::io::stdin().is_terminal();
        if !asking {
            return Err(Error::Control(format!("{risks}; pass --yes to import it")));
        }
        if !confirm(&confirmation(&risks, args.to)) {
            println!("not imported");
            return Ok(());
        }
    }
    let value = ask("flow import", control::Command::FlowImport(request))?;
    if args.print.json {
        return print_json(&value);
    }
    let imported: Imported = answer(value)?;
    if let Some(note) = &imported.note {
        eprintln!("{note}");
    }
    print_out(&imported_line(&imported))
}

pub(super) fn fetched(source: &Source, risks: Option<String>) -> String {
    let rest = risks.map(|risks| format!("; {risks}")).unwrap_or_default();
    share::visible(&format!(
        "from {}, which nobody here reviewed: its prompts drive agents that never ask for permission; read them above or at {}{rest}",
        source.label(),
        source.page()
    ))
}

fn read_shared(file: &Path) -> Result<String> {
    let limit = share::MAX_BYTES as u64 + 1;
    let mut bytes = Vec::new();
    if file == Path::new("-") {
        std::io::stdin().take(limit).read_to_end(&mut bytes)?;
    } else {
        let open =
            std::fs::File::open(file).map_err(|e| Error::Control(format!("cannot read `{}`: {e}", file.display())))?;
        open.take(limit).read_to_end(&mut bytes)?;
    }
    if bytes.len() > share::MAX_BYTES {
        return Err(Error::Control(format!("`{}` is too long for a flow (at most 256 KiB)", file.display())));
    }
    String::from_utf8(bytes).map_err(|_| Error::Control(format!("`{}` is not text", file.display())))
}

pub(super) fn confirmation(risks: &str, to: ImportTo) -> String {
    let place = match to {
        ImportTo::Yours => "your flows",
        ImportTo::Project => "the project's flows",
    };
    format!("{risks}\nImport it into {place}? [y/N] ")
}

pub(super) fn imported_line(imported: &Imported) -> String {
    let place = if imported.origin == "project" { "this project" } else { "yours" };
    let path = imported.path.as_ref().map(|path| format!(": {}", path.display())).unwrap_or_default();
    let replaced = if imported.replaced { " (replaced)" } else { "" };
    format!("imported {} into {place}{replaced}{path}\n", share::visible(&imported.name))
}

pub(super) fn listing(flows: &Flows) -> String {
    let mut out = String::new();
    for flow in &flows.flows {
        let _ = writeln!(out, "{}", heading(flow));
        if !flow.description.is_empty() {
            let _ = writeln!(out, "  {}", flow.description);
        }
        let roles: Vec<String> = flow
            .roles
            .iter()
            .map(|role| {
                let model = role.model.as_deref().map(|m| format!(" · {m}")).unwrap_or_default();
                let effort = role.effort.as_deref().map(|e| format!(" · {e}")).unwrap_or_default();
                let edits = match (role.edits, role.worktree) {
                    (true, Worktree::Own) => " (edits, own worktree)",
                    (true, Worktree::Shared) => " (edits)",
                    (false, Worktree::Own) => " (own worktree)",
                    (false, Worktree::Shared) => "",
                };
                format!("{}: {}{model}{effort}{edits}", role.role, role.agent)
            })
            .collect();
        let _ = writeln!(out, "  {}", roles.join(", "));
        for problem in &flow.problems {
            let _ = writeln!(out, "  cannot run: {problem}");
        }
        if let Some(check) = &flow.check {
            let _ = writeln!(out, "  {check}");
        }
    }
    for broken in &flows.broken {
        let _ = writeln!(out, "cannot read {}: {}", broken.path.display(), broken.error);
    }
    if !flows.hidden.is_empty() {
        let names: Vec<String> = flows.hidden.iter().map(|flow| share::visible(&flow.name)).collect();
        let _ = writeln!(
            out,
            "deleted built-in flows: {} (`cornercase flow restore NAME` brings one back)",
            names.join(", ")
        );
    }
    out
}

fn heading(flow: &FlowInfo) -> String {
    let origin = match flow.origin.as_str() {
        "built-in" => "built in",
        "user" => "yours",
        _ => "this project",
    };
    match &flow.path {
        Some(path) => format!("{} ({origin}, {})", flow.name, path.display()),
        None => format!("{} ({origin})", flow.name),
    }
}

pub(super) fn folder_note(run: &RunInfo) -> Option<String> {
    run.run_dir.as_ref().map(|dir| format!("the run keeps its files in {}", dir.display()))
}

pub(super) fn started(run: &RunInfo) -> String {
    let mut out = format!("{}\n", run.workspace);
    for member in &run.members {
        let _ = writeln!(out, "{}\t{}", member.pane, member.role);
    }
    out
}

#[cfg(test)]
mod tests;
