use super::*;

#[test]
fn status_lists_the_window_and_marks_the_pane_it_runs_in() {
    let mut app = Harness::start();

    let report = app.session.report();
    let inside = app.session.dir.join("inside.json");
    app.send(format!("{} status --json > '{}'; echo status-\"\"saved\r", bin(), inside.display()).as_bytes());
    app.wait_for("the status is saved", |s| s.contains("status-saved"));

    let seen: Report = serde_json::from_str(&std::fs::read_to_string(&inside).expect("read it")).expect("a report");
    assert_eq!(report.projects[0].path, temp());
    assert_eq!((report.caller, seen.caller), (None, report.shown.pane));
}

#[test]
fn status_panes_gives_every_pane_a_row_of_its_own() {
    let mut app = Harness::start();
    let split = app.session.says(&["split"]);
    let inside = app.session.dir.join("panes.tsv");

    app.send(format!("{} status --panes > '{}'; echo panes-\"\"saved\r", bin(), inside.display()).as_bytes());
    app.wait_for("the panes are saved", |s| s.contains("panes-saved"));

    let report = app.session.report();
    let rows: PaneRows = serde_json::from_str(&app.session.says(&["status", "--panes", "--json"])).expect("rows");
    let text = std::fs::read_to_string(&inside).expect("read it");
    let lines: Vec<Vec<&str>> = text.lines().map(|line| line.split('\t').collect()).collect();
    let column = |name: &str| PaneRow::COLUMNS.iter().position(|c| *c == name).expect("a column");
    let marked = |name: &str| lines[1..].iter().filter(|l| l[column(name)] == "true").map(|l| l[0]).collect::<Vec<_>>();
    let shown = report.shown.pane.expect("a shown pane").to_string();
    let ids: Vec<u64> = panes(report).map(|p| p.id).collect();
    assert_eq!(lines[0], PaneRow::COLUMNS);
    assert!(lines.iter().all(|line| line.len() == PaneRow::COLUMNS.len()), "{text}");
    assert_eq!(lines[1..].iter().map(|line| line[0].parse().expect("an id")).collect::<Vec<u64>>(), ids);
    assert!(ids.contains(&split.parse().expect("an id")), "{text}");
    assert_eq!((marked("caller"), marked("shown")), (vec![shown.as_str()], vec![shown.as_str()]));
    assert_eq!(rows.panes.iter().map(|row| row.pane).collect::<Vec<_>>(), ids);
    assert!(rows.panes.iter().all(|row| !row.caller), "{rows:?}");
}

#[test]
fn open_adds_a_project_and_leaves_the_window_as_it_is() {
    let mut app = Harness::start();
    let name = format!("ccop-{}", std::process::id());
    let dir = temp_dir_named(&name);
    let path = dir.display().to_string();

    let id = app.session.says(&["open", &path]);

    app.wait_for("the project shows", |s| s.contains(&entry(&name)));
    assert!(app.text().contains(&format!("▌{}", first_entry())), "the window moved:\n{}", app.text());
    assert_eq!(app.session.says(&["open", &path]), id);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn new_workspace_adds_one_to_the_project() {
    let app = Harness::start();

    let id = app.session.says(&["new-workspace", "notes"]);

    let report = app.session.report();
    let workspace = report.projects[0].workspaces.iter().find(|w| w.id.to_string() == id).expect("the workspace");
    let shown = report.shown.workspace == Some(workspace.id);
    assert_eq!((workspace.name.as_str(), workspace.tabs.len(), shown), ("notes", 1, false));
}

#[test]
fn a_command_typed_in_a_new_tab_is_waited_for_and_read() {
    let mut app = Harness::start();

    let pane = app.session.says(&["new-tab", "--name", "build", "--", "sleep 0.5; echo built-$((40+2))"]);
    let ended = app.session.says(&["wait", "--pane", &pane, "--until", "shell", "--timeout", "10"]);

    assert_eq!(ended, "shell");
    assert!(app.session.says(&["read", "--pane", &pane, "--lines", "5"]).contains("built-42"));
    app.wait_for("the tab shows in the list", |s| s.contains("build"));
}

#[test]
fn search_finds_what_a_pane_printed_with_its_ids() {
    let mut app = Harness::start();
    let pane = app.session.says(&["new-tab", "--name", "build", "--", "echo needle-\"\"found"]);
    app.session.says(&["wait", "--pane", &pane, "--until", "shell", "--timeout", "10"]);
    app.wait_for("the tab shows in the list", |s| s.contains("build"));

    let table = app.session.says(&["search", "NEEDLE-found"]);
    let rows: Vec<Vec<&str>> = table.lines().map(|line| line.split('\t').collect()).collect();
    let json: serde_json::Value =
        serde_json::from_str(&app.session.says(&["search", "needle-found", "--json"])).expect("json");
    let none = app.session.says(&["search", "no-such-text-anywhere"]);

    assert_eq!(rows[0], ["pane", "who", "source", "place", "line", "text"]);
    assert_eq!((rows[1][0], rows[1][2]), (pane.as_str(), "screen"));
    assert!(rows[1][5].ends_with("needle-found"), "{table}");
    assert!(rows[1][3].ends_with("build"), "{table}");
    assert_eq!(json["matches"][0]["pane"], pane.parse::<u64>().expect("an id"));
    assert_eq!(none, "pane\twho\tsource\tplace\tline\ttext");
}

#[test]
fn focus_reveals_a_search_match_and_selects_it() {
    let mut app = Harness::start();
    let pane = app.session.says(&["new-tab", "--name", "build", "--", "echo needle-\"\"found; seq 1 60"]);
    app.session.says(&["wait", "--pane", &pane, "--until", "shell", "--timeout", "10"]);
    let found: serde_json::Value =
        serde_json::from_str(&app.session.says(&["search", "needle-found", "--json"])).expect("json");
    let line = found["matches"][0]["line"].to_string();

    let args = ["focus", "--pane", &pane, "--reveal", "needle-found", "--line", &line, "--json"];
    let revealed: serde_json::Value = serde_json::from_str(&app.session.says(&args)).expect("json");
    let selected = app.session.says(&["selection", "--pane", &pane]);
    app.wait_for("the window shows the match", |s| s.contains("needle-found"));
    let lost = app.session.cli(&["focus", "--pane", &pane, "--reveal", "no-such-text-anywhere"]);

    assert_eq!(revealed["revealed"], true);
    assert_eq!(selected, "needle-found");
    assert_eq!(lost.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&lost.stderr).contains("warning:"), "{lost:?}");
}

pub(super) fn new_tab(app: &Harness) -> (String, String) {
    let pane = app.session.says(&["new-tab"]);
    let rows: PaneRows = serde_json::from_str(&app.session.says(&["status", "--panes", "--json"])).expect("pane rows");
    let tab = rows.panes.iter().find(|row| row.pane.to_string() == pane).expect("the new pane").tab.to_string();
    (pane, tab)
}

pub(super) struct Followed {
    lines: std::sync::mpsc::Receiver<String>,
    pub(super) seen: Vec<serde_json::Value>,
}

impl Followed {
    pub(super) fn of(child: &mut std::process::Child) -> Self {
        let out = child.stdout.take().expect("its output");
        let (tx, lines) = std::sync::mpsc::channel();
        thread::spawn(move || forward_lines(out, &tx));
        Self { lines, seen: Vec::new() }
    }

    pub(super) fn until(&mut self, what: &str, cond: impl Fn(&serde_json::Value) -> bool) {
        while !self.seen.iter().any(&cond) {
            self.next(what);
        }
    }

    pub(super) fn next(&mut self, what: &str) -> serde_json::Value {
        let line = self.lines.recv_timeout(TIMEOUT).unwrap_or_else(|_| panic!("timed out waiting for: {what}"));
        let value: serde_json::Value = serde_json::from_str(&line).expect("one JSON object per line");
        self.seen.push(value.clone());
        value
    }
}

fn forward_lines(out: std::process::ChildStdout, tx: &std::sync::mpsc::Sender<String>) {
    for line in std::io::BufRead::lines(std::io::BufReader::new(out)).map_while(Result::ok) {
        if tx.send(line).is_err() {
            return;
        }
    }
}

fn told(event: &serde_json::Value, what: &str, key: &str, value: &str, pane: &str) -> bool {
    event["event"] == what && event[key] == value && event["pane"].as_u64() == pane.parse().ok()
}

#[test]
fn events_follow_what_opens_ends_and_closes_until_the_server_stops() {
    let app = Harness::start();
    let first = panes(app.session.report()).next().expect("a pane").id.to_string();
    let mut all = app.session.spawn(&["events", "--json"]);
    let one = app.session.spawn(&["events", "--pane", &first]);
    app.session.wait_for_file("server.log", "both follow", |log| log.matches("command=events").count() == 2);
    let mut followed = Followed::of(&mut all);

    let pane = app.session.says(&["new-tab", "--", "sleep 2"]);
    followed.until("the tab opens", |event| told(event, "opened", "kind", "pane", &pane));
    followed.until("sleep ends", |event| told(event, "exited", "program", "sleep", &pane));
    app.session.says(&["close", "--pane", &first]);
    let one = finish_within(one, TIMEOUT);
    followed.until("the first pane closes", |event| told(event, "closed", "kind", "pane", &first));
    let stopped = app.session.run("kill-server");

    assert!(stopped.status.success(), "{stopped:?}");
    assert!(finish_within(all, TIMEOUT).status.success(), "events ends with the server");
    assert_eq!(
        (one.status.code(), String::from_utf8_lossy(&one.stdout)),
        (Some(0), format!("pane {first}  closed\n").into())
    );
}

#[test]
fn events_host_tells_a_programs_clipboard_and_bell_and_the_log_keeps_neither() {
    let mut app = Harness::open_with(Session::new(), ROWS, COLS, &[("CORNERCASE_LOG", "debug")]);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    let mut host = app.session.spawn(&["events", "--host", "--json"]);
    let mut plain = app.session.spawn(&["events", "--json"]);
    app.session.wait_for_file("server.log", "both follow", |log| log.matches("command=events").count() == 2);
    let (mut hosted, mut plainly) = (Followed::of(&mut host), Followed::of(&mut plain));

    let pane = app.session.says(&["new-tab", "--", "printf '\\033]52;c;Y2xpcC1tYXJrZXItNDU=\\007\\007'; sleep 1"]);
    hosted.until("the clipboard write", |e| e["event"] == "clipboard" && e["text"] == "clip-marker-45");
    hosted.until("the bell", |e| e["event"] == "bell" && e["pane"].as_u64() == pane.parse().ok());
    hosted.until("sleep ends", |e| told(e, "exited", "program", "sleep", &pane));
    plainly.until("sleep ends", |e| told(e, "exited", "program", "sleep", &pane));
    let log = app.session.says(&["logs", "-n", "1000"]);
    let stopped = app.session.run("kill-server");

    assert!(stopped.status.success(), "{stopped:?}");
    assert!(plainly.seen.iter().all(|e| e["event"] != "clipboard" && e["event"] != "bell"), "{:?}", plainly.seen);
    assert!(!log.contains("clip-marker"), "{log}");
    drop((host, plain));
}

fn first_tab(snapshot: &serde_json::Value) -> &serde_json::Value {
    &snapshot["projects"][0]["workspaces"][0]["tabs"][0]
}

#[test]
fn status_follow_streams_whole_snapshots_and_a_new_one_starts_from_now() {
    let app = Harness::start();
    let mut watching = app.session.spawn(&["status", "--follow"]);
    let mut followed = Followed::of(&mut watching);
    followed.until("the first snapshot", |snapshot| snapshot["revision"].as_u64().is_some());
    let first = followed.seen[0].clone();

    app.session.says(&["rename", "renamed-by-script"]);
    followed.until("the new name", |snapshot| first_tab(snapshot)["name"] == "renamed-by-script");
    let renamed = followed.seen.last().expect("a snapshot").clone();
    let mut again = app.session.spawn(&["status", "--follow"]);
    let mut refollowed = Followed::of(&mut again);
    refollowed.until("the current state", |snapshot| snapshot["revision"].as_u64().is_some());
    let current = refollowed.seen[0].clone();
    let stopped = app.session.run("kill-server");

    assert!(stopped.status.success(), "{stopped:?}");
    assert!(finish_within(watching, TIMEOUT).status.success(), "status --follow ends with the server");
    assert!(finish_within(again, TIMEOUT).status.success(), "the second one too");
    assert!(first["groups"].is_array() && first_tab(&first)["layout"]["leaf"].is_u64(), "{first}");
    assert!(renamed["revision"].as_u64() > first["revision"].as_u64(), "{renamed}");
    assert_eq!((&current["boot"], first_tab(&current)["name"].as_str()), (&first["boot"], Some("renamed-by-script")));
    assert!(current["revision"].as_u64() >= renamed["revision"].as_u64(), "{current}");
}

#[rstest]
#[case::events(&["events", "--json"], "command=events", 0)]
#[case::the_rest_of_the_app(&["status", "--app", "--follow"], "command=app", 1)]
fn a_follow_ends_soon_after_its_reader_leaves(#[case] args: &[&str], #[case] asked: &str, #[case] printed: usize) {
    let (session, mut server) = windowless();
    let mut follow = session.spawn(args);
    let mut out = std::io::BufReader::new(follow.stdout.take().expect("its output"));
    session.wait_for_file("server.log", "it follows", |log| log.contains(asked));
    for _ in 0..printed {
        std::io::BufRead::read_line(&mut out, &mut String::new()).expect("a line");
    }

    drop(out);
    let ended = finish_within(follow, TIMEOUT);

    assert_eq!((ended.status.code(), ended.stderr.as_slice()), (Some(0), &b""[..]), "{ended:?}");
    drop(session);
    let _ = server.wait();
}

#[test]
fn several_panes_are_waited_for_at_once() {
    let app = Harness::start();
    let quick = app.session.says(&["new-tab", "--", "sleep 0.2"]);
    let slow = app.session.says(&["new-tab", "--", "sleep 4"]);
    let both = ["--pane", slow.as_str(), "--pane", quick.as_str(), "--until", "shell", "--timeout", "20"];

    let any = app.session.says(&[&["wait", "--any"][..], &both].concat());
    let all = app.session.says(&[&["wait", "--all"][..], &both].concat());
    let neither = app.session.cli(&[&["wait"][..], &both].concat());

    assert_eq!(any, format!("{quick} shell"));
    assert_eq!(all, format!("{slow} shell\n{quick} shell"));
    assert_eq!(neither.status.code(), Some(2), "{neither:?}");
}

#[test]
fn split_opens_a_pane_beside_the_shown_one() {
    let mut app = Harness::start();

    let pane = app.session.says(&["split", "--", "echo", "split-\"\"done"]);

    app.wait_for("the new pane runs its command", |s| s.contains("split-done"));
    let tab = &app.session.report().projects[0].workspaces[0].tabs[0];
    assert_eq!(tab.panes.iter().map(|p| p.id.to_string()).collect::<Vec<_>>()[1], pane);
}

fn answer_twice(session: &Session) -> Vec<serde_json::Value> {
    let mut messages = Vec::new();
    for prompt in ["first answer", "second answer"] {
        let start = session.spawn(&["start", "claude", "--prompt", prompt, "--wait", "--timeout", "30"]);
        session.wait_for_working();
        std::fs::write(session.claude_dir().join("finish"), "").expect("let the agent finish");
        let started = Session::output(start);
        let pane = started.lines().next().expect("pane");
        let value: serde_json::Value =
            serde_json::from_str(&session.says(&["read", "--pane", pane, "--last-message", "--json"])).expect("json");
        messages.push(value);
    }
    messages
}

fn assert_read_in_the_order_given(session: &Session, (first, second): (&str, &str), messages: &[serde_json::Value]) {
    let tab = session.report().panes().iter().find(|p| p.pane.to_string() == first).expect("agent pane").tab;
    let tab = tab.to_string();
    let args = ["read", "--last-message", "--tab", &tab, "--pane", second, "--pane", second, "--tab", &tab];
    let json: serde_json::Value =
        serde_json::from_str(&session.says(&[&args[..], &["--json"]].concat())).expect("json");
    assert_eq!(json, serde_json::json!({ "panes": [messages[1], messages[0]] }));
    let text = session.says(&args);
    assert_eq!(
        text,
        format!(
            "pane {second}  claude  turn_over=true  written=2026-10-08T12:00:01.000Z\nsaid: second answer\n\n\
             pane {first}  claude  turn_over=true  written=2026-10-08T12:00:01.000Z\nsaid: first answer"
        )
    );
    let repeated = session.cli(&["read", "--last-message", "--pane", first, "--pane", first]);
    assert!(repeated.status.success(), "{repeated:?}");
    assert_eq!(repeated.stdout, b"said: first answer\n");
}

fn mixed_read<'a>(shell: &'a str, first: &'a str, empty: &'a str, second: &'a str) -> Vec<&'a str> {
    let panes = [shell, first, empty, second];
    let mut args = vec!["read", "--last-message"];
    for pane in panes {
        args.extend(["--pane", pane]);
    }
    args.extend(["--tab", "999999"]);
    args
}

fn assert_failures_in_json(session: &Session, mixed: &[&str], messages: &[serde_json::Value]) {
    let failed = session.cli(&[mixed, &["--json"]].concat());
    assert_eq!(failed.status.code(), Some(3), "{failed:?}");
    assert!(failed.stderr.is_empty(), "{failed:?}");
    let json: serde_json::Value = serde_json::from_slice(&failed.stdout).expect("json");
    let entries = json["panes"].as_array().expect("panes");
    assert_eq!(entries.len(), 5);
    assert_eq!(
        (entries[0]["pane"].to_string(), entries[2]["pane"].to_string()),
        (mixed[3].to_string(), mixed[7].to_string())
    );
    let error = |at: usize| entries[at]["error"].as_str().expect("error");
    assert!(error(0).contains("runs no agent") && error(2).contains("has not written a message yet"));
    assert!(error(4).contains("no tab") && entries[4]["tab"] == 999_999);
    assert_eq!((&entries[1], &entries[3]), (&messages[0], &messages[1]));
}

fn assert_failures_in_text(session: &Session, mixed: &[&str]) {
    let (shell, first, empty, second) = (mixed[3], mixed[5], mixed[7], mixed[9]);
    let failed = session.cli(mixed);
    assert_eq!(failed.status.code(), Some(3), "{failed:?}");
    let text = String::from_utf8(failed.stdout).expect("text");
    let positions: Vec<usize> = [
        format!("pane {shell}\nerror:"),
        format!("pane {first}  claude"),
        format!("pane {empty}\nerror:"),
        format!("pane {second}  claude"),
        "tab 999999\nerror:".into(),
    ]
    .iter()
    .map(|header| text.find(header).expect("header"))
    .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{text}");
}

#[test]
fn several_agents_messages_are_read_in_order_and_failures_leave_the_others_visible() {
    let app = agent_app();
    let session = &app.session;
    let messages = answer_twice(session);
    let first = messages[0]["pane"].to_string();
    let second = messages[1]["pane"].to_string();
    assert_read_in_the_order_given(session, (&first, &second), &messages);
    let empty = session.says(&["start", "claude"]);
    session.wait_for_report("the agent has a record without any messages", |_| {
        let out = session.cli(&["read", "--last-message", "--pane", &empty]);
        String::from_utf8_lossy(&out.stderr).contains("has not written a message yet")
    });
    let shell = session.report().projects[0].workspaces[0].tabs[0].panes[0].id.to_string();
    let mixed = mixed_read(&shell, &first, &empty, &second);
    assert_failures_in_json(session, &mixed, &messages);
    assert_failures_in_text(session, &mixed);
    let screens = session.cli(&["read", "--pane", &first, "--pane", &second]);
    assert!(refused(&screens, 2, "needs --last-message"), "{screens:?}");
}

fn agent_app() -> Harness {
    let session = agent_session();
    let mut app = Harness::open(Arc::clone(&session), ROWS, COLS);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    app
}

fn let_agent_finish(session: &Session) {
    session.wait_for_working();
    std::fs::write(session.claude_dir().join("finish"), "").expect("let the agent finish");
}

fn send_and_finish(session: &Session, pane: &str, until: &[&str], text: &str) -> String {
    let args = [&["send", "--pane", pane, "--enter", "--wait"][..], until, &[text]].concat();
    let send = session.spawn(&args);
    let_agent_finish(session);
    Session::output(send)
}

#[test]
fn an_agent_started_from_the_command_line_takes_its_prompts_is_waited_for_and_read() {
    let app = agent_app();
    let session = &app.session;

    let start = session.spawn(&["start", "claude", "--prompt", "fix the login", "--wait", "--timeout", "30"]);
    let_agent_finish(session);
    let started = Session::output(start);
    let lines: Vec<&str> = started.lines().collect();
    let [pane, ended] = lines[..] else { panic!("not a pane and an ending: {started}") };
    let first = session.says(&["read", "--pane", pane, "--last-message"]);
    let next = send_and_finish(session, pane, &["--timeout", "30"], "add tests");
    let turn = send_and_finish(session, pane, &["--until", "turn-over"], "watch in the background");
    let status = session.says(&["status"]);
    std::fs::write(session.claude_dir().join("finish"), "").expect("let the background shell end");
    let last = session.says(&["wait", "--pane", pane, "--timeout", "30"]);

    session.says(&["keys", "--pane", pane, "p", "a", "n", "e", "l", "enter"]);
    let unrecorded = session.cli(&["send", "--pane", pane, "--enter", "--timeout", "1", "never recorded"]);

    assert_eq!((ended, next.as_str(), turn.as_str(), last.as_str()), ("done", "done", "shell", "done"));
    assert!(status.contains("working (background shell)"), "{status}");
    assert!(refused(&unrecorded, 1, "has not recorded the prompt yet"), "{unrecorded:?}");
    let screen = session.says(&["read", "--pane", pane]);
    assert!(screen.contains("done: fix the login") && screen.contains("done: add tests"), "{screen}");
    let said: serde_json::Value =
        serde_json::from_str(&session.says(&["read", "--pane", pane, "--last-message", "--json"])).expect("json");
    let shell = session.report().projects[0].workspaces[0].tabs[0].panes[0].id.to_string();
    let no_agent = session.cli(&["read", "--pane", &shell, "--last-message"]);

    assert_eq!(first, "said: fix the login");
    assert_eq!(
        (&said["text"], &said["agent"], &said["turn_over"], &said["written"]),
        (&"said: watch in the background".into(), &"claude".into(), &true.into(), &"2026-10-08T12:00:03.000Z".into())
    );
    assert!(refused(&no_agent, 1, &format!("pane {shell} runs no agent")), "{no_agent:?}");
}

#[test]
fn keys_reach_the_program_in_the_pane() {
    let mut app = Harness::start();
    app.send(b"echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs", |s| s.contains("cat-starts"));

    app.session.says(&["keys", "ctrl+b", "x", "enter"]);

    app.wait_for("cat echoes them", |s| s.matches("^Bx").count() >= 2);
}

#[test]
fn close_ends_a_tab_and_its_shell() {
    let mut app = Harness::start();
    let pane = app.session.says(&["new-tab", "--name", "doomed"]);
    app.wait_for("the tab shows", |s| s.contains("doomed"));

    app.session.says(&["close", "--pane", &pane]);

    app.wait_for("the tab goes", |s| !s.contains("doomed"));
}

#[test]
fn rename_inside_a_pane_names_its_own_tab() {
    let mut app = Harness::start();

    app.send(format!("{} rename from-\"\"inside\r", bin()).as_bytes());

    let tab = usize::from(ui::workspace_row(workspaces_list(), 1, &tab_lines(&[1]), 0, WorkspaceRow::Tab(0, 0)).y);
    app.wait_for("the tab has its name", |s| s.lines().nth(tab).is_some_and(|l| l.contains("from-inside")));
}

#[test]
fn focus_shows_another_tab() {
    let mut app = Harness::start();
    let pane = app.session.says(&["new-tab", "--", "echo", "in-the-\"\"second"]);
    assert!(!app.text().contains("in-the-second"), "the window moved:\n{}", app.text());

    app.session.says(&["focus", "--pane", &pane]);

    app.wait_for("the second tab shows", |s| s.contains("in-the-second"));
}

#[test]
fn notify_reaches_the_window_and_the_desktop() {
    let mut app = Harness::start();

    app.session.says(&["notify", "the", "build", "is", "ready"]);

    app.wait_for_raw("ghostty is asked to notify", |raw| raw.contains("\x1b]777;notify;cornercase;the build is ready"));
    app.wait_for("a toast says it", |s| s.contains("the build is ready"));
}

#[test]
fn the_todo_list_takes_commands() {
    let app = Harness::start();

    let id = app.session.says(&["todo", "add", "buy", "milk"]);
    app.session.says(&["todo", "done", &id]);

    assert_eq!(app.session.says(&["todo", "list"]), format!("{id} [x] buy milk"));
    app.session.wait_for_file("todos.json", "the item is saved", |saved| saved.contains("buy milk"));
}

#[test]
fn a_worktree_made_from_the_command_line_is_removed_by_its_branch() {
    let session = Session::new();
    let worktrees = session.dir.join("worktrees");
    std::fs::write(session.dir.join("config.json"), format!("{{\"worktrees_dir\": \"{}\"}}", worktrees.display()))
        .expect("write config");
    let repo = temp_dir("cli-worktree");
    git(&repo, &["init", "--quiet"]);
    git(&repo, &["commit", "--quiet", "--allow-empty", "-m", "init"]);
    let mut app = Harness::open(Arc::clone(&session), ROWS, COLS);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    let project = session.says(&["open", &repo.display().to_string(), "--focus"]);

    let workspace = session.says(&["new-workspace", "e2e/cli", "--worktree", "--project", &project]);
    let checkout = worktrees.join(repo.file_name().expect("repo name")).join("e2e-cli");
    assert!(checkout.is_dir(), "no checkout at {}", checkout.display());
    let missing = session.cli(&["close", "--worktree", "e2e/other", "--remove-worktree"]);
    assert!(refused(&missing, 2, &format!("workspace {workspace}  e2e/cli  branch e2e/cli  worktree")), "{missing:?}");
    let check = session.says(&["close", "--worktree", "e2e/cli", "--remove-worktree", "--check", "--json"]);
    let check: serde_json::Value = serde_json::from_str(&check).expect("the check");
    assert_eq!((&check["changed"], &check["lock"]), (&false.into(), &serde_json::Value::Null), "{check}");
    assert!(checkout.is_dir(), "the check removed the checkout");
    session.says(&["close", "--worktree", "e2e/cli", "--remove-worktree"]);

    assert!(!checkout.exists(), "the checkout is still there");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn close_agent_checks_then_closes_an_agents_tab_and_refuses_a_shell() {
    let mut app = Harness::start();
    let shell = app.session.report().shown.pane.expect("the first pane").to_string();
    let sessions = app.session.claude_dir().join("sessions");
    std::fs::create_dir_all(&sessions).expect("create the sessions folder");
    let bin = temp_dir("close-agent");
    let claude = bin.join("claude");
    write_executable(
        &claude,
        "#!/bin/sh\nprintf '{\"pid\":%s,\"status\":\"waiting\"}' $$ > \"$1/$$.json\"\nread answer\n",
    );
    let (claude, sessions) = (claude.display().to_string(), sessions.display().to_string());
    let pane = app.session.says(&["new-tab", "--name", "helper", "--", &claude, &sessions]);
    let id: u64 = pane.parse().expect("a pane id");
    app.session.wait_for_report("claude waits", |report| {
        panes(report).any(|p| p.id == id && p.status.as_deref() == Some("waiting"))
    });

    let shell_refused = app.session.cli(&["close", "--agent", "--pane", &shell]);
    let check = app.session.says(&["close", "--agent", "--pane", &pane, "--check", "--json"]);
    let check: serde_json::Value = serde_json::from_str(&check).expect("the check");
    assert!(refused(&shell_refused, 1, "runs no agent cornercase reads"), "{shell_refused:?}");
    assert_eq!(
        (&check["closed"], &check["removes"], check["tabs"].as_array().map(Vec::len)),
        (&false.into(), &serde_json::Value::Null, Some(1)),
        "{check}"
    );
    app.wait_for("the agent's tab still shows", |s| s.contains("helper"));
    app.session.says(&["close", "--agent", "--pane", &pane]);

    app.wait_for("the agent's tab goes", |s| !s.contains("helper"));
    let _ = std::fs::remove_dir_all(&bin);
}

#[test]
fn skill_prints_the_instructions_for_agents() {
    let out = std::process::Command::new(bin()).arg("skill").output().expect("run cornercase");

    assert!(String::from_utf8_lossy(&out.stdout).starts_with("---\nname: cornercase\n"), "{out:?}");
}

#[rstest]
#[case::status(&["status"])]
#[case::several_last_messages(&["read", "--last-message", "--pane", "1", "--pane", "2"])]
#[case::usage(&["usage"])]
#[case::issue_list(&["issue", "list"])]
fn a_command_without_a_server_says_so_and_starts_none(#[case] args: &[&str]) {
    let session = Session::new();

    let out = session.cli(args);

    assert!(refused(&out, 1, "no cornercase server is running"), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");
    assert_eq!(session.servers(), 0);
}

#[test]
fn wrong_usage_exits_with_2() {
    let session = Session::new();

    let out = session.cli(&["send", "--wait", "hi"]);

    assert!(refused(&out, 2, "--enter"), "{out:?}");
}

#[test]
fn a_server_without_a_window_refuses_new_terminals() {
    let session = Session::new();
    let mut server = session.command().arg("server").spawn().expect("start a server");
    let deadline = Instant::now() + TIMEOUT;
    while std::os::unix::net::UnixStream::connect(session.socket()).is_err() {
        assert!(Instant::now() < deadline, "the server never listened");
        thread::sleep(POLL);
    }

    let out = session.cli(&["new-tab"]);

    assert!(refused(&out, 1, "cornercase start-server"), "{out:?}");
    assert_eq!(session.report().projects, []);
    drop(session);
    let _ = server.wait();
}

#[test]
fn stats_reads_the_local_file_without_starting_a_server() {
    let session = Session::new();
    let now = std::time::SystemTime::now();
    let mut stats = cornercase::agents::stats::Stats::new(now);
    stats.counted(
        cornercase::agents::Known::Claude,
        cornercase::agents::stats::Counted {
            prompts: vec![now],
            tokens: vec![(
                now,
                cornercase::agents::stats::Tokens { out: 321, ..cornercase::agents::stats::Tokens::default() },
            )],
        },
    );
    cornercase::agents::stats::save(&session.dir.join("stats.json"), &stats.saved(now)).expect("save stats");
    let text = session.says(&["stats"]);
    assert!(text.contains("prompts") && text.contains("321"), "{text}");
    let json: serde_json::Value = serde_json::from_str(&session.says(&["stats", "--json"])).expect("stats JSON");
    assert_eq!(json["windows"]["total"]["prompts"], 1);
    assert_eq!(json["windows"]["total"]["tokens"]["out"], 321);
    assert_eq!(session.says(&["stats", "--share"]), cornercase::agents::stats::share_link(&stats, now));
    assert!(!session.socket().exists());
}

#[test]
fn stats_asks_the_running_server_for_all_three_output_formats() {
    let mut app = Harness::start();
    app.wait_for("a shell prompt", |s| s.contains('$'));
    let text = app.session.says(&["stats"]);
    assert!(text.contains("no agent worked") && text.contains("most at once"), "{text}");
    let json: serde_json::Value = serde_json::from_str(&app.session.says(&["stats", "--json"])).expect("stats JSON");
    assert_eq!(json["windows"]["total"]["agents"], 0);
    assert_eq!(json["blocks"].as_array().expect("blocks").len(), 182);
    assert!(app.session.says(&["stats", "--share"]).starts_with("https://usecornercase.dev/stats/#v2&24h="));
    assert!(!app.session.dir.join("stats.json").exists());
}

fn windowless() -> (Arc<Session>, std::process::Child) {
    let session = Session::new();
    let server = session.command().arg("server").spawn().expect("start a server");
    let deadline = Instant::now() + TIMEOUT;
    while std::os::unix::net::UnixStream::connect(session.socket()).is_err() {
        assert!(Instant::now() < deadline, "the server never listened");
        thread::sleep(POLL);
    }
    (session, server)
}

#[test]
fn a_flow_is_exported_to_a_file_imported_under_another_name_and_listed() {
    let (session, mut server) = windowless();
    let file = session.dir.join("quick.toml");

    let exported = session.says(&["flow", "export", "Quick"]);
    std::fs::write(&file, &exported).expect("write the file");
    assert!(exported.starts_with("# A cornercase flow"), "{exported}");
    let said = session.says(&["flow", "import", &file.display().to_string(), "--name", "Quick mine"]);
    let listed = session.says(&["flow", "list"]);

    assert!(said.starts_with("imported Quick mine into yours"), "{said}");
    assert!(listed.contains("Quick mine (yours, "), "{listed}");
    let again = session.cli(&["flow", "import", &file.display().to_string(), "--name", "Quick mine"]);
    assert!(refused(&again, 1, "--replace"), "{again:?}");
    drop(session);
    let _ = server.wait();
}

#[test]
fn an_import_that_sets_variables_needs_yes_without_a_terminal() {
    let (session, mut server) = windowless();
    let exported = session.says(&["flow", "export", "Quick"]);
    let risky = exported.replacen("[orchestrator]\n", "[orchestrator]\nenv = { AWS_PROFILE = \"work\" }\n", 1);
    assert_ne!(risky, exported, "{exported}");
    let run = |args: &[&str]| session.fed(args, &risky);

    let refused_import = run(&["flow", "import", "-", "--name", "Risky"]);
    assert!(refused(&refused_import, 1, "pass --yes to import it"), "{refused_import:?}");
    assert!(!session.says(&["flow", "list"]).contains("Risky"));
    let accepted = run(&["flow", "import", "-", "--name", "Risky", "--yes"]);
    assert!(accepted.status.success(), "{accepted:?}");
    assert!(session.says(&["flow", "list"]).contains("Risky (yours"));
    drop(session);
    let _ = server.wait();
}

#[test]
fn a_flow_from_a_repo_is_fetched_and_always_asks_first() {
    let (session, mut server) = windowless();
    let exported = session.says(&["flow", "export", "Quick"]);
    let url =
        updates::serve(|_| vec![("/octocat/shared/HEAD/cornercase-flow.toml".to_string(), exported.into_bytes())]);
    let run = |args: &[&str]| {
        session
            .command()
            .env("CORNERCASE_FLOWS_URL", &url)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("run cornercase")
    };

    let refused_import = run(&["flow", "import", "octocat/shared", "--name", "From the web"]);
    assert!(refused(&refused_import, 1, "pass --yes to import it"), "{refused_import:?}");
    assert!(String::from_utf8_lossy(&refused_import.stderr).contains("fetching octocat/shared"), "{refused_import:?}");
    assert!(!session.says(&["flow", "list"]).contains("From the web"));
    let accepted = run(&["flow", "import", "octocat/shared", "--name", "From the web", "--yes"]);
    assert!(accepted.status.success(), "{accepted:?}");
    assert!(session.says(&["flow", "list"]).contains("From the web (yours"));
    let missing = run(&["flow", "import", "octocat/nothing", "--yes"]);
    assert!(refused(&missing, 1, "no flow.toml or cornercase-flow.toml in octocat/nothing"), "{missing:?}");
    drop(session);
    let _ = server.wait();
}

fn cells_of(frame: &serde_json::Value, pane: usize) -> Vec<String> {
    frame["panes"][pane]["lines"]
        .as_array()
        .expect("lines")
        .iter()
        .map(|line| {
            let runs = line.as_array().expect("runs").iter();
            runs.flat_map(|run| run["cells"].as_array().expect("cells").iter().filter_map(serde_json::Value::as_str))
                .collect()
        })
        .collect()
}

fn spelled_in_green(frame: &serde_json::Value, word: &str) -> bool {
    let green = |style: &serde_json::Value| {
        let style = &frame["styles"][usize::try_from(style.as_u64().expect("a style")).expect("an index")];
        style["bold"] == true && style["fg"]["index"] == 2
    };
    frame["panes"][0]["lines"].as_array().expect("lines").iter().any(|line| {
        line.as_array().expect("runs").iter().any(|run| {
            let text: String =
                run["cells"].as_array().expect("cells").iter().filter_map(serde_json::Value::as_str).collect();
            green(&run["style"]) && text.contains(word)
        })
    })
}

#[test]
fn a_tab_the_window_does_not_show_streams_its_cells_until_it_closes() {
    let app = Harness::start();
    let (pane, tab) = new_tab(&app);
    let shown = app.session.report().shown;
    let args = ["surface", "--tab", &tab, "--cols", "80", "--rows", "24"];
    let mut streaming = app.session.spawn(&args);
    let mut followed = Followed::of(&mut streaming);
    followed.until("the first frame", |frame| frame["revision"] == 1);

    app.session.says(&["send", "--pane", &pane, "--enter", "printf '\\033[1;32m%s\\033[0m\\n' surface-\"\"ok"]);
    followed.until("the green word", |frame| spelled_in_green(frame, "surface-ok"));
    let unchanged = app.session.report().shown == shown;
    let on_screen = app.text().contains("surface-ok");
    app.session.says(&["split", "--pane", &pane]);
    followed.until("two panes and a divider", |frame| {
        frame["panes"].as_array().is_some_and(|p| p.len() == 2)
            && frame["dividers"].as_array().is_some_and(|d| d.len() == 1)
    });
    let mut again = app.session.spawn(&args);
    let mut baseline = Followed::of(&mut again);
    baseline.until("a whole baseline", |frame| frame["revision"] == 1);
    let revisions: Vec<u64> = followed.seen.iter().filter_map(|frame| frame["revision"].as_u64()).collect();
    app.session.says(&["close", "--tab", &tab]);

    assert!(unchanged && !on_screen, "the window kept showing what it showed");
    assert!(revisions.windows(2).all(|pair| pair[1] == pair[0] + 1), "{revisions:?}");
    assert!(cells_of(&baseline.seen[0], 0).iter().any(|line| line.contains("surface-ok")));
    assert_eq!(finish_within(streaming, TIMEOUT).status.code(), Some(0));
    assert_eq!(finish_within(again, TIMEOUT).status.code(), Some(0));
}

fn typed(app: &Harness, pane: &str, line: &str) {
    let enter = r#"{"key":{"code":"enter"}}"#;
    app.session.says(&["input", "--pane", pane, &serde_json::json!({"text": line}).to_string(), enter]);
}

#[test]
fn a_hidden_tab_takes_input_selection_and_scrolling_while_the_window_shows_a_dialog() {
    let mut app = Harness::start();
    let pane = app.session.says(&["new-tab"]);
    app.pick_in_more("settings");
    app.wait_for("the dialog opens", |s| s.contains(" restart "));
    let shown = app.session.report().shown;

    typed(&app, &pane, "printf '\\033[2J\\033[Hinput-ok\\n'");
    app.session.says(&["wait", "--pane", &pane, "--text", "^input-ok$", "--timeout", "10"]);
    let selected = app.session.says(&["selection", "--pane", &pane, "--from", "0,0", "--to", "7,0"]);
    let read_back = app.session.says(&["selection", "--pane", &pane]);
    typed(&app, &pane, "seq 1 100");
    app.session.says(&["wait", "--pane", &pane, "--text", "^100$", "--timeout", "10"]);
    let up = app.session.says(&["scroll", "--pane", &pane, "--up", "5", "--json"]);
    let bottom = app.session.says(&["scroll", "--pane", &pane, "--bottom", "--json"]);

    let offset =
        |text: &str| serde_json::from_str::<serde_json::Value>(text).expect("json")["scroll"]["offset"].as_u64();
    assert!(app.text().contains(" restart "), "the dialog stayed");
    assert_eq!(app.session.report().shown, shown, "the window shows what it showed");
    assert_eq!((selected.as_str(), read_back.as_str()), ("input-ok", "input-ok"));
    assert_eq!(offset(&up).zip(offset(&bottom)).map(|(up, bottom)| bottom - up), Some(5), "{up} {bottom}");
}

#[test]
fn a_surface_resizes_in_place_and_a_divider_moves() {
    let app = Harness::start();
    let (pane, tab) = new_tab(&app);
    let mut streaming = app.session.spawn(&["surface", "--tab", &tab, "--cols", "80", "--rows", "24"]);
    let mut followed = Followed::of(&mut streaming);
    followed.until("the first frame", |frame| frame["revision"] == 1);
    let surface = followed.seen[0]["surface"].as_u64().expect("a surface id").to_string();

    app.session.says(&["resize", "--surface", &surface, "--cols", "100", "--rows", "30"]);
    followed.until("the larger frame", |frame| frame["cols"] == 100 && frame["rows"] == 30);
    let resized = followed.seen.last().expect("a frame")["revision"].as_u64().expect("a revision");
    app.session.says(&["split", "--pane", &pane]);
    followed.until("two panes", |frame| frame["panes"].as_array().is_some_and(|p| p.len() == 2));
    app.session.says(&["ratio", "--tab", &tab, "0.25"]);
    followed.until("a divider moved", |frame| {
        let width = |pane: usize| frame["panes"][pane]["rect"]["cols"].as_u64().unwrap_or(0);
        frame["cols"] == 100 && width(0) > 0 && width(0) * 2 < width(1)
    });
    app.session.says(&["close", "--tab", &tab]);

    assert_eq!(resized, 2, "the stream kept counting");
    assert_eq!(finish_within(streaming, TIMEOUT).status.code(), Some(0));
}

#[test]
fn a_surface_with_patches_sends_what_changed_and_redraw_a_complete_line() {
    let app = Harness::start();
    let (pane, tab) = new_tab(&app);
    let mut streaming = app.session.spawn(&["surface", "--tab", &tab, "--cols", "80", "--rows", "24", "--patches"]);
    let mut followed = Followed::of(&mut streaming);
    followed.until("the first frame", |frame| frame["revision"] == 1);
    let surface = followed.seen[0]["surface"].as_u64().expect("a surface id").to_string();
    assert!(followed.seen[0].get("base").is_none());

    app.session.says(&["send", "--pane", &pane, "--enter", "echo patch-\"\"ok"]);
    followed.until("a patch with the word", |frame| {
        frame.get("base").is_some()
            && frame["panes"][0]["lines"]
                .as_object()
                .is_some_and(|rows| rows.values().any(|row| row.to_string().contains(r#""o","k""#)))
    });
    let seen = followed.seen.len();
    app.session.says(&["redraw", "--surface", &surface]);
    followed.until("a complete line", |frame| frame.get("base").is_none() && frame["revision"].as_u64() > Some(1));
    app.session.says(&["close", "--tab", &tab]);

    assert!(followed.seen.len() > seen);
    assert_eq!(finish_within(streaming, TIMEOUT).status.code(), Some(0));
}

#[test]
fn link_says_which_file_a_click_on_a_printed_path_opens() {
    let app = Harness::start();
    let pane = app.session.says(&["new-tab"]);
    typed(&app, &pane, "printf 'a\\nb\\nc\\n' > note.md; printf '\\033[H\\033[2Jsee note.md:2\\n'");
    app.session.says(&["wait", "--pane", &pane, "--text", "^see note.md:2$", "--timeout", "10"]);

    let text = app.session.says(&["link", "--pane", &pane, "--cell", "4,0"]);
    let json = app.session.says(&["link", "--pane", &pane, "--cell", "4,0", "--json"]);
    let nothing = app.session.says(&["link", "--pane", &pane, "--cell", "0,0"]);

    let link = serde_json::from_str::<serde_json::Value>(&json).expect("json")["link"].clone();
    assert!(text.ends_with("/note.md:2"), "{text}");
    assert_eq!(
        (&link["inside"], &link["relative"], &link["lines"]["first"]),
        (&true.into(), &"note.md".into(), &2.into())
    );
    assert_eq!(nothing, "");
}
