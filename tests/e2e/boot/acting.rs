use super::*;

struct Aims {
    session: Arc<Session>,
    project: String,
    group: String,
    workspace: String,
    pane: String,
    tab: String,
    item: String,
    folder: String,
}

fn a_machine_with_ids_to_aim_at() -> Aims {
    let (session, project) = machine_with(&["aimed"]);
    let workspace = id_of(&session, "aimed");
    let pane = panes(session.report()).next().expect("a pane").id.to_string();
    let status: Value = serde_json::from_str(&session.says(&["status", "--json"])).expect("a status report");
    let tab = status["projects"][0]["workspaces"][0]["tabs"][0]["id"].to_string();
    let group = remote_says(&session, &["group", "new", "work"]);
    remote_says(&session, &["move", "--project", &project, "--into", &group]);
    remote_says(&session, &["split", "--pane", &pane]);
    let item = remote_says(&session, &["todo", "add", "x"]);
    let folder = repository(&session.dir, "other").display().to_string();
    Aims { session, project, group, workspace, pane, tab, item, folder }
}

fn queued(session: &Session) -> bool {
    remote_says(session, &["queue", "list", "--json"]).contains("\"prompt\"")
}

fn terminal_forms(aims: &Aims) -> Vec<Vec<&str>> {
    let Aims { pane, tab, item, .. } = aims;
    vec![
        vec!["keys", "--pane", &pane, "q", "z", "q"],
        vec!["held", "--pane", &pane, "--send-now"],
        vec!["todo", "send", &item, "--pane", &pane],
        vec!["scroll", "--pane", &pane, "--up", "3"],
        vec!["selection", "--pane", &pane, "--clear"],
        vec!["split", "--pane", &pane],
        vec!["focus", "--pane", &pane],
        vec!["ratio", "--tab", &tab, "0.3"],
        vec!["resize", "--surface", "99999", "--cols", "80", "--rows", "24"],
        vec!["redraw", "--surface", "99999"],
        vec!["view-set", "--view", "99999", "--cols", "80", "--rows", "24"],
    ]
}

fn arranging_forms(aims: &Aims) -> Vec<Vec<&str>> {
    let Aims { project, group, workspace, pane, folder, .. } = aims;
    vec![
        vec!["move", "--workspace", &workspace, "--last"],
        vec!["group", "style", &group, "--icon", "★"],
        vec!["group", "delete", &group],
        vec!["new-tab", "--workspace", &workspace],
        vec!["new-tab", "--worktree", "main"],
        vec!["setup", "--workspace", &workspace],
        vec!["start", "claude", "--workspace", &workspace, "--prompt", "x"],
        vec!["new-workspace", "x", "--project", &project],
        vec!["open", &folder, "--into", &group],
        vec!["agents", "close", "--workspace", &workspace, "--check"],
        vec!["agents", "reopen", "--workspace", &workspace],
        vec!["agents", "forget", "--workspace", &workspace, "--yes"],
        vec!["changes", "viewed", "a.txt", "--workspace", &workspace],
        vec!["changes", "stage", "a.txt", "--workspace", &workspace],
        vec!["changes", "unstage", "a.txt", "--workspace", &workspace],
        vec!["changes", "discard", "a.txt", "--workspace", &workspace, "--yes"],
        vec!["changes", "base", "main", "--workspace", &workspace],
        vec!["queue", "add", "--workspace", &workspace, "--agent", "claude", "--prompt", "x", "--at", "in 2h"],
        vec!["account", "add", "claude", "work"],
        vec!["account", "login", "claude", "--system", "--workspace", &workspace],
        vec!["worktrees", "free-builds", "--workspace", &workspace, "--check"],
        vec!["plugin", "run", "p", "a", "--pane", &pane],
        vec!["plugin", "run", "p", "a", "--pane", &pane, "--revision", "1"],
    ]
}

#[test]
fn every_command_that_acts_refuses_an_earlier_boot_and_changes_nothing() {
    let aims = a_machine_with_ids_to_aim_at();
    let (session, group, pane, item) = (&aims.session, &aims.group, &aims.pane, &aims.item);
    let styled = remote_says(session, &["group", "style", group]);
    let accounts = remote_says(session, &["account", "list"]);
    let todos = remote_says(session, &["todo", "list", "--json"]);
    let forms = [terminal_forms(&aims), arranging_forms(&aims)].concat();

    refuses_every_form_and_changes_nothing(session, &forms);

    assert!(!screen(session, pane).contains("qzq"));
    assert_eq!(remote_says(session, &["todo", "list", "--json"]), todos, "{item}");
    assert!(!queued(session));
    assert_eq!(remote_says(session, &["group", "style", group]), styled);
    assert_eq!(remote_says(session, &["account", "list"]), accounts);
}

#[test]
fn the_boot_of_this_run_lets_the_commands_that_act_go_through() {
    let Aims { session, project, group, workspace, pane, tab, folder, .. } = a_machine_with_ids_to_aim_at();
    let boot = boot(&session);
    let before = ssh_runs(&session);
    let run = |args: &[&str]| remote_says(&session, &at(args, &boot));

    run(&["keys", "--pane", &pane, "q", "z", "q"]);
    let split = run(&["split", "--pane", &pane, "--json"]);
    run(&["ratio", "--tab", &tab, "0.3"]);
    run(&["selection", "--pane", &pane, "--from", "0,0", "--to", "1,0"]);
    run(&["focus", "--pane", &pane]);
    run(&["move", "--tab", &tab, "--last"]);
    let styled = run(&["group", "style", &group, "--icon", "★"]);
    run(&["new-tab", "--workspace", &workspace, "--json"]);
    run(&["new-workspace", "y", "--project", &project]);
    run(&["queue", "add", "--workspace", &workspace, "--agent", "claude", "--prompt", "x", "--at", "in 2h"]);
    run(&["changes", "base", "--workspace", &workspace]);
    run(&["open", &folder, "--into", &group]);

    assert_eq!(ssh_runs(&session), before + 12, "each command's check and request share one ssh connection");
    wait_until("the keys show", || screen(&session, &pane).contains("qzq"));
    assert!(split.contains("pane"), "{split}");
    assert!(styled.starts_with('★'), "{styled}");
    assert!(queued(&session));
}

#[test]
fn input_follow_aimed_before_a_restart_types_nothing_and_ends_with_4() {
    let session = a_machine_with_ids_to_aim_at().session;
    let earlier = boot(&session);
    restarted(&session);
    let pane = panes(session.report()).next().expect("a pane of this run").id.to_string();
    let command = at(&["input", "--pane", &pane, "--follow", "--json", "-"], &earlier);
    let mut child = remotely(&session, &command);
    child.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let mut child = child.spawn().expect("run cornercase");
    let mut stdin = child.stdin.take().expect("its input");

    let lines = "{\"text\":\"stale-follow\"}\n{\"key\":{\"code\":\"enter\"}}\n";
    let _ = stdin.write_all(lines.as_bytes());
    drop(stdin);
    let out = child.wait_with_output().expect("it ends");

    assert_stale(&out, &earlier);
    assert!(!screen(&session, &pane).contains("stale-follow"));
}

#[test]
fn input_follow_with_the_boot_of_this_run_types_every_line() {
    let Aims { session, pane, .. } = a_machine_with_ids_to_aim_at();
    let boot = boot(&session);
    let before = ssh_runs(&session);
    let command = at(&["input", "--pane", &pane, "--follow", "--json", "-"], &boot);
    let mut typist = Typist::start(remotely(&session, &command));

    let answer = typist.types(&json!({"text": "fresh-follow"}));
    let ended = typist.ends();

    assert_eq!(answer["pane"].to_string(), pane, "{answer}");
    assert_eq!(ended.status.code(), Some(0), "{ended:?}");
    wait_until("the line shows", || screen(&session, &pane).contains("fresh-follow"));
    assert_eq!(ssh_runs(&session), before + 1);
}
