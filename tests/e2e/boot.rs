use std::os::unix::net::{UnixListener, UnixStream};

use serde_json::{Value, json};

use super::input::Typist;
use super::machines::{VPS, in_state, reach, start_listing, wait_for_machine};
use super::remote::{boot_of, fake_ssh_to, remote_says, remotely, ssh_runs};
use super::*;

const EARLIER: &str = "1-1";
const ANSWER: u8 = 5;

fn there(session: &Session, args: &[&str]) -> std::process::Output {
    remotely(session, args).output().expect("run cornercase")
}

fn at<'a>(args: &[&'a str], boot: &'a str) -> Vec<&'a str> {
    [args, &["--boot", boot]].concat()
}

fn says_it_restarted(text: &str, boot: &str) -> bool {
    text.starts_with("this request is for the cornercase server started as ")
        && text.contains(boot)
        && text.contains("its ids may name other items now")
}

fn assert_stale(out: &std::process::Output, boot: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(4), "{out:?}");
    assert!(stderr.strip_prefix("error: ").is_some_and(|text| says_it_restarted(text, boot)), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
}

fn machine_with(workspaces: &[&str]) -> (Arc<Session>, String) {
    let session = Session::new();
    let config = json!({"worktrees_dir": session.dir.join("worktrees")});
    std::fs::write(session.dir.join("config.json"), config.to_string()).expect("write the config");
    remote_says(&session, &["start-server"]);
    let repo = repository(&session.dir, "api").display().to_string();
    let project = remote_says(&session, &["open", &repo]);
    for name in workspaces {
        remote_says(&session, &["new-workspace", name, "--project", &project]);
    }
    (session, project)
}

fn boot(session: &Session) -> String {
    boot_of(&remote_says(session, &["status", "--json"])).as_str().expect("a boot").to_string()
}

fn restarted(session: &Session) {
    remote_says(session, &["restart", "--yes"]);
    remote_says(session, &["start-server"]);
}

fn id_of(session: &Session, name: &str) -> String {
    let mut workspaces = session.report().projects.into_iter().flat_map(|project| project.workspaces);
    workspaces.find(|workspace| workspace.name == name).map_or_else(|| panic!("no {name}"), |w| w.id.to_string())
}

fn rows(session: &Session) -> Vec<String> {
    let status: Value = serde_json::from_str(&session.says(&["status", "--json"])).expect("a status report");
    rows_under(&status)
}

fn rows_under(row: &Value) -> Vec<String> {
    let mut rows = Vec::new();
    for key in ["projects", "workspaces", "tabs", "panes"] {
        for item in row[key].as_array().into_iter().flatten() {
            let (status, pid, layout) = (&item["status"], &item["pid"], &item["layout"]);
            rows.push(format!("{} {} {status} {pid} {layout}", item["id"], item["custom_name"]));
            rows.extend(rows_under(item));
        }
    }
    rows
}

fn lists(session: &Session, id: &str) -> bool {
    rows(session).iter().any(|row| row.starts_with(&format!("{id} ")))
}

struct Aimed {
    session: Arc<Session>,
    boot: String,
    id: String,
}

fn aimed_at_an_id_the_next_workspace_takes() -> Aimed {
    let (session, _) = machine_with(&["aimed", "next"]);
    restarted(&session);
    let (boot, id) = (boot(&session), id_of(&session, "aimed"));
    remote_says(&session, &["close", "--workspace", &id]);
    restarted(&session);
    assert_eq!(id_of(&session, "next"), id, "the workspace after it took its id");
    Aimed { session, boot, id }
}

#[test]
fn a_close_aimed_before_a_restart_leaves_the_workspace_that_took_its_id() {
    let Aimed { session, boot, id } = aimed_at_an_id_the_next_workspace_takes();

    let out = there(&session, &["close", "--workspace", &id, "--boot", &boot]);

    assert_stale(&out, &boot);
    assert_eq!(id_of(&session, "next"), id);
    remote_says(&session, &["close", "--workspace", &id]);
    assert!(!lists(&session, &id), "without it, the command closes what has the id now");
}

#[test]
fn a_rename_aimed_before_a_restart_leaves_the_workspace_that_took_its_id() {
    let Aimed { session, boot, id } = aimed_at_an_id_the_next_workspace_takes();

    let out = there(&session, &["rename", "--workspace", &id, "renamed", "--boot", &boot]);

    assert_stale(&out, &boot);
    assert_eq!(id_of(&session, "next"), id);
}

#[test]
fn every_form_of_close_and_rename_refuses_an_earlier_boot_and_changes_nothing() {
    let (session, project) = machine_with(&["aimed"]);
    let workspace = id_of(&session, "aimed");
    let pane = panes(session.report()).next().expect("a pane").id.to_string();
    let status: Value = serde_json::from_str(&session.says(&["status", "--json"])).expect("a status report");
    let tab = status["projects"][0]["workspaces"][0]["tabs"][0]["id"].to_string();
    let forms: [&[&str]; 15] = [
        &["close", "--pane", &pane],
        &["close", "--tab", &tab],
        &["close", "--workspace", &workspace],
        &["close", "--worktree", "main"],
        &["close", "--project", &project],
        &["close", "--workspace", &workspace, "--remove-worktree"],
        &["close", "--workspace", &workspace, "--remove-worktree", "--check", "--json"],
        &["close", "--agent", "--pane", &pane],
        &["close", "--agent", "--pane", &pane, "--check"],
        &["rename", "--tab", &tab, "x"],
        &["rename", "--workspace", &workspace, "x"],
        &["rename", "--worktree", "main", "x"],
        &["rename", "--project", &project, "x"],
        &["rename", "--group", "99999", "x"],
        &["rename", "x"],
    ];

    refuses_every_form_and_changes_nothing(&session, &forms);
}

fn refuses_every_form_and_changes_nothing(session: &Session, forms: &[&[&str]]) {
    let before = rows(session);
    for form in forms {
        assert_stale(&there(session, &at(form, EARLIER)), EARLIER);
    }
    assert_eq!(rows(session), before);
}

struct Panes {
    session: Arc<Session>,
    agent: String,
    agent_tab: String,
    shell: String,
    split: String,
}

fn pane_in(session: &Session, id: &str) -> Value {
    let status: Value = serde_json::from_str(&session.says(&["status", "--json"])).expect("a status report");
    let mut panes = Vec::new();
    walk(&status, &mut |row| panes.extend(row["panes"].as_array().cloned().unwrap_or_default()));
    panes.into_iter().find(|pane| pane["id"].as_u64().is_some_and(|n| n.to_string() == id)).unwrap_or(Value::Null)
}

fn walk(row: &Value, visit: &mut dyn FnMut(&Value)) {
    visit(row);
    for key in ["projects", "workspaces", "tabs"] {
        row[key].as_array().into_iter().flatten().for_each(|item| walk(item, visit));
    }
}

fn a_machine_with_an_agent_and_a_split_tab() -> Panes {
    let session = agent_session();
    remote_says(&session, &["start-server"]);
    let shell = session.report().projects[0].workspaces[0].tabs[0].panes[0].id.to_string();
    let split = remote_says(&session, &["split", "--pane", &shell]);
    let started: Value = serde_json::from_str(&remote_says(&session, &["start", "claude", "--json"])).expect("start");
    let (agent, agent_tab) = (started["pane"].to_string(), started["tab"].to_string());
    session.says(&["account", "add", "claude", "work"]);
    wait_until("the agent is seen", || pane_in(&session, &agent)["agent"] == "claude");
    Panes { session, agent, agent_tab, shell, split }
}

#[test]
fn the_boot_of_this_run_lets_close_and_rename_act() {
    let (session, _) = machine_with(&["kept", "closed"]);
    let (boot, kept, closed) = (boot(&session), id_of(&session, "kept"), id_of(&session, "closed"));
    let before = ssh_runs(&session);

    let renamed = remote_says(&session, &["rename", "--workspace", &kept, "renamed", "--boot", &boot]);
    let gone = remote_says(&session, &["close", "--workspace", &closed, "--boot", &boot]);

    assert_eq!((renamed.as_str(), gone.as_str()), ("", ""));
    assert_eq!(ssh_runs(&session), before + 2, "the feature check and the request share one ssh connection");
    assert_eq!(id_of(&session, "renamed"), kept);
    assert!(!lists(&session, &closed), "{closed} is gone");
}

#[test]
fn a_worktree_is_checked_and_removed_with_the_boot_of_this_run() {
    let (session, project) = machine_with(&[]);
    remote_says(&session, &["focus", "--project", &project]);
    remote_says(&session, &["new-workspace", "feat/x", "--worktree", "--project", &project]);
    let boot = boot(&session);
    let by_branch = ["close", "--worktree", "feat/x", "--remove-worktree"];

    let check = remote_says(&session, &at(&[&by_branch[..], &["--check", "--json"]].concat(), &boot));
    let check: Value = serde_json::from_str(&check).expect("the check");
    let path = check["path"].as_str().expect("the worktree's path").to_string();
    assert!(std::path::Path::new(&path).exists(), "{check}");
    remote_says(&session, &at(&by_branch, &boot));

    wait_until("the worktree leaves the disk", || !std::path::Path::new(&path).exists());
}

#[test]
fn every_form_of_mark_move_pane_account_restart_and_context_refuses_an_earlier_boot_and_changes_nothing() {
    let Panes { session, agent, agent_tab, shell, split } = a_machine_with_an_agent_and_a_split_tab();
    let forms: [&[&str]; 13] = [
        &["mark"],
        &["mark", "--pane", &agent],
        &["mark", "--tab", &agent_tab],
        &["mark", "--pane", &agent, "--reviewed"],
        &["move-pane", "--pane", &split, "--target", &shell, "--place", "left"],
        &["move-pane", "--pane", &split, "--target", &shell, "--place", "swap", "--json"],
        &["account", "restart", "claude", "work"],
        &["account", "restart", "claude", "work", "--pane", &agent, "--json"],
        &["account", "restart", "claude", "work", "--pane", &shell, "--pane", &split],
        &["context"],
        &["context", "--pane", &agent],
        &["context", "--tab", &agent_tab, "--json"],
        &["context", "--pane", &agent, "--cached"],
    ];

    refuses_every_form_and_changes_nothing(&session, &forms);

    let cached = remote_says(&session, &["context", "--pane", &agent, "--cached", "--json"]);
    let cached: Value = serde_json::from_str(&cached).expect("the context");
    assert_eq!((&cached["loading"], &cached["read_at"]), (&json!(false), &Value::Null), "{cached}");
}

#[test]
fn the_boot_of_this_run_lets_mark_move_pane_account_restart_and_context_act() {
    let Panes { session, agent, shell, split, .. } = a_machine_with_an_agent_and_a_split_tab();
    std::fs::create_dir_all(session.claude_dir()).expect("the default account's folder");
    let boot = boot(&session);
    let before = ssh_runs(&session);

    let marked = remote_says(&session, &at(&["mark", "--pane", &agent], &boot));
    let context = remote_says(&session, &at(&["context", "--pane", &agent, "--cached", "--json"], &boot));
    let moved = remote_says(
        &session,
        &at(&["move-pane", "--pane", &split, "--target", &shell, "--place", "left", "--json"], &boot),
    );
    let restarted =
        remote_says(&session, &at(&["account", "restart", "claude", "work", "--pane", &split, "--json"], &boot));

    assert_eq!(marked, "");
    assert_eq!(pane_in(&session, &agent)["status"], "pending");
    let context: Value = serde_json::from_str(&context).expect("the context");
    assert_eq!(context["pane"].to_string(), agent, "{context}");
    let moved: Value = serde_json::from_str(&moved).expect("the layout");
    assert!(moved["layout"].is_object(), "{moved}");
    let restarted: Value = serde_json::from_str(&restarted).expect("the restart");
    assert_eq!(restarted["restarted"], json!([split.parse::<u64>().expect("an id")]), "{restarted}");
    assert_eq!(ssh_runs(&session), before + 4, "each command's checks and request share one ssh connection");
}

#[rstest]
#[case::close(&["close", "--workspace", "99999"], "there is no workspace 99999")]
#[case::check(&["close", "--workspace", "99999", "--remove-worktree", "--check"], "there is no workspace 99999")]
#[case::agent(&["close", "--agent", "--pane", "99999"], "there is no pane 99999")]
#[case::rename(&["rename", "--workspace", "99999", "x"], "there is no workspace 99999")]
#[case::mark(&["mark", "--pane", "99999"], "there is no pane 99999")]
#[case::move_pane(&["move-pane", "--pane", "99999", "--target", "99998", "--place", "swap"], "there is no pane 99999")]
#[case::account_restart(&["account", "restart", "claude", "--pane", "99999"], "there is no pane 99999")]
#[case::context(&["context", "--pane", "99999"], "there is no pane 99999")]
fn any_other_refusal_keeps_its_message_and_status(#[case] args: &[&str], #[case] message: &str) {
    let (session, _) = machine_with(&[]);
    let boot = boot(&session);

    let out = there(&session, &at(args, &boot));

    assert!(refused(&out, 1, message), "{out:?}");
    assert!(!String::from_utf8_lossy(&out.stderr).contains("cannot read"), "{out:?}");
}

#[test]
fn a_line_of_requests_takes_a_boot_for_itself_through_one_ssh_connection() {
    let (session, _) = machine_with(&["kept", "closed"]);
    let (boot, kept, closed) = (boot(&session), id_of(&session, "kept"), id_of(&session, "closed"));
    let before = ssh_runs(&session);
    let mut typist = Typist::start(remotely(&session, &["requests", "--follow", "-"]));

    let stale = typist.types(&json!({"args": at(&["close", "--workspace", &closed], EARLIER)}));
    let listed = typist.types(&json!({"args": ["status", "--json"]}));
    let renamed = typist.types(&json!({"args": at(&["rename", "--workspace", &kept, "renamed"], &boot)}));
    let gone = typist.types(&json!({"args": at(&["close", "--workspace", &closed], &boot)}));
    let ended = typist.ends();

    assert_eq!(stale["status"], 4, "{stale}");
    assert!(stale["error"].as_str().is_some_and(|text| says_it_restarted(text, EARLIER)), "{stale}");
    assert!(stale.get("text").is_none() && stale.get("json").is_none(), "{stale}");
    assert_eq!((&listed["status"], &listed["json"]["boot"]), (&json!(0), &json!(boot)), "a line after it: {listed}");
    assert_eq!((&renamed["status"], &gone["status"]), (&json!(0), &json!(0)), "{renamed} {gone}");
    assert_eq!(id_of(&session, "renamed"), kept);
    assert!(!lists(&session, &closed), "{closed} is gone");
    assert_eq!((ended.status.code(), ssh_runs(&session)), (Some(3), before + 1), "{ended:?}");
}

#[test]
fn the_boot_a_machines_entry_lists_is_the_one_its_workspaces_take() {
    let machine = Session::new();
    let here = Session::new();
    start_listing(&here, &[(VPS, VPS)], &[(VPS, &reach(&machine))], json!({}));
    wait_for_machine(&here, VPS, "the machine is linked", in_state("linked"));
    let repo = repository(&machine.dir, "api").display().to_string();
    let project = machine.says(&["open", &repo]);
    let workspace = machine.says(&["new-workspace", "notes", "--project", &project]);
    let mut entry = Value::Null;
    wait_until("the machine's workspace is listed here", || {
        let status: Value = serde_json::from_str(&here.says(&["status", "--json"])).expect("a status report");
        entry = status["machines"][0].clone();
        entry["projects"].to_string().contains("notes")
    });
    let boot = entry["boot"].as_str().expect("the machine's boot");
    assert_ne!(json!(boot), boot_of(&here.says(&["status", "--json"])), "the machine's, not this server's");

    let mut renaming = here.cold_command();
    renaming.env("PATH", fake_ssh_to(&here.dir, &[(VPS, &reach(&machine))]));
    let out = renaming.args(["--remote", VPS, "rename", "--workspace", &workspace, "done", "--boot", boot]);
    let out = out.output().expect("run cornercase");

    assert!(out.status.success(), "{out:?}");
    assert_eq!(id_of(&machine, "done"), workspace);
}

mod a_server_that_knows_no_boot {
    use super::*;

    const WITHOUT_THE_FEATURE: &str =
        r#"{"ok":{"version":"0.0.1","contract":1,"boot":"1-1","operations":[],"features":["views"],"streams":[]}}"#;
    const WITHOUT_A_CONTRACT: &str = r#"{"error":"the running cornercase server (0.0.1) has no `contract` command"}"#;

    fn read_frame(stream: &mut UnixStream) -> Option<String> {
        let mut length = [0; 4];
        stream.read_exact(&mut length).ok()?;
        let mut message = vec![0; usize::try_from(u32::from_le_bytes(length)).ok()?];
        stream.read_exact(&mut message).ok()?;
        let text = 2 + message[1..].iter().position(|byte| byte & 0x80 == 0)?;
        Some(String::from_utf8_lossy(&message[text..]).into_owned())
    }

    fn write_frame(stream: &mut UnixStream, text: &str) {
        let mut message = vec![ANSWER];
        let mut length = text.len();
        while length >= 0x80 {
            message.push(u8::try_from(length & 0x7f).expect("seven bits") | 0x80);
            length >>= 7;
        }
        message.push(u8::try_from(length).expect("seven bits"));
        message.extend_from_slice(text.as_bytes());
        stream.write_all(&u32::try_from(message.len()).expect("a short message").to_le_bytes()).expect("its length");
        stream.write_all(&message).expect("an answer");
    }

    fn answering(session: &Session, answer: &'static str) -> thread::JoinHandle<Vec<String>> {
        let listener = UnixListener::bind(session.socket()).expect("the fake server's socket");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("a client");
            let mut seen = Vec::new();
            while let Some(text) = read_frame(&mut stream) {
                seen.push(text);
                write_frame(&mut stream, answer);
            }
            seen
        })
    }

    #[rstest]
    #[case::close_without_the_feature(WITHOUT_THE_FEATURE, &["close", "--workspace", "7", "--boot", "1-1"])]
    #[case::rename_without_the_feature(WITHOUT_THE_FEATURE, &["rename", "--workspace", "7", "x", "--boot", "1-1"])]
    #[case::close_without_a_contract(WITHOUT_A_CONTRACT, &["close", "--workspace", "7", "--boot", "1-1"])]
    #[case::mark_without_the_feature(WITHOUT_THE_FEATURE, &["mark", "--pane", "7", "--boot", "1-1"])]
    #[case::move_pane_without_the_feature(
        WITHOUT_THE_FEATURE,
        &["move-pane", "--pane", "7", "--target", "8", "--place", "swap", "--boot", "1-1"]
    )]
    #[case::account_restart_without_the_feature(
        WITHOUT_THE_FEATURE,
        &["account", "restart", "claude", "--pane", "7", "--boot", "1-1"]
    )]
    #[case::context_without_a_contract(WITHOUT_A_CONTRACT, &["context", "--pane", "7", "--boot", "1-1"])]
    fn is_refused_by_name_and_never_gets_the_request(#[case] answer: &'static str, #[case] args: &[&str]) {
        let (here, far) = (Session::new(), Session::new());
        let (near, away) = (answering(&here, answer), answering(&far, answer));

        let locally = here.cold(args);
        let remotely = there(&far, args);

        assert!(refused(&locally, 1, "server would ignore --boot"), "{locally:?}");
        assert!(refused(&remotely, 1, "server on `devbox` would ignore --boot"), "{remotely:?}");
        for seen in [near.join().expect("the fake server"), away.join().expect("the fake server")] {
            assert_eq!(seen.len(), 1, "{seen:?}");
            assert!(seen[0].contains(r#""command":"contract""#), "{seen:?}");
        }
    }
}
