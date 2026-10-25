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
    assert!(ports.iter().all(|p| p["port"] != port), "{listing}");
}

#[test]
fn a_workspace_that_does_not_exist_is_refused() {
    let (mut app, _rx) = app();
    assert_eq!(refused(&mut app, "ports", &json!({"workspace": 999})).code, "not-found");
}
