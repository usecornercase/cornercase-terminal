use std::fmt::Write as _;

use super::args::processes::{ProcessesAction, ProcessesArgs, StopArgs};
use super::{print_json, print_out, workspace};
use crate::client::{answer, ask};
use crate::control::processes::{PaneProcesses, ProcessInfo, ProcessList};
use crate::control::{Command, ProcessStopped, Processes, StopProcess};
use crate::error::Result;
use crate::process::command_line;
use crate::text::memory_size;

const COMMAND_WIDTH: usize = 40;

pub(super) fn run(args: &ProcessesArgs) -> Result<bool> {
    if let Some(ProcessesAction::Stop(stop)) = &args.action {
        return run_stop(stop);
    }
    let workspace = workspace("processes", args.workspace, args.worktree.as_deref())?;
    let request = Processes { pane: args.pane, workspace, interval: args.interval };
    let value = ask("processes", Command::Processes(request))?;
    let list: ProcessList = answer(value.clone())?;
    if args.print.json {
        print_json(&value)?;
    } else {
        print_out(&text(&list))?;
    }
    Ok(true)
}

fn run_stop(args: &StopArgs) -> Result<bool> {
    let value = ask("stop-process", Command::StopProcess(StopProcess { pid: args.pid, started: args.started }))?;
    let stopped: ProcessStopped = answer(value.clone())?;
    if args.print.json {
        print_json(&value)?;
    } else {
        print_out(&stopped_text(&stopped))?;
    }
    Ok(stopped.outcome != "still-running")
}

pub(crate) fn stopped_text(stopped: &ProcessStopped) -> String {
    let program = stopped.program.as_deref().unwrap_or("the program");
    let also = match stopped.processes.saturating_sub(1) {
        0 => String::new(),
        n => format!(" and {} it started", crate::text::plural(n, "program", "programs")),
    };
    let what = format!("{program} (pid {}){also}", stopped.pid);
    match stopped.outcome.as_str() {
        "stopped" => format!("stopped {what}\n"),
        "killed" => format!("killed {what}: they did not stop within 3 s of SIGTERM\n"),
        _ => format!("{what} still running after SIGKILL\n"),
    }
}

pub(crate) fn text(list: &ProcessList) -> String {
    let mut panes: Vec<&PaneProcesses> = list.panes.iter().collect();
    panes.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
    let mut text = String::new();
    for pane in panes {
        pane_text(&mut text, pane);
    }
    if let Some(own) = &list.cornercase {
        let _ = writeln!(text, "cornercase  {:.1}%  {}", own.cpu, memory_size(own.memory));
    }
    let total = &list.total;
    let _ = writeln!(
        text,
        "total: {}, {}, cpu {:.1}%, {}",
        crate::text::plural(total.panes, "pane", "panes"),
        crate::text::plural(total.processes, "process", "processes"),
        total.cpu,
        memory_size(total.memory)
    );
    text
}

fn pane_text(text: &mut String, pane: &PaneProcesses) {
    let program = pane.program.as_deref().unwrap_or("-");
    let _ = writeln!(text, "pane {}  {program}  {:.1}%  {}", pane.pane, pane.cpu, memory_size(pane.memory));
    for (at, proc) in pane.processes.iter().enumerate() {
        let depth = depth_of(&pane.processes, at);
        let branch = if depth == 0 { String::new() } else { format!("{}└ ", "  ".repeat(depth - 1)) };
        let command = command_line(Some(&proc.name), &proc.args, COMMAND_WIDTH);
        let cpu = proc.cpu.map_or_else(|| "-".to_string(), |cpu| format!("{cpu:.1}%"));
        let memory = proc.memory.map_or_else(|| "-".to_string(), memory_size);
        let _ = writeln!(text, "  {:<7} {branch}{command:<COMMAND_WIDTH$}  {cpu:>7}  {memory:>8}", proc.pid);
    }
}

fn depth_of(procs: &[ProcessInfo], at: usize) -> usize {
    let mut depth = 0;
    let mut parent = procs[at].parent;
    while let Some(pid) = parent {
        let Some(up) = procs.iter().find(|proc| proc.pid == pid) else { break };
        depth += 1;
        parent = up.parent;
    }
    depth
}

#[cfg(test)]
mod tests;
