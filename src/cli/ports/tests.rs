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
        args: vec!["node".into(), "vite".into()],
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
