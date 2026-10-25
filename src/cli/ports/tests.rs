use clap::Parser;

use super::*;
use crate::cli::Cli;
use crate::cli::args::{Command, Control};

fn port(port: u16, via: Option<&str>) -> PortInfo {
    PortInfo {
        port,
        addresses: vec!["127.0.0.1".into()],
        url: format!("http://localhost:{port}"),
        pid: 41,
        program: Some("node".into()),
        args: vec!["/usr/local/bin/node".into(), "vite".into()],
        project: Some(1),
        workspace: Some(2),
        pane: Some(3),
        via: via.map(str::to_string),
        agent: None,
    }
}

#[test]
fn each_port_says_its_url_program_and_where_it_runs() {
    let agent = PortInfo { agent: Some("claude".into()), ..port(5173, Some("agent")) };
    let cwd = PortInfo { pane: None, ..port(3000, Some("cwd")) };
    let none = PortInfo { workspace: None, pane: None, project: None, ..port(8080, None) };
    let listing = Listing { ports: vec![port(4000, Some("pane")), agent, cwd, none] };
    assert_eq!(
        text(&listing),
        "http://localhost:4000  127.0.0.1  node vite  pid 41  workspace 2, pane 3
http://localhost:5173  127.0.0.1  node vite  pid 41  workspace 2, agent claude in pane 3
http://localhost:3000  127.0.0.1  node vite  pid 41  workspace 2, cwd
http://localhost:8080  127.0.0.1  node vite  pid 41  no workspace
"
    );
}

#[test]
fn nothing_listening_says_so() {
    assert_eq!(text(&Listing::default()), "nothing is listening on a port\n");
}

#[test]
fn all_and_a_workspace_cannot_go_together() {
    assert!(Cli::try_parse_from(["cornercase", "ports", "--all", "--workspace", "3"]).is_err());
    let cli = Cli::try_parse_from(["cornercase", "ports", "--worktree", "feat/x", "--json"]).expect("parse");
    match cli.command {
        Some(Command::Control(Control::Ports(args))) => {
            assert_eq!((args.worktree.as_deref(), args.all, args.print.json), (Some("feat/x"), false, true));
        }
        other => panic!("not ports: {other:?}"),
    }
}

#[test]
fn a_long_command_line_is_cut_and_a_path_shows_its_name() {
    let args: Vec<String> =
        ["/Applications/Discord.app/Contents/Frameworks/Discord Helper (Renderer)", "--type=renderer"]
            .iter()
            .map(ToString::to_string)
            .chain((0..40).map(|i| format!("--flag-{i}")))
            .collect();
    let port = PortInfo { program: None, args, ..port(6463, Some("pane")) };
    let line = text(&Listing { ports: vec![port] });
    assert!(line.contains("  Discord Helper (Renderer) --type=render…  pid 41"), "{line}");
}

#[test]
fn stopping_says_what_happened() {
    let stopped =
        |outcome: &str| PortStopped { port: 5173, pid: 41, program: Some("node".into()), outcome: outcome.into() };
    assert_eq!(stopped_text(&stopped("stopped")), "stopped node (pid 41) on port 5173\n");
    assert_eq!(
        stopped_text(&stopped("killed")),
        "killed node (pid 41) on port 5173: it did not stop within 3 s of SIGTERM\n"
    );
    assert_eq!(stopped_text(&stopped("still-running")), "node (pid 41) on port 5173 is still running after SIGKILL\n");
}

#[test]
fn ports_stop_takes_a_port_and_maybe_a_pid() {
    let cli = Cli::try_parse_from(["cornercase", "ports", "stop", "5173", "--pid", "41"]).expect("parse");
    match cli.command {
        Some(Command::Control(Control::Ports(args))) => match args.action {
            Some(PortsAction::Stop(stop)) => assert_eq!((stop.port, stop.pid), (5173, Some(41))),
            other => panic!("not stop: {other:?}"),
        },
        other => panic!("not ports: {other:?}"),
    }
    assert!(Cli::try_parse_from(["cornercase", "ports", "--all", "stop", "5173"]).is_err());
}
