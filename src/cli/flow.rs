use std::fmt::Write as _;

use super::args::FlowAction;
use super::{print_json, print_out, read_prompt};
use crate::client::{answer, ask};
use crate::control::{self, FlowInfo, Flows, RunInfo};
use crate::error::Result;

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
            print_out(&started(&answer(value)?))
        }
    }
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
                let edits = if role.edits { " (edits)" } else { "" };
                format!("{}: {}{model}{edits}", role.role, role.agent)
            })
            .collect();
        let _ = writeln!(out, "  {}", roles.join(", "));
        for problem in &flow.problems {
            let _ = writeln!(out, "  cannot run: {problem}");
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
