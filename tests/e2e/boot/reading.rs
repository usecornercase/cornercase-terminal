use std::process::Stdio;

use super::*;

fn another_pane(aims: &Aims) -> String {
    let ids = panes(aims.session.report()).map(|pane| pane.id.to_string());
    ids.into_iter().find(|id| *id != aims.pane).expect("the split pane")
}

fn reading_forms<'a>(aims: &'a Aims, other: &'a str) -> Vec<Vec<&'a str>> {
    let Aims { project, workspace, pane, tab, .. } = aims;
    vec![
        vec!["read", "--pane", pane],
        vec!["read", "--tab", tab, "--last-message"],
        vec!["read", "--pane", pane, "--pane", other, "--last-message", "--json"],
        vec!["wait", "--pane", pane, "--until", "turn-over", "--timeout", "5"],
        vec!["wait", "--pane", pane, "--pane", other, "--any", "--timeout", "5"],
        vec!["link", "--pane", pane, "--cell", "0,0"],
        vec!["processes", "--pane", pane],
        vec!["ports", "--workspace", workspace],
        vec!["changes", "file", "a.txt", "--workspace", workspace],
        vec!["changes", "shared", "--workspace", workspace],
        vec!["changes", "shared", "--worktree", "main"],
        vec!["changes", "merge-check", "--workspace", workspace, "--json"],
        vec!["files", "list", "--workspace", workspace],
        vec!["files", "read", "a.txt", "--workspace", workspace],
        vec!["files", "search", "x", "--workspace", workspace],
        vec!["issue", "sources", "--project", project],
        vec!["worktrees", "size", "--workspace", workspace],
        vec!["worktrees", "list", "--project", project],
        vec!["flow", "list", "--project", project],
        vec!["flow", "export", "Quick", "--project", project],
        vec!["flow", "files", "--workspace", workspace],
        vec!["flow", "messages", "--workspace", workspace],
    ]
}

#[test]
fn every_command_that_reads_refuses_an_earlier_boot_and_prints_nothing() {
    let aims = a_machine_with_ids_to_aim_at();
    let other = another_pane(&aims);
    let forms = reading_forms(&aims, &other);

    refuses_every_form_and_changes_nothing(&aims.session, &forms);
}

fn spawned(session: &Session, args: &[&str]) -> std::process::Child {
    let mut command = remotely(session, args);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    command.spawn().expect("run cornercase")
}

#[test]
fn every_stream_refuses_an_earlier_boot_before_its_first_line() {
    let Aims { session, workspace, pane, tab, .. } = a_machine_with_ids_to_aim_at();
    let forms: [&[&str]; 11] = [
        &["events"],
        &["events", "--pane", &pane, "--json"],
        &["surface", "--tab", &tab, "--cols", "80", "--rows", "24"],
        &["surface", "--view", "99999"],
        &["view", "--tab", &tab],
        &["view"],
        &["changes", "list", "--workspace", &workspace],
        &["changes", "list", "--follow"],
        &["files", "list", "--follow", "--workspace", &workspace],
        &["files", "read", "a.txt", "--follow", "--workspace", &workspace],
        &["flow", "messages", "--workspace", &workspace, "--follow"],
    ];

    for form in forms {
        let child = spawned(&session, &at(form, EARLIER));
        let out = finish_within(child, Duration::from_secs(30));
        assert_stale(&out, EARLIER);
    }
}

fn written(session: &Session, name: &str, text: &str) -> String {
    let file = session.dir.join(name);
    std::fs::write(&file, text).expect("write the file");
    file.display().to_string()
}

fn the_quick_flow_as_mine(session: &Session) -> String {
    let list: Value = serde_json::from_str(&session.says(&["flow", "list", "--json"])).expect("the flows");
    let flows = list["flows"].as_array().expect("flows");
    let mut flow = flows.iter().find(|f| f["name"] == "Quick").expect("Quick")["flow"].clone();
    flow["name"] = json!("Mine");
    written(session, "mine.json", &flow.to_string())
}

#[test]
fn every_flow_command_go_and_clone_refuse_an_earlier_boot_and_change_nothing() {
    let Aims { session, project, workspace, .. } = a_machine_with_ids_to_aim_at();
    let (saved, exported) = (the_quick_flow_as_mine(&session), session.says(&["flow", "export", "Quick"]));
    let shared = written(&session, "quick.toml", &exported);
    let source = repository(&session.dir, "toclone").display().to_string();
    let folder = session.dir.join("clones").display().to_string();
    let flows = session.says(&["flow", "list", "--json"]);
    let forms: [&[&str]; 10] = [
        &["flow", "run", "NoSuchFlow", "--prompt", "x", "--project", &project],
        &["flow", "save", &saved, "--project", &project, "--to", "project"],
        &["flow", "import", &shared, "--check", "--project", &project, "--to", "project"],
        &["flow", "delete", "NoSuch", "--project", &project],
        &["flow", "default", "Quick", "--project", &project],
        &["flow", "order", "Quick", "--project", &project],
        &["flow", "close-when-merged", "--workspace", &workspace],
        &["go", "cc-1-1", "--view", "99999"],
        &["clone", &source, &folder, "--view", "99999"],
        &["clone", &source, &folder],
    ];

    refuses_every_form_and_changes_nothing(&session, &forms);

    assert_eq!(session.says(&["flow", "list", "--json"]), flows);
    assert!(!std::path::Path::new(&folder).exists(), "nothing was cloned");
}

#[test]
fn a_read_aimed_before_a_restart_never_prints_what_took_the_id() {
    let typed = a_machine_with_a_shell_to_type_into();
    let Typed { session, pane, .. } = &typed;
    let earlier = boot(session);
    restarted(session);
    remote_says(session, &["input", "--pane", pane, r#"{"text": "fresh-marker"}"#]);
    wait_until("the marker shows", || screen(session, pane).contains("fresh-marker"));

    let out = there(session, &at(&["read", "--pane", pane], &earlier));

    assert_stale(&out, &earlier);
    assert!(!String::from_utf8_lossy(&out.stdout).contains("fresh-marker"));
}

#[test]
fn the_boot_of_this_run_lets_the_commands_that_read_go_through() {
    let Aims { session, project, workspace, pane, tab, .. } = a_machine_with_ids_to_aim_at();
    let boot = boot(&session);
    let before = ssh_runs(&session);
    let run = |args: &[&str]| remote_says(&session, &at(args, &boot));

    run(&["read", "--pane", &pane]);
    run(&["wait", "--pane", &pane, "--quiet", "0.5", "--timeout", "10"]);
    run(&["processes", "--pane", &pane, "--json"]);
    run(&["changes", "list", "--workspace", &workspace, "--json"]);
    run(&["changes", "shared", "--workspace", &workspace, "--json"]);
    run(&["worktrees", "list", "--project", &project, "--json"]);
    run(&["flow", "list", "--project", &project, "--json"]);
    run(&["flow", "default", "Quick", "--project", &project]);

    assert_eq!(ssh_runs(&session), before + 8, "each command's check and request share one ssh connection");
    let list: Value = serde_json::from_str(&session.says(&["flow", "list", "--json"])).expect("the flows");
    let marked: Vec<&Value> =
        list["flows"].as_array().expect("flows").iter().filter(|f| f["default"] == true).collect();
    assert_eq!(marked.first().map(|f| &f["name"]), Some(&json!("Quick")), "{list}");
    let child = spawned(&session, &at(&["surface", "--tab", &tab, "--cols", "80", "--rows", "24"], &boot));
    let out = finish_within(child, Duration::from_secs(5));
    let first = String::from_utf8_lossy(&out.stdout).lines().next().map(str::to_string).expect("a first line");
    let first: Value = serde_json::from_str(&first).expect("a JSON line");
    assert_eq!(first["boot"], boot.as_str(), "{first}");
}
