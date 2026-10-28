use super::*;
use crate::process;
use crate::project::Hierarchy;
use crate::test_util::{Running, this_pid, wait_until};

fn first_pane(app: &App) -> (u64, i32) {
    let term = app.projects.terms().next().expect("a pane");
    (term.id, term.shell_pid().expect("a shell"))
}

fn sleeping(app: &mut App) -> (u64, i32, i32) {
    let (pane, shell) = first_pane(app);
    type_line(app, "sleep 30");
    wait_until("the sleep starts", || !process::children(shell).is_empty());
    (pane, shell, process::children(shell)[0])
}

fn pids(list: &Value) -> Vec<i64> {
    list["processes"].as_array().expect("processes").iter().filter_map(|proc| proc["pid"].as_i64()).collect()
}

fn orphan_sleeper() -> i32 {
    let out = std::process::Command::new("/bin/sh")
        .args(["-c", "/bin/sleep 30 >/dev/null 2>&1 & echo $!"])
        .output()
        .expect("run sh");
    let pid: i32 = String::from_utf8_lossy(&out.stdout).trim().parse().expect("a pid");
    wait_until("the orphan is reparented", || process::parent(pid).is_some_and(|parent| parent != this_pid()));
    pid
}

#[test]
fn every_pane_is_listed_with_its_shell_and_its_children() {
    let (mut app, rx) = app();
    let (pane, shell, sleeper) = sleeping(&mut app);

    let list = asked(&mut app, &rx, "processes", &json!({"interval": 100})).expect("an answer");

    assert_eq!(list["interval"], 100);
    let panes = list["panes"].as_array().expect("panes");
    assert_eq!(panes.len(), 1, "{list}");
    assert_eq!((panes[0]["pane"].as_u64(), panes[0]["pid"].as_i64()), (Some(pane), Some(i64::from(shell))));
    let procs = panes[0]["processes"].as_array().expect("processes");
    let by = |pid: i32| procs.iter().find(|proc| proc["pid"] == pid).expect("a process");
    assert_eq!(by(shell)["parent"], Value::Null);
    assert_eq!(by(sleeper)["parent"], shell);
    assert_eq!(by(sleeper)["name"], "sleep");
    assert!(by(sleeper)["memory"].as_u64().is_some_and(|bytes| bytes > 0), "{list}");
    assert_eq!(list["total"]["panes"], 1);
}

#[test]
fn cornercase_itself_comes_last_with_the_server_first() {
    let (mut app, rx) = app();

    let list = asked(&mut app, &rx, "processes", &json!({"interval": 100})).expect("an answer");

    assert_eq!(list["cornercase"]["pid"], this_pid());
    assert_eq!(pids(&list["cornercase"]).first(), Some(&i64::from(this_pid())));
}

#[test]
fn a_pane_or_a_workspace_narrows_the_list_and_leaves_cornercase_out() {
    let (mut app, rx) = app();
    let (pane, _) = first_pane(&app);
    let workspace = app.projects[0].workspaces[0].id;

    let one = asked(&mut app, &rx, "processes", &json!({"pane": pane, "interval": 100})).expect("an answer");
    let all = asked(&mut app, &rx, "processes", &json!({"workspace": workspace, "interval": 100})).expect("an answer");

    for list in [one, all] {
        assert_eq!(list["panes"].as_array().map(Vec::len), Some(1), "{list}");
        assert_eq!(list["cornercase"], Value::Null);
    }
}

#[test]
fn an_unknown_pane_or_workspace_is_refused() {
    let (mut app, _rx) = app();
    assert_eq!(refused(&mut app, "processes", &json!({"pane": 999})).code, "not-found");
    assert_eq!(refused(&mut app, "processes", &json!({"workspace": 999})).code, "not-found");
}

#[test]
fn a_child_of_a_pane_is_stopped_and_the_shell_stays() {
    let (mut app, rx) = app();
    let (pane, shell, sleeper) = sleeping(&mut app);

    let stopped = asked(&mut app, &rx, "stop-process", &json!({"pid": sleeper})).expect("stopped");

    assert_eq!((stopped["outcome"].as_str(), stopped["pane"].as_u64()), (Some("stopped"), Some(pane)));
    assert_eq!((stopped["pid"].as_i64(), stopped["program"].as_str()), (Some(i64::from(sleeper)), Some("sleep")));
    assert!(!process::running(sleeper, None));
    assert!(process::running(shell, None));
    let again = asked(&mut app, &rx, "stop-process", &json!({"pid": sleeper})).expect_err("refused");
    assert_eq!(again.code, "not-found");
}

#[test]
fn the_shell_of_a_pane_cornercase_and_a_stranger_are_refused_and_keep_running() {
    let (mut app, rx) = app();
    let (pane, shell, sleeper) = sleeping(&mut app);
    let orphan = orphan_sleeper();
    let server_child = Running::spawn(std::process::Command::new("/bin/sleep").arg("30"));

    let mut refuse = |pid: i32| asked(&mut app, &rx, "stop-process", &json!({"pid": pid})).expect_err("refused");
    let of_shell = refuse(shell);
    let of_server = refuse(this_pid());
    let of_orphan = refuse(orphan);
    let of_server_child = refuse(server_child.pid());

    assert_eq!(of_shell.code, "conflict");
    assert!(
        of_shell.message.contains(&format!("close the pane instead (cornercase close --pane {pane})")),
        "{}",
        of_shell.message
    );
    assert!(of_server.message.contains("cornercase itself"), "{}", of_server.message);
    assert!(of_orphan.message.contains("not a program of a cornercase pane"), "{}", of_orphan.message);
    assert!(of_server_child.message.contains("cornercase itself"), "{}", of_server_child.message);
    assert!([shell, sleeper, orphan, server_child.pid()].iter().all(|pid| process::running(*pid, None)));
    let _ = std::process::Command::new("/bin/kill").arg(orphan.to_string()).status();
}

#[test]
fn a_started_value_that_changed_is_refused() {
    let (mut app, rx) = app();
    let (_, _, sleeper) = sleeping(&mut app);
    let started = process::start_stamp(sleeper).map(|stamp| stamp + 1);

    let refused =
        asked(&mut app, &rx, "stop-process", &json!({"pid": sleeper, "started": started})).expect_err("refused");

    assert_eq!(refused.code, "conflict");
    assert!(refused.message.contains("no longer the program that was listed"), "{}", refused.message);
    assert!(process::running(sleeper, None));
}
