use std::net::TcpListener;

use super::worktrees::{made, repo};
use super::*;

#[test]
fn a_plain_workspace_is_placed_by_its_panes_and_a_worktree_also_by_its_folder() {
    let (mut app, rx, repo, _worktrees, _config) = repo();
    let (worktree, path) = made(&mut app, &rx);

    let places = app.port_places();

    assert_eq!(places.worktrees, [(path, worktree)]);
    assert!(places.worktrees.iter().all(|(folder, _)| folder != repo.path()));
    let plain = app.projects[0].workspaces[0].id;
    let workspaces: Vec<u64> = places.panes.iter().map(|pane| pane.workspace).collect();
    assert!(workspaces.contains(&plain) && workspaces.contains(&worktree), "{workspaces:?}");
    assert_eq!(places.server, i32::try_from(std::process::id()).expect("a pid"));
}

#[test]
fn every_port_is_answered_after_the_scan_without_the_servers_own() {
    let (mut app, rx) = app();
    let socket = TcpListener::bind("127.0.0.1:0").expect("listen");
    let port = socket.local_addr().expect("its address").port();

    let listing = asked(&mut app, &rx, "ports", &json!({"all": true})).expect("an answer");

    let ports = listing["ports"].as_array().expect("a list");
    let server = std::process::id();
    assert!(ports.iter().all(|p| p["pid"] != server || p["port"] != port), "{listing}");
}

#[test]
fn a_workspace_that_does_not_exist_is_refused() {
    let (mut app, _rx) = app();
    assert_eq!(refused(&mut app, "ports", &json!({"workspace": 999})).code, "not-found");
}

#[test]
fn stopping_a_port_nothing_listens_on_is_not_found() {
    let (mut app, rx) = app();
    let refused = asked(&mut app, &rx, "stop-port", &json!({"port": 1})).expect_err("refused");
    assert_eq!((refused.code.as_str(), refused.message.as_str()), ("not-found", "nothing listens on port 1"));
}

#[test]
fn a_program_of_no_workspace_is_not_stopped() {
    let (mut app, rx) = app();
    let mut child = std::process::Command::new("python3")
        .args(["-c", "import socket,time; s=socket.socket(); s.bind(('127.0.0.1',0)); s.listen(); print(s.getsockname()[1], flush=True); time.sleep(60)"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("run python3");
    let mut line = String::new();
    std::io::BufRead::read_line(&mut std::io::BufReader::new(child.stdout.take().expect("stdout")), &mut line)
        .expect("read the port");
    let port: u16 = line.trim().parse().expect("a port");

    let refused = asked(&mut app, &rx, "stop-port", &json!({"port": port})).expect_err("refused");

    let running = child.try_wait().expect("check python3").is_none();
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(refused.code, "conflict", "{}", refused.message);
    assert!(refused.message.contains("not held by a program of a workspace"), "{}", refused.message);
    assert!(running, "python3 was stopped by the request");
}
