use std::fmt::Write as _;
use std::io::{IsTerminal as _, Read as _};
use std::path::Path;

use super::args::{
    FlowAction, FlowDeleteArgs, FlowFilesArgs, FlowImportArgs, FlowMessagesArgs, FlowRunsArgs, ImportTo,
};
use super::{print_json, print_out, quietly, read_prompt, stream_json, workspace};
use crate::client::{self, answer, ask, confirm};
use crate::control::{
    self, Exported, FlowBacklog, FlowInfo, FlowMessage, FlowRemoved, FlowRuns, Flows, Imported, RunEdge, RunFiles,
    RunInfo, RunState,
};
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
        FlowAction::Runs(runs) => follow_runs(&runs),
        FlowAction::Messages(messages) => messages_of(&messages),
        FlowAction::Files(files) => files_of(&files),
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

fn follow_runs(args: &FlowRunsArgs) -> Result<()> {
    let command = control::Command::FlowRuns(control::FlowRunsWatch {});
    if args.follow {
        return stream_json("flow-runs", command);
    }
    let value = ask("flow runs", command)?;
    if args.print.json {
        return print_json(&value);
    }
    print_out(&runs_text(&answer(value)?))
}

pub(super) fn runs_text(runs: &FlowRuns) -> String {
    if runs.runs.is_empty() {
        return "no flow is running\n".into();
    }
    runs.runs.iter().map(run_block).collect::<Vec<_>>().join("\n")
}

fn run_block(run: &RunState) -> String {
    let mut out = format!("workspace {}  {}", run.workspace, share::visible(&run.flow.name));
    match run.stage {
        Some(stage) => {
            let _ = write!(out, "  stage {} of {}", stage + 1, run.flow.stages.len());
        }
        None => out.push_str("  not started"),
    }
    for (stage, rounds) in run.rounds.iter().enumerate().filter(|(_, rounds)| **rounds > 0) {
        if let Some(back) = run.flow.stages.get(stage).and_then(|stage| stage.back.as_ref()) {
            let _ = write!(out, "  {} {rounds}/{}", share::visible(&back.when), back.at_most);
        }
    }
    out.push('\n');
    let task = run.task.lines().next().unwrap_or_default();
    let _ = writeln!(out, "  task: {}", share::visible(&crate::text::truncate_right(task, 100)));
    let width = run.roles.iter().map(|role| role.role.chars().count()).max().unwrap_or_default();
    for role in &run.roles {
        let pane = role.pane.map_or_else(|| "-".to_string(), |pane| format!("pane {pane}"));
        let status = match (&role.status, role.closed) {
            (_, true) => "closed",
            (Some(status), false) => status.as_str(),
            (None, false) => "-",
        };
        let _ = writeln!(out, "  {:<width$}  {pane:<9}  {status}", share::visible(&role.role));
    }
    if let Some(last) = &run.last {
        let from = last.from.as_deref().map(|from| format!("{} ", share::visible(from))).unwrap_or_default();
        let _ = writeln!(out, "  last: {from}→ {}", share::visible(&last.to));
    }
    if let Some(RunEdge::Outside { from, to }) = run.outside {
        let from = from.map_or_else(|| "the start".to_string(), |from| format!("stage {}", from + 1));
        let _ = writeln!(out, "  outside the plan: {from} → stage {}", to + 1);
    }
    if run.ended {
        out.push_str("  ended: the orchestrator marked its pane for review\n");
    }
    out
}

fn messages_of(args: &FlowMessagesArgs) -> Result<()> {
    let workspace = workspace("flow messages", args.place.workspace, args.place.worktree.as_deref())?;
    let command = control::Command::FlowMessages(control::FlowMessages { workspace });
    let json = args.print.json;
    if !args.follow {
        let backlog: FlowBacklog = answer(ask("flow messages", command)?)?;
        let text: String = backlog.messages.iter().map(|message| message_text(message, json)).collect();
        return print_out(&text);
    }
    client::follow_all("flow messages", command, |value| {
        let text = frame_text(value, json)?;
        let mut out = std::io::stdout().lock();
        quietly(std::io::Write::write_all(&mut out, text.as_bytes()).and_then(|()| std::io::Write::flush(&mut out)))
    })
}

pub(super) fn frame_text(value: serde_json::Value, json: bool) -> Result<String> {
    if value.get("event").and_then(serde_json::Value::as_str) == Some("dropped") {
        let count = value.get("count").and_then(serde_json::Value::as_u64).unwrap_or_default();
        return Ok(if json {
            format!("{value}\n")
        } else {
            format!(
                "{count} message{} dropped: this command fell behind; run it again to read what is kept\n",
                if count == 1 { "" } else { "s" }
            )
        });
    }
    Ok(match value.get("messages") {
        Some(_) => answer::<FlowBacklog>(value)?.messages.iter().map(|message| message_text(message, json)).collect(),
        None => message_text(&answer::<FlowMessage>(value)?, json),
    })
}

fn message_text(message: &FlowMessage, json: bool) -> String {
    if json {
        return format!("{}\n", serde_json::to_string(message).unwrap_or_default());
    }
    let what = match message.kind.as_str() {
        "send" => "sent",
        "keys" => "keys pressed",
        "read" => "read",
        "wait" => "waited for",
        other => other,
    };
    let bytes = message.bytes;
    let mut out = format!(
        "{}  {} → {}  {what}  {bytes} byte{}\n",
        message.time,
        share::visible(&message.from),
        share::visible(&message.to),
        if bytes == 1 { "" } else { "s" }
    );
    for line in message.text.iter().flat_map(|text| text.lines()) {
        let line: String = line.chars().map(|c| if c.is_control() && c != '\t' { ' ' } else { c }).collect();
        let _ = writeln!(out, "  {line}");
    }
    out
}

fn files_of(args: &FlowFilesArgs) -> Result<()> {
    let workspace = workspace("flow files", args.place.workspace, args.place.worktree.as_deref())?;
    let value = ask("flow files", control::Command::FlowFiles(control::FlowFiles { workspace }))?;
    if args.print.json {
        return print_json(&value);
    }
    let files: RunFiles = answer(value)?;
    if files.more {
        eprintln!("only the newest {} files are listed", files.files.len());
    }
    print_out(&files_text(&files))
}

pub(super) fn files_text(files: &RunFiles) -> String {
    let mut out = String::from("name\tbytes\tmodified\tpath\n");
    for file in &files.files {
        let name = file.name.replace(|c: char| c.is_control(), " ");
        let path = file.path.display().to_string().replace(|c: char| c.is_control(), " ");
        let modified = file.modified.as_deref().unwrap_or("-");
        let _ = writeln!(out, "{name}\t{}\t{modified}\t{path}", file.bytes);
    }
    out
}

#[cfg(test)]
mod tests;
