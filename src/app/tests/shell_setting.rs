#![cfg(unix)]

use super::*;
use crate::config::{ProjectSettings, ShellChoice};
use crate::process;
use crate::project::Hierarchy;
use crate::test_util::{git_repo, write_executable};

fn choose(app: &mut App, program: &str, args: &[&str]) {
    app.config.shell = ShellChoice { program: program.into(), args: args.iter().map(ToString::to_string).collect() };
}

fn shell_args(app: &App, tab: usize) -> Vec<String> {
    tab_term(app, 0, tab).shell_pid().map(process::args).unwrap_or_default()
}

#[test]
fn the_next_tab_starts_the_shell_read_when_it_opens() {
    let (mut app, _rx, _dirs) = app_with(1);
    choose(&mut app, "/bin/sh", &["-i"]);

    click_row(&mut app, WorkspaceRow::NewTab(0));

    wait_until("the new tab's shell has its argument", || shell_args(&app, 1).iter().any(|arg| arg == "-i"));
    assert!(!shell_args(&app, 0).iter().any(|arg| arg == "-i"), "the open tab keeps its shell");
    wait_until("a shell with arguments is at its prompt", || tab_term(&app, 0, 1).shell_in_foreground());
}

#[test]
fn a_program_that_cannot_be_found_starts_the_default_and_says_so() {
    let (mut app, _rx, _dirs) = app_with(1);
    choose(&mut app, "/nonexistent/shell", &["-x"]);

    click_row(&mut app, WorkspaceRow::NewTab(0));

    assert_eq!(app.projects[0].workspaces[0].tabs.len(), 2);
    let said = toast(&app).expect("a toast");
    assert!(said.starts_with("cannot start the shell `/nonexistent/shell`: "), "{said}");
    wait_until("the default shell runs", || {
        tab_term(&app, 0, 1).shell_pid().and_then(process::name).is_some_and(|name| is_sh(&name))
    });
    assert!(!shell_args(&app, 1).iter().any(|arg| arg == "-x"), "the arguments went with the shell");
}

#[test]
fn the_setup_ends_in_the_chosen_shell_with_its_arguments() {
    let (repo, dir) = (git_repo(&[("README", "hi")]), TempDir::new());
    let (shell, seen) = (dir.path().join("shell"), dir.path().join("seen"));
    write_executable(&shell, &format!("#!/bin/sh\nprintf '%s|' \"$@\" > '{}'\nexec /bin/sh\n", seen.display()));
    let project = ProjectSettings { setup: "true".into(), ..ProjectSettings::default() };
    let settings = Config { projects: [(repo.path().display().to_string(), project)].into(), ..Config::default() };
    let (mut app, _rx) = new_app(write_config(dir.path(), &settings));
    app.open_project(repo.path().to_path_buf(), AREA).expect("open the project");
    choose(&mut app, &shell.display().to_string(), &["-l", "two words"]);
    let at = app.projects.workspace_at(0, 0).expect("the workspace");

    app.push_setup_tab(at, AREA, None).expect("the setup tab");

    wait_until("the shell got its arguments", || {
        std::fs::read_to_string(&seen).is_ok_and(|text| text == "-l|two words|")
    });
}
