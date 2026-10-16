use std::fmt::Write as _;
use std::io::{IsTerminal as _, Read as _};
use std::path::Path;

use super::args::{FlowAction, FlowImportArgs, ImportTo};
use super::{print_json, print_out, read_prompt};
use crate::client::{answer, ask, confirm};
use crate::control::{self, Exported, FlowInfo, Flows, Imported, RunInfo};
use crate::error::{Error, Result};
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
            let request = control::FlowRun {
                name: run.name,
                prompt: read_prompt(run.prompt, run.prompt_file)?.unwrap_or_default(),
                worktree: run.worktree,
                project: run.project,
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
    }
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
                let edits = if role.edits { " (edits)" } else { "" };
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

pub(super) fn started(run: &RunInfo) -> String {
    let mut out = format!("{}\n", run.workspace);
    for member in &run.members {
        let _ = writeln!(out, "{}\t{}", member.pane, member.role);
    }
    out
}
