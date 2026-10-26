use std::fmt::Write as _;
use std::io::IsTerminal as _;
use std::path::{Component, Path, PathBuf};

use super::args::{
    self, FlowAction, FlowDefaultArgs, FlowDeleteArgs, FlowFilesArgs, FlowImportArgs, FlowMessagesArgs, FlowOrderArgs,
    FlowRunsArgs, FlowSaveArgs, ImportTo,
};
use super::{print_json, print_out, quietly, read_limited, read_prompt, stream_json, workspace};
use crate::client::{self, answer, ask, confirm};
use crate::control::{
    self, Exported, FlowBacklog, FlowDefaultSet, FlowInfo, FlowMessage, FlowOrdered, FlowRemoved, FlowRuns, FlowSaved,
    FlowSpec, Flows, Imported, RoleInfo, RunEdge, RunFiles, RunInfo, RunState, contract,
};
use crate::error::{Error, Result};
use crate::flows::registry::{self, Source};
use crate::flows::share;
use crate::flows::{Flow, Worktree};

pub(super) fn run(action: FlowAction) -> Result<()> {
    match action {
        FlowAction::List(list) => flow_list(&list),
        FlowAction::Run(run) => flow_run(run),
        FlowAction::Export(export) => flow_export(export),
        FlowAction::Import(import) => import_flow(import),
        FlowAction::Save(save) => save_flow(save),
        FlowAction::Runs(runs) => follow_runs(&runs),
        FlowAction::Messages(messages) => messages_of(&messages),
        FlowAction::Files(files) => files_of(&files),
        FlowAction::Delete(delete) => delete_flow(delete),
        FlowAction::Hide(hide) => {
            let request = control::FlowHide { name: hide.name };
            answered("flow hide", control::Command::FlowHide(request), hide.print.json, hidden_line)
        }
        FlowAction::Restore(restore) => {
            let request = control::FlowRestore { name: restore.name };
            answered("flow restore", control::Command::FlowRestore(request), restore.print.json, restored_line)
        }
        FlowAction::Default(default) => set_default(default),
        FlowAction::Order(order) => set_order(order),
    }
}

fn flow_list(list: &args::FlowListArgs) -> Result<()> {
    let value = ask("flow list", control::Command::FlowList(control::FlowList { project: list.project }))?;
    if list.print.json {
        return print_json(&value);
    }
    print_out(&listing(&answer(value)?))
}

fn flow_run(run: args::FlowRunArgs) -> Result<()> {
    super::issue_features(&run.issue)?;
    let (profile, accounts) = super::accounts::profile_arg(run.profile)?;
    let request = control::FlowRun {
        name: run.name,
        prompt: read_prompt(run.prompt, run.prompt_file)?.unwrap_or_default(),
        worktree: run.worktree,
        project: run.project,
        issue: run.issue.key,
        source: run.issue.source,
        workspace_name: run.issue.workspace_name,
        focus: run.create.focus,
        profile,
        accounts,
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

fn flow_export(export: args::FlowExportArgs) -> Result<()> {
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

const MAX_DRAFT: usize = 4 * share::MAX_BYTES;

fn save_flow(args: FlowSaveArgs) -> Result<()> {
    let flow = draft(&args.source)?;
    let path = args.path.map(|path| listed_path(&path)).transpose()?;
    let to = match (args.to, &path) {
        (None, Some(path)) => place_of(path, args.project)?,
        (to, _) => to,
    };
    let place = to.unwrap_or(ImportTo::Yours).name().to_string();
    let request = control::FlowSave { flow, place, project: args.project, path, check: args.check };
    answered("flow save", control::Command::FlowSave(request), args.print.json, saved_text)
}

fn draft(source: &Path) -> Result<FlowSpec> {
    let text = read_limited(source, MAX_DRAFT, "is too long for a flow (at most 1 MiB of JSON)")?;
    parsed(&text).map_err(|e| Error::Control(share::visible(&format!("cannot read `{}`: {e}", source.display()))))
}

fn parsed(text: &str) -> std::result::Result<FlowSpec, String> {
    let raw: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if !raw.is_object() {
        return Err("a flow is a JSON object, the `flow` of one entry of `cornercase flow list --json`".into());
    }
    let flow: FlowSpec = serde_json::from_str(text).map_err(|e| e.to_string())?;
    match contract::unknown_fields(&raw, &flow) {
        Some(key) => Err(format!(
            "`{key}` is not a key of a flow: give the `flow` of one entry of `cornercase flow list --json`, such as `jq '.flows[0].flow'`"
        )),
        None => Ok(flow),
    }
}

fn place_of(path: &Path, project: Option<u64>) -> Result<Option<ImportTo>> {
    let flows: Flows = answer(ask("flow save", control::Command::FlowList(control::FlowList { project }))?)?;
    Ok(listed_place(&flows, path))
}

fn listed_place(flows: &Flows, path: &Path) -> Option<ImportTo> {
    let listed = flows.flows.iter().find(|flow| flow.path.as_deref() == Some(path))?;
    Some(if listed.origin == "project" { ImportTo::Project } else { ImportTo::Yours })
}

fn listed_path(path: &Path) -> Result<PathBuf> {
    let mut clean = PathBuf::new();
    for part in std::path::absolute(path)?.components() {
        match part {
            Component::ParentDir => {
                clean.pop();
            }
            Component::CurDir => {}
            other => clean.push(other),
        }
    }
    Ok(clean)
}

fn into_place(origin: &str) -> &'static str {
    if origin == "project" { "this project" } else { "yours" }
}

fn saved_text(saved: &FlowSaved) -> String {
    let name = share::visible(&saved.name);
    let place = into_place(&saved.origin);
    let mut out = match &saved.path {
        Some(path) if saved.saved => format!("saved {name} into {place}: {}\n", path.display()),
        _ if saved.problems.is_empty() => format!("{name} can be saved into {place}; nothing was written\n"),
        _ => format!("{name} has problems; nothing was written\n"),
    };
    for problem in &saved.problems {
        let _ = writeln!(out, "  cannot run: {problem}");
    }
    if let Some(check) = &saved.check {
        let _ = writeln!(out, "  {check}");
    }
    out
}

fn delete_flow(args: FlowDeleteArgs) -> Result<()> {
    let request = control::FlowDelete {
        name: args.name.unwrap_or_default(),
        place: args.place.map(|place| place.name().to_string()).unwrap_or_default(),
        project: args.project,
        path: args.path.map(|path| listed_path(&path)).transpose()?,
    };
    answered("flow delete", control::Command::FlowDelete(request), args.print.json, deleted_line)
}

fn answered<T: serde::de::DeserializeOwned>(
    name: &'static str,
    command: control::Command,
    json: bool,
    line: impl Fn(&T) -> String,
) -> Result<()> {
    let value = ask(name, command)?;
    if json {
        return print_json(&value);
    }
    print_out(&line(&answer(value)?))
}

fn set_default(args: FlowDefaultArgs) -> Result<()> {
    let name = args.name.filter(|_| !args.clear);
    let request = control::FlowDefault { name, project: args.project };
    answered("flow default", control::Command::FlowDefault(request), args.print.json, default_line)
}

fn set_order(args: FlowOrderArgs) -> Result<()> {
    let names = if args.clear { Vec::new() } else { args.names };
    let request = control::FlowOrder { names, project: args.project };
    answered("flow order", control::Command::FlowOrder(request), args.print.json, ordered_line)
}

pub(super) fn default_line(set: &FlowDefaultSet) -> String {
    match (&set.name, set.changed) {
        (Some(name), true) => format!("the start dialog opens on {} now\n", share::visible(name)),
        (Some(name), false) => format!("{} was already the default flow\n", share::visible(name)),
        (None, true) => "no default flow: the start dialog opens on the first built-in flow that can start\n".into(),
        (None, false) => "there was no default flow\n".into(),
    }
}

pub(super) fn ordered_line(ordered: &FlowOrdered) -> String {
    let names: Vec<String> = ordered.names.iter().map(|name| share::visible(name)).collect();
    let what = if ordered.changed { "are in this order now" } else { "were already in this order" };
    format!("the flows {what}: {}\n", names.join(", "))
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
    let text = import_text(&source)?;
    let flow = share::from_share(&text)
        .map_err(|e| Error::Control(share::visible(&format!("cannot read `{}`: {e}", args.source))))?;
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
        check: args.check,
    };
    if args.check {
        return checked_import(request, &flow, &source, args.print.json);
    }
    eprint!("{}", share::summary(&flow));
    if let Some(risks) = asked.filter(|_| !args.yes)
        && !confirmed_import(&mut request, &source, &risks, args.to)?
    {
        println!("not imported");
        return Ok(());
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

fn checked_import(request: control::FlowImport, flow: &Flow, source: &Source, json: bool) -> Result<()> {
    let value = ask("flow import", control::Command::FlowImport(request))?;
    let value = complete_preview(value, flow, source)?;
    if json {
        return print_json(&value);
    }
    print_out(&checked_line(&answer(value)?))
}

fn complete_preview(mut value: serde_json::Value, flow: &Flow, source: &Source) -> Result<serde_json::Value> {
    let preview: Imported = answer(value.clone())?;
    if preview.roles.is_empty() {
        value["roles"] = serde_json::json!(RoleInfo::of_flow(flow));
    }
    let risks = preview.check.or_else(|| share::risks(flow, "it"));
    let risks = match source {
        Source::Url(_) | Source::Repo(_) => Some(fetched(source, risks)),
        _ => risks,
    };
    if let Some(risks) = risks {
        value["check"] = risks.into();
    }
    Ok(value)
}

fn checked_line(preview: &Imported) -> String {
    let mut out = format!("{}\n", share::visible(&preview.name));
    for role in &preview.roles {
        let _ = writeln!(out, "  {}", share::visible(&role_summary(role)));
    }
    if let Some(check) = &preview.check {
        let _ = writeln!(out, "{check}");
    }
    if let Some(note) = &preview.note {
        let _ = writeln!(out, "{note}");
    }
    let replaced = if preview.replaced { " (replaces the existing flow)" } else { "" };
    let _ = writeln!(out, "would import into {}{replaced}; nothing was imported", into_place(&preview.origin));
    out
}

fn import_text(source: &Source) -> Result<String> {
    match source {
        Source::Stdin => read_shared(Path::new("-")),
        Source::File(path) => read_shared(path),
        remote => {
            eprintln!("fetching {}", remote.label());
            registry::fetch(remote).map_err(|e| Error::Control(share::visible(&e)))
        }
    }
}

fn confirmed_import(request: &mut control::FlowImport, source: &Source, risks: &str, to: ImportTo) -> Result<bool> {
    request.check = true;
    ask("flow import", control::Command::FlowImport(request.clone()))?;
    request.check = false;
    let asking = *source != Source::Stdin && std::io::stdin().is_terminal();
    if !asking {
        return Err(Error::Control(format!("{risks}; pass --yes to import it")));
    }
    Ok(confirm(&confirmation(risks, to)))
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
    read_limited(file, share::MAX_BYTES, "is too long for a flow (at most 256 KiB)")
}

pub(super) fn confirmation(risks: &str, to: ImportTo) -> String {
    let place = match to {
        ImportTo::Yours => "your flows",
        ImportTo::Project => "the project's flows",
    };
    format!("{risks}\nImport it into {place}? [y/N] ")
}

pub(super) fn imported_line(imported: &Imported) -> String {
    let place = into_place(&imported.origin);
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
        let roles: Vec<String> = flow.roles.iter().map(role_summary).collect();
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

fn role_summary(role: &RoleInfo) -> String {
    let model = role.model.as_deref().map(|m| format!(" · {m}")).unwrap_or_default();
    let effort = role.effort.as_deref().map(|e| format!(" · {e}")).unwrap_or_default();
    let edits = match (role.edits, role.worktree) {
        (true, Worktree::Own) => " (edits, own worktree)",
        (true, Worktree::Shared) => " (edits)",
        (false, Worktree::Own) => " (own worktree)",
        (false, Worktree::Shared) => "",
    };
    format!("{}: {}{model}{effort}{edits}", role.role, role.agent)
}

fn heading(flow: &FlowInfo) -> String {
    let origin = match flow.origin.as_str() {
        "built-in" => "built in",
        "user" => "yours",
        _ => "this project",
    };
    let default = if flow.default { ", default" } else { "" };
    match &flow.path {
        Some(path) => format!("{} ({origin}{default}, {})", flow.name, path.display()),
        None => format!("{} ({origin}{default})", flow.name),
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
