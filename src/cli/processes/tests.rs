use clap::Parser;

use super::*;
use crate::cli::Cli;
use crate::cli::args::{Command, Control};
use crate::control::processes::{OwnProcesses, ProcessTotal};

fn proc(pid: i32, parent: Option<i32>, name: &str, cpu: f64, memory: u64) -> ProcessInfo {
    ProcessInfo {
        pid,
        parent,
        name: name.into(),
        args: vec![name.into()],
        started: Some(1),
        cpu: Some(cpu),
        memory: Some(memory),
    }
}

fn pane(id: u64, cpu: f64, processes: Vec<ProcessInfo>) -> PaneProcesses {
    PaneProcesses {
        pane: id,
        program: Some("npm".into()),
        cpu,
        memory: 812 << 20,
        pid: 4200,
        processes,
        ..PaneProcesses::default()
    }
}

#[test]
fn the_text_sorts_panes_by_cpu_and_indents_the_tree() {
    let busy = pane(
        12,
        98.0,
        vec![
            proc(4200, None, "zsh", 0.0, 4 << 20),
            proc(4210, Some(4200), "npm", 0.1, 60 << 20),
            proc(4211, Some(4210), "node", 97.9, 740 << 20),
        ],
    );
    let idle = pane(3, 1.0, vec![proc(10, None, "zsh", 1.0, 4 << 20)]);
    let list = ProcessList {
        interval: 1000,
        total: ProcessTotal { cpu: 99.8, memory: 900 << 20, panes: 2, processes: 4 },
        panes: vec![idle, busy],
        cornercase: Some(OwnProcesses { pid: 1, cpu: 0.8, memory: 85 << 20, processes: vec![] }),
    };
    let text = text(&list);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "pane 12  npm  98.0%  812 MB");
    assert!(lines[1].starts_with("  4200    zsh"), "{}", lines[1]);
    assert!(lines[3].contains("  └ node"), "{}", lines[3]);
    assert!(lines[2].contains("└ npm"), "{}", lines[2]);
    assert!(lines[3].ends_with("97.9%    740 MB"), "{}", lines[3]);
    assert_eq!(lines[4], "pane 3  npm  1.0%  812 MB");
    assert_eq!(lines[lines.len() - 2], "cornercase  0.8%  85 MB");
    assert_eq!(lines[lines.len() - 1], "total: 2 panes, 4 processes, cpu 99.8%, 900 MB");
}

#[test]
fn an_unreadable_figure_is_a_dash() {
    let unknown = ProcessInfo { cpu: None, memory: None, ..proc(5, None, "sudo", 0.0, 0) };
    let text = text(&ProcessList { panes: vec![pane(1, 0.0, vec![unknown])], ..ProcessList::default() });
    let line = text.lines().nth(1).expect("a process line");
    assert_eq!(line.split_whitespace().rev().take(2).collect::<Vec<_>>(), ["-", "-"], "{text}");
}

#[test]
fn stopping_says_what_happened_and_what_else_it_stopped() {
    let stopped = |outcome: &str, processes| ProcessStopped {
        pid: 41,
        program: Some("node".into()),
        pane: Some(3),
        outcome: outcome.into(),
        processes,
    };
    assert_eq!(stopped_text(&stopped("stopped", 3)), "stopped node (pid 41) and 2 programs it started\n");
    assert_eq!(stopped_text(&stopped("stopped", 1)), "stopped node (pid 41)\n");
    assert_eq!(
        stopped_text(&stopped("killed", 2)),
        "killed node (pid 41) and 1 program it started: they did not stop within 3 s of SIGTERM\n"
    );
    assert_eq!(stopped_text(&stopped("still-running", 1)), "node (pid 41) still running after SIGKILL\n");
}

#[test]
fn the_flags_parse_and_the_interval_has_a_range() {
    let cli =
        Cli::try_parse_from(["cornercase", "processes", "--pane", "12", "--interval", "500", "--json"]).expect("parse");
    match cli.command {
        Some(Command::Control(Control::Processes(args))) => {
            assert_eq!((args.pane, args.interval, args.print.json), (Some(12), Some(500), true));
        }
        other => panic!("not processes: {other:?}"),
    }
    assert!(Cli::try_parse_from(["cornercase", "processes", "--interval", "5"]).is_err());
    assert!(Cli::try_parse_from(["cornercase", "processes", "--pane", "1", "--workspace", "2"]).is_err());
}

#[test]
fn processes_stop_takes_a_pid_and_maybe_a_started_value() {
    let cli = Cli::try_parse_from(["cornercase", "processes", "stop", "41", "--started", "99"]).expect("parse");
    match cli.command {
        Some(Command::Control(Control::Processes(args))) => match args.action {
            Some(ProcessesAction::Stop(stop)) => assert_eq!((stop.pid, stop.started), (41, Some(99))),
            other => panic!("not stop: {other:?}"),
        },
        other => panic!("not processes: {other:?}"),
    }
}
