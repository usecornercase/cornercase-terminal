use super::*;

const SSH_PID: &str = "ssh.pid";
const SSH_RUNS: &str = "ssh.runs";

pub(super) fn fake_ssh(dir: &std::path::Path, prefix: &str) -> String {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).expect("create the fake ssh's folder");
    let pid = dir.join(SSH_PID);
    write_executable(
        &bin.join("ssh"),
        &format!(
            "#!/bin/sh\necho $$ > '{}'\necho $$ >> '{}'\nwhile [ $# -gt 0 ]; do\n  case \"$1\" in\n    -o) shift 2 ;;\n    --) shift; break ;;\n    \
             -*) shift ;;\n    *) break ;;\n  esac\ndone\nshift\nexec /bin/sh -c \"exec {prefix} $*\"\n",
            pid.display(),
            dir.join(SSH_RUNS).display()
        ),
    );
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default())
}

fn ssh_pid(session: &Session) -> String {
    std::fs::read_to_string(session.dir.join(SSH_PID)).unwrap_or_default().trim().to_string()
}

fn ssh_runs(session: &Session) -> usize {
    std::fs::read_to_string(session.dir.join(SSH_RUNS)).unwrap_or_default().lines().count()
}

fn remote_refusal(args: &[&str], command: &str) -> std::process::Output {
    let session = Session::new();
    let path = fake_ssh(&session.dir, "");
    session.command().args(args).args([command]).env("PATH", path).output().expect("run cornercase")
}

pub(super) fn remotely(session: &Session, args: &[&str]) -> std::process::Command {
    let mut cmd = session.cold_command();
    cmd.env("PATH", fake_ssh(&session.dir, "")).args(["--remote", "devbox", "--remote-command", bin()]).args(args);
    cmd
}

fn remote_says(session: &Session, args: &[&str]) -> String {
    let out = remotely(session, args).output().expect("run cornercase");
    assert!(out.status.success(), "cornercase --remote {args:?} failed: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn gone(pid: &str) -> bool {
    let ps = std::process::Command::new("ps").args(["-o", "stat=", "-p", pid]).output().expect("run ps");
    let state = String::from_utf8_lossy(&ps.stdout);
    state.trim().is_empty() || state.trim().starts_with('Z')
}

fn boot_of(json: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(json).expect("json")["boot"].clone()
}

#[test]
fn a_remote_window_comes_back_after_the_connection_drops() {
    let session = Session::new();
    let path = fake_ssh(&session.dir, "");
    let exe = env!("CARGO_BIN_EXE_cornercase");
    let args = ["remote", "devbox", "--command", exe];
    let mut app = Harness::run(exe.as_ref(), &args, Arc::clone(&session), (ROWS, COLS), &[("PATH", &path)]);
    app.wait_for("the remote window shows its shell", |s| s.contains(&first_entry()));
    app.send(b"kept=yes; echo set-\"\"done\r");
    app.wait_for("the shell ran it", |s| s.contains("set-done"));
    let first = ssh_pid(&session);

    let killed = std::process::Command::new("kill").args(["-9", &first]).status().expect("run kill");

    assert!(killed.success(), "kill failed");
    app.wait_for_raw("the window says it reconnects", |raw| raw.contains("reconnecting to devbox"));
    app.wait_for("the window comes back", |s| !s.contains("reconnecting") && s.contains(&first_entry()));
    assert_ne!(ssh_pid(&session), first, "a new ssh");
    app.send(b"echo \"$kept\"-still\r");
    app.wait_for("the same shell answers", |s| s.contains("yes-still"));
    assert_eq!(session.servers(), 1);
}

#[rstest]
#[case::window("window", &["remote", "devbox", "--command"], "Run `cornercase update` there")]
#[case::commands("commands", &["--remote", "devbox", "status", "--remote-command"], "Update it from here with `cornercase --remote devbox")]
fn an_older_remote_says_how_to_update_it(#[case] name: &str, #[case] args: &[&str], #[case] hint: &str) {
    let dir = temp_dir(&format!("old-remote-{name}"));
    let old = dir.join("cornercase");
    write_executable(&old, "#!/bin/sh\necho 'cornercase-proxy 1 0.0.1'\ncat > /dev/null\n");

    let out = remote_refusal(args, &old.display().to_string());

    let _ = std::fs::remove_dir_all(&dir);
    assert!(refused(&out, 1, "`devbox` runs cornercase 0.0.1"), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains(hint), "{out:?}");
}

#[rstest]
#[case::window(&["remote", "devbox", "--command"], "give its path with --command")]
#[case::commands(&["--remote", "devbox", "status", "--remote-command"], "give its path with --remote-command")]
#[case::kill_server(&["--remote", "devbox", "kill-server", "--remote-command"], "give its path with --remote-command")]
fn a_remote_without_cornercase_says_how_to_point_at_it(#[case] args: &[&str], #[case] hint: &str) {
    let out = remote_refusal(args, "/nonexistent/cornercase");

    assert!(refused(&out, 1, "cornercase was not found on `devbox`"), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains(hint), "{out:?}");
}

#[test]
fn commands_reach_the_server_on_the_other_machine() {
    let session = Session::new();

    let started = remote_says(&session, &["start-server", "--json"]);

    assert!(started.contains("\"already\": false") || started.contains("\"already\":false"), "{started}");
    let there = remote_says(&session, &["status", "--json"]);
    assert_eq!(boot_of(&there), boot_of(&session.says(&["status", "--json"])));
    assert_eq!(session.servers(), 1);
}

#[test]
fn a_command_without_a_server_there_says_so_and_starts_none() {
    let session = Session::new();

    let out = remotely(&session, &["status"]).env("RUST_BACKTRACE", "1").output().expect("run cornercase");

    assert!(refused(&out, 1, "no cornercase server is running on `devbox`"), "{out:?}");
    assert_eq!(session.servers(), 0);
}

#[test]
fn text_reaches_the_other_machine_as_it_is() {
    let session = Session::new();
    remote_says(&session, &["start-server"]);
    let pane = remote_says(&session, &["new-tab"]);
    let text = r#"it's "quoted" \ and $HOME"#;
    let out = feed(remotely(&session, &["send", "--pane", &pane, "-"]), text);

    assert!(out.status.success(), "{out:?}");
    wait_until("the text shows up", || session.says(&["read", "--pane", &pane]).contains(text));
}

#[test]
fn a_file_of_this_machine_reaches_a_pane_there() {
    let session = Session::new();
    remote_says(&session, &["start-server"]);
    let pane = remote_says(&session, &["new-tab"]);
    let file = session.dir.join("shot.txt");
    std::fs::write(&file, "from the laptop").expect("write the file");

    let path = remote_says(&session, &["paste-file", "--pane", &pane, &file.display().to_string()]);
    let piped =
        feed(remotely(&session, &["paste-file", "--pane", &pane, "--name", "notes.txt", "--json"]), "from stdin");

    assert!(path.starts_with(&session.dir.join("server.pasted").display().to_string()), "{path}");
    assert!(path.ends_with("/shot.txt"), "{path}");
    assert_eq!(std::fs::read_to_string(&path).expect("the kept file"), "from the laptop");
    let answer: serde_json::Value = serde_json::from_slice(&piped.stdout).expect("json");
    let kept = answer["path"].as_str().expect("path");
    assert!(kept.ends_with("/notes.txt"), "{answer}");
    assert_eq!(
        (std::fs::read_to_string(kept).expect("stdin's file"), answer["bytes"].as_u64()),
        ("from stdin".into(), Some(10))
    );
}

#[test]
fn a_stream_from_the_other_machine_ends_there_when_it_ends_here() {
    let session = Session::new();
    remote_says(&session, &["start-server"]);
    let mut following = remotely(&session, &["status", "--follow"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("run cornercase");
    let stdout = following.stdout.take().expect("stdout");
    let (first, line) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let mut text = String::new();
        let _ = std::io::BufRead::read_line(&mut std::io::BufReader::new(stdout), &mut text);
        let _ = first.send(text);
    });
    assert_ne!(line.recv_timeout(TIMEOUT).expect("the stream starts"), "");
    let proxy = ssh_pid(&session);

    following.kill().expect("kill the command");
    let _ = following.wait();

    wait_until("the proxy there ends", || gone(&proxy));
}

#[test]
fn input_follow_types_through_one_ssh_connection() {
    let session = Session::new();
    remote_says(&session, &["start-server"]);
    let pane = remote_says(&session, &["new-tab"]);
    let mut typist = input::Typist::start(remotely(&session, &["input", "--pane", &pane, "--follow", "--json", "-"]));

    typist.types(&serde_json::json!({"text": "echo remote-\"\"follow"}));
    let proxy = ssh_pid(&session);
    typist.types(&serde_json::json!({"key": {"code": "enter"}}));
    wait_until("the text is typed", || session.says(&["read", "--pane", &pane]).contains("remote-follow\n"));
    let ended = typist.ends();

    assert_eq!((ssh_pid(&session), ended.status.code()), (proxy.clone(), Some(0)), "{ended:?}");
    wait_until("the proxy there ends", || gone(&proxy));
}

#[test]
fn a_remote_follow_ends_when_its_reader_leaves() {
    let session = Session::new();
    remote_says(&session, &["start-server"]);
    let mut following = remotely(&session, &["status", "--follow"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run cornercase");
    let mut out = std::io::BufReader::new(following.stdout.take().expect("stdout"));
    std::io::BufRead::read_line(&mut out, &mut String::new()).expect("the first snapshot");
    let proxy = ssh_pid(&session);

    drop(out);
    let ended = finish_within(following, TIMEOUT);

    assert_eq!((ended.status.code(), ended.stderr.as_slice()), (Some(0), &b""[..]), "{ended:?}");
    wait_until("the proxy there ends", || gone(&proxy));
}

#[test]
fn cornercase_remote_reaches_it_too_but_stays_out_of_its_panes() {
    let session = Session::new();
    let started = session
        .cold_command()
        .env("PATH", fake_ssh(&session.dir, ""))
        .env(REMOTE_ENV, "devbox")
        .args(["--remote-command", bin(), "start-server"])
        .output()
        .expect("run cornercase");
    assert!(started.status.success(), "{started:?}");

    let pane = session.says(&["new-tab", "--", "echo remote-$((1+1))-${CORNERCASE_REMOTE-unset}"]);

    wait_until("the pane ran it", || session.says(&["read", "--pane", &pane]).contains("remote-2-"));
    assert!(session.says(&["read", "--pane", &pane]).contains("remote-2-unset"));
}

#[rstest]
#[case::the_flag(&["--remote", "devbox", "plugin", "install", "x"], None, "plugin")]
#[case::the_variable(&["plugin", "install", "x"], Some("devbox"), "plugin")]
fn commands_of_this_machine_refuse_remote(#[case] args: &[&str], #[case] var: Option<&str>, #[case] name: &str) {
    let session = Session::new();
    let mut cmd = session.cold_command();
    cmd.env("PATH", fake_ssh(&session.dir, "")).args(args);
    if let Some(var) = var {
        cmd.env(REMOTE_ENV, var);
    }

    let out = cmd.output().expect("run cornercase");

    assert!(refused(&out, 2, &format!("`{name}` acts on this machine")), "{out:?}");
    assert_eq!(ssh_pid(&session), "", "ssh never ran");
}

#[test]
fn the_files_of_the_other_machine_are_listed_and_read_by_absolute_path() {
    let session = Session::new();
    remote_says(&session, &["start-server"]);
    let dir = temp_dir("remote-files");
    std::fs::create_dir(dir.join("sub")).expect("create a folder");
    std::fs::write(dir.join("hello.txt"), "hi there\n").expect("write a file");
    let folder = dir.display().to_string();

    let listed = remote_says(&session, &["files", "list", &folder, "--json"]);
    let read = remote_says(&session, &["files", "read", &format!("{folder}/hello.txt")]);

    let listing: serde_json::Value = serde_json::from_str(&listed).expect("json");
    let entries: Vec<&str> = listing["folders"][0]["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|e| e["name"].as_str())
        .collect();
    assert_eq!(entries, ["sub", "hello.txt"], "{listed}");
    assert_eq!(read, "hi there");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_newer_remote_answers_commands_but_not_the_window() {
    let session = Session::new();
    remote_says(&session, &["start-server"]);
    let wrapper = session.dir.join("newer");
    write_executable(
        &wrapper,
        &format!(
            "#!/bin/sh\necho 'cornercase-proxy 99 99.0.0'\n'{}' \"$@\" | {{ IFS= read -r _; exec cat; }}\n",
            bin()
        ),
    );
    let wrapper = wrapper.display().to_string();

    let status = session
        .cold_command()
        .env("PATH", fake_ssh(&session.dir, ""))
        .args(["--remote", "devbox"])
        .args(["--remote-command", &wrapper, "status", "--json"])
        .output()
        .expect("run cornercase");
    let window = remote_refusal(&["remote", "devbox", "--command"], &wrapper);

    assert!(status.status.success(), "{status:?}");
    assert_eq!(boot_of(&String::from_utf8_lossy(&status.stdout)), boot_of(&session.says(&["status", "--json"])));
    assert!(refused(&window, 1, "`devbox` runs cornercase 99.0.0"), "{window:?}");
    assert!(String::from_utf8_lossy(&window.stderr).contains("Run `cornercase update` here"), "{window:?}");
}

#[test]
fn the_log_of_the_other_machine_is_where_its_logs_command_says() {
    let session = Session::new();
    remote_says(&session, &["start-server"]);

    let there = remote_says(&session, &["logs", "--path"]);

    assert_eq!(there, session.says(&["logs", "--path"]));
}

#[test]
fn the_other_machine_is_stopped_through_its_own_command() {
    let session = Session::new();
    remote_says(&session, &["start-server"]);
    assert_eq!(session.servers(), 1);

    remote_says(&session, &["kill-server", "--yes"]);

    wait_until("the server stopped", || session.servers() == 0);
}

#[test]
fn the_status_of_the_command_there_is_the_status_here() {
    let session = Session::new();

    let out = remotely(&session, &["logs"]).output().expect("run cornercase");

    assert!(refused(&out, 1, "no log yet"), "{out:?}");
}

#[test]
fn killing_a_remote_restart_when_idle_calls_it_off_there() {
    let mut app = Harness::start();
    let agent = updates::busy_claude(&app.session);
    app.session.says(&["new-tab", "--", &agent.display().to_string()]);
    app.session.wait_for_working();
    let mut waiting = remotely(&app.session, &["restart", "--when-idle", "--yes"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("run cornercase");
    app.wait_for("the window says a restart is pending", |s| s.contains("restart pending until 1 agent ends its turn"));
    let there = ssh_pid(&app.session);

    waiting.kill().expect("kill the command");
    let _ = waiting.wait();

    app.wait_for("the restart is called off", |s| !s.contains("restart pending"));
    wait_until("the command there and its ssh end", || gone(&there));
}

#[test]
fn a_command_that_checks_the_server_first_opens_one_ssh_connection() {
    let session = Session::new();
    remote_says(&session, &["start-server"]);
    let created: serde_json::Value = serde_json::from_str(&session.says(&["new-tab", "--json"])).expect("json");
    let tab = created["tab"].to_string();
    let (mut opened, _followed, view) = views::opened(&session, &tab);
    let runs = |args: &[&str]| {
        let before = ssh_runs(&session);
        remote_says(&session, args);
        ssh_runs(&session) - before
    };

    let focused = runs(&["focus", "--tab", &tab, "--view", &view]);
    let hidden = runs(&["view-set", "--view", &view, "--hidden"]);
    let _ = opened.kill();
    let _ = opened.wait();

    assert_eq!((focused, hidden), (1, 1));
}
