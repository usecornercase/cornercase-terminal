use super::*;

fn columns() -> u16 {
    areas().pane.x
}

#[test]
fn a_flooding_pane_can_be_interrupted_and_requests_keep_answering() {
    let mut app = Harness::start();
    app.send(b"yes 'flood flood flood flood'\r");
    app.wait_for("the pane floods", |s| s.matches("flood flood flood flood").count() > 10);

    let keys = app.session.spawn(&["keys", "ctrl+c"]);
    let interrupted = finish_within(keys, Duration::from_secs(5));
    assert!(interrupted.status.success(), "interrupting the flood failed: {interrupted:?}");
    let status = finish_within(app.session.spawn(&["status", "--json"]), Duration::from_secs(5));
    assert!(status.status.success(), "status after the flood failed: {status:?}");
    app.send(b"echo recovered-\"\"after-flood\r");

    app.wait_for("the shell answers after the flood", |s| s.contains("recovered-after-flood"));
}

#[test]
fn the_server_log_tells_what_happened_but_never_what_was_typed() {
    let session = Session::new();
    let mut app = Harness::open_with(Arc::clone(&session), ROWS, COLS, &[("CORNERCASE_LOG", "debug")]);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));

    app.send(b"echo typed-\"\"marker-$((40+1))\r");
    app.wait_for("the typed line ran", |s| s.contains("typed-marker-41"));
    app.send(b"\x1b[200~echo pasted-\"\"marker-$((40+2))\x1b[201~\r");
    app.wait_for("the pasted line ran", |s| s.contains("pasted-marker-42"));
    session.says(&["send", "--enter", "echo sent-\"\"marker-$((40+3))"]);
    app.wait_for("the sent line ran", |s| s.contains("sent-marker-43"));
    session.wait_for_file("server.log", "the log has the answer to send", |log| log.contains("command=send"));

    let log = session.says(&["logs", "-n", "1000"]);
    for told in ["server: started", "server: client attached", "app: pane opened", "control: send", "server: input"] {
        assert!(log.contains(told), "the log lacks {told:?}:\n{log}");
    }
    for secret in ["typed-", "pasted-", "sent-", "marker"] {
        assert!(!log.contains(secret), "the log holds {secret:?}:\n{log}");
    }
}

#[test]
fn quit_button_detaches_and_programs_keep_running() {
    let mut app = Harness::start();
    app.send(b"echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs", |s| s.contains("cat-starts"));

    app.quit();
    app.wait_exit("client exits");
    let mut again = app.attach(ROWS, COLS);
    again.send(b"\x02\r");

    again.wait_for("cat still echoes", |s| s.contains("cat-starts") && s.contains("^B"));
}

#[test]
fn clients_mirror_each_other() {
    let mut first = Harness::start();
    let mut second = first.attach(ROWS, COLS);

    second.send(b"echo from-\"\"second\r");

    first.wait_for("first client sees it", |s| s.contains("from-second"));
}

#[test]
fn the_last_used_client_sets_the_size() {
    let mut big = Harness::start();
    let mut small = big.attach(ROWS - 6, COLS - 8);

    small.send(b"stty size\r");
    small.wait_for("pane fits the small client", |s| s.contains(&format!("{} {}", ROWS - 6, COLS - 8 - columns())));
    big.send(b"stty size\r");

    big.wait_for("pane fits the big client", |s| s.contains(&format!("{ROWS} {}", COLS - columns())));
}

#[test]
fn a_hung_small_client_does_not_shrink_a_new_big_one() {
    let first = Harness::start();
    let mut small = first.attach(ROWS - 6, COLS - 8);
    small.send(b"stty size\r");
    small.wait_for("pane fits the small client", |s| s.contains(&format!("{} {}", ROWS - 6, COLS - 8 - columns())));
    small.stop();
    drop(first);

    let mut big = Harness::open(Arc::clone(&small.session), ROWS, COLS);
    big.wait_for("client shows the sidebar", |s| s.contains("Projects"));
    big.send(b"stty size\r");

    big.wait_for("pane fits the new big client", |s| s.contains(&format!("{ROWS} {}", COLS - columns())));
}

#[test]
fn a_smaller_client_sees_the_frame_cut_off() {
    let mut big = Harness::start();
    let small = big.attach(ROWS - 6, COLS - 8);
    let line = format!("{}>", "=".repeat(usize::from(COLS - columns() - 1)));

    big.send(format!("clear; printf '%s\\n' '{line}'\r").as_bytes());
    big.wait_for("the line fills the big pane", |s| s.lines().any(|l| l.ends_with(&line)));

    let cut = |h: &Harness| h.screen.lock().screen().rows(0, COLS - 8).take(usize::from(ROWS - 6)).collect::<Vec<_>>();
    let deadline = Instant::now() + TIMEOUT;
    while cut(&small) != cut(&big) {
        assert!(Instant::now() < deadline, "the small client does not show the cut frame\n{}", small.text());
        thread::sleep(POLL);
    }
}

#[test]
fn quitting_one_client_leaves_the_others_attached() {
    let mut first = Harness::start();
    let mut second = first.attach(ROWS, COLS);

    second.quit();
    second.wait_exit("second client exits");
    first.send(b"echo still-\"\"here\r");

    first.wait_for("first client keeps working", |s| s.contains("still-here"));
}

#[test]
fn kill_server_says_what_it_stopped() {
    let mut app = Harness::start();
    app.send(b"sleep 600\r");
    app.session.wait_for_program("sleep");

    let out = app.session.run("kill-server");

    assert!(String::from_utf8_lossy(&out.stdout).contains("stopped 1 program"), "{out:?}");
}

#[test]
fn a_window_that_closes_puts_the_pointer_back() {
    let mut app = Harness::start();
    let border = areas().border(ui::Border::Projects);
    let (x, y) = (border.x + 1, border.y + 3);
    app.send(format!("\x1b[<35;{x};{y}M").as_bytes());
    app.wait_for_raw("the pointer turns sideways", |raw| raw.contains("\x1b]22;ew-resize\x1b\\"));

    let out = app.session.run("kill-server");
    assert!(out.status.success(), "kill-server failed: {out:?}");
    app.wait_exit("the window closes");

    app.wait_for_raw("the window puts the pointer back", |raw| {
        raw.rsplit_once("ew-resize").is_some_and(|(_, after)| after.contains("\x1b]22;text\x1b\\"))
    });
}

#[test]
fn kill_server_closes_every_client() {
    let mut first = Harness::start();
    let mut second = first.attach(ROWS, COLS);

    let out = first.session.run("kill-server");

    assert!(out.status.success(), "kill-server failed: {out:?}");
    first.wait_exit("first client exits");
    second.wait_exit("second client exits");
}

#[test]
fn clients_starting_at_once_share_one_server() {
    let session = Session::new();
    let (mut first, mut second) = thread::scope(|s| {
        let first = s.spawn(|| Harness::open(Arc::clone(&session), ROWS, COLS));
        let second = Harness::open(Arc::clone(&session), ROWS, COLS);
        (first.join().expect("first client"), second)
    });
    first.wait_for("first client has a terminal", |s| s.contains(&first_entry()));
    second.wait_for("second client has a terminal", |s| s.contains(&first_entry()));

    second.send(b"echo from-\"\"second\r");

    first.wait_for("first client sees it", |s| s.contains("from-second"));
    let entries = first.text().matches(&first_entry()).count();
    assert_eq!(entries, 1, "two servers opened a terminal each:\n{}", first.text());
    let deadline = Instant::now() + TIMEOUT;
    while session.servers() != 1 {
        assert!(Instant::now() < deadline, "{} servers are running for one socket", session.servers());
        thread::sleep(POLL);
    }
}

#[test]
fn a_new_server_reopens_the_saved_projects() {
    let mut app = Harness::start();
    let name = format!("ccrs-{}", std::process::id());
    let dir = temp_dir_named(&name);
    app.open_project(1, &dir);
    app.wait_for("project 2 appears", |s| s.contains(&entry(&name)));
    app.session.wait_for_saved("both projects are saved", |saved| saved.contains(name.as_str()));

    let out = app.session.run("kill-server");
    assert!(out.status.success(), "kill-server failed: {out:?}");
    app.wait_exit("client exits with the server");
    let mut again = app.attach(ROWS, COLS);

    again.wait_for("both projects come back", |s| s.contains(&first_entry()) && s.contains(&entry(&name)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn kill_server_says_when_none_is_running() {
    let session = Session::new();

    let out = session.run("kill-server");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && stderr.contains("no cornercase server is running"), "{out:?}");
}

#[test]
fn running_cornercase_inside_itself_is_refused() {
    let mut app = Harness::start();

    let bin = env!("CARGO_BIN_EXE_cornercase");

    app.send(format!("{bin} 2>&1 | grep -q 'nesting it is not supported' && echo refused-\"\"ok\r").as_bytes());
    app.wait_for("nested client explains why", |s| s.contains("refused-ok"));
    app.send(format!("{bin} 2>/dev/null; echo exit-\"\"code-$?\r").as_bytes());

    app.wait_for("nested client fails", |s| s.contains("exit-code-1"));
}

#[test]
fn shells_do_not_inherit_the_agent_session_that_started_the_server() {
    let keys: Vec<&str> = CLAUDE_SESSION_ENV.into_iter().chain(cornercase::agents::copilot::SESSION_ENV).collect();
    let mut env: Vec<(&str, &str)> = keys.iter().map(|key| (*key, "leaked")).collect();
    env.push(("CORNERCASE_E2E_KEPT", "kept"));
    let mut app = Harness::open_with(Session::new(), ROWS, COLS, &env);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));

    let vars = keys.iter().map(|key| format!("${{{key}}}")).collect::<Vec<_>>().concat();
    app.send(format!("echo \"session-[{vars}]-$CORNERCASE_E2E_KEPT\"\r").as_bytes());

    app.wait_for("only the unrelated variable reaches the shell", |s| s.contains("session-[]-kept"));
}

#[test]
fn sigterm_restores_the_terminal() {
    let mut app = Harness::start();
    let pid = app.child.process_id().expect("child pid");
    app.raw.lock().clear();

    let status = std::process::Command::new("kill").args(["-TERM", &pid.to_string()]).status().expect("run kill");
    assert!(status.success(), "kill failed: {status}");

    app.wait_exit("app exits on SIGTERM");
    app.wait_for_raw("kitty keys and mouse capture off, alternate screen left", |raw| {
        raw.contains("\x1b[<1u") && raw.contains("\x1b[?1000l") && raw.contains("\x1b[?1049l")
    });
}

#[test]
fn a_socket_folder_other_users_can_write_is_refused() {
    let session = Session::new();
    std::fs::set_permissions(&session.dir, std::fs::Permissions::from_mode(0o777)).expect("open the folder to all");

    let outputs = [Some("server"), Some("kill-server"), None].map(|arg| {
        let mut cmd = session.command();
        cmd.args(arg).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
        finish_within(cmd.spawn().expect("run cornercase"), TIMEOUT)
    });

    let refused = outputs.iter().map(|out| refused_with(out, KEEP_THE_FOLDER));
    assert_eq!(refused.collect::<Vec<_>>(), [true, true, true]);
}

#[test]
fn a_bare_socket_name_is_checked_in_the_folder_it_runs_in() {
    let session = Session::new();
    std::fs::set_permissions(&session.dir, std::fs::Permissions::from_mode(0o777)).expect("open the folder to all");
    let mut cmd = session.command();
    cmd.arg("kill-server").env(SOCKET_ENV, "server.sock").current_dir(&session.dir);

    let out = cmd.output().expect("run cornercase");

    assert!(refused_with(&out, KEEP_THE_FOLDER), "{out:?}");
}

#[test]
fn a_debug_build_keeps_to_the_folders_of_its_source_tree() {
    let session = Session::new();
    let home = PathBuf::from(format!("/tmp/cch-{}-home", std::process::id()));
    let run = session.dir.join("run");
    let own = |args: &[&str]| {
        let mut cmd = session.command();
        cmd.args(args)
            .env_remove(SOCKET_ENV)
            .env_remove("XDG_STATE_HOME")
            .env_remove("XDG_CONFIG_HOME")
            .env("HOME", &home)
            .env("XDG_RUNTIME_DIR", &run);
        cmd.output().expect("run cornercase")
    };

    let logs = own(&["logs", "--path"]);
    let status = own(&["status"]);

    let expected = home.join(".local/state").join(own_folder()).join("server.log");
    assert_eq!(String::from_utf8_lossy(&logs.stdout).trim(), expected.to_string_lossy());
    assert!(!status.status.success(), "{status:?}");
    assert!(String::from_utf8_lossy(&status.stderr).contains("no cornercase server is running"), "{status:?}");
    assert!(home.join(".local/state").join(own_folder()).is_dir());
    let _ = std::fs::remove_dir_all(&home);
}

const KEEP_THE_FOLDER: &str = "Point CORNERCASE_SOCKET at a socket in a folder only you can write to";

fn refused_with(out: &std::process::Output, advice: &str) -> bool {
    let stderr = String::from_utf8_lossy(&out.stderr);
    let removal = stderr.to_lowercase().contains("remove");
    !out.status.success() && stderr.contains("can be written by other users") && stderr.contains(advice) && !removal
}

fn pane_kill_server(app: &mut Harness, args: &str, marker: &str) {
    let exe = env!("CARGO_BIN_EXE_cornercase");
    app.send(format!("\"{exe}\" kill-server {args}; echo {marker}-\"\"$?\r").as_bytes());
}

#[test]
fn kill_server_inside_its_own_terminal_refuses_without_a_terminal_or_yes() {
    let mut app = Harness::start();
    app.send(b"echo ready-\"\"here\r");
    app.wait_for("the shell runs", |s| s.contains("ready-here"));

    pane_kill_server(&mut app, "</dev/null", "refused");

    app.wait_for("kill-server refuses", |s| s.contains("refused-1") && s.contains("--yes"));
    app.send(b"echo alive-\"\"still\r");
    app.wait_for("the server and the shell are still there", |s| s.contains("alive-still"));
}

#[test]
fn kill_server_inside_its_own_terminal_asks_and_a_no_keeps_the_server() {
    let mut app = Harness::start();

    pane_kill_server(&mut app, "", "answered");
    app.wait_for("it asks", |s| s.contains("Stop it now? [y/N]"));
    app.send(b"n\r");

    app.wait_for("the server keeps running", |s| s.contains("the server keeps running") && s.contains("answered-0"));
    assert!(app.session.report().caller.is_none());
    app.send(b"echo alive-\"\"still\r");
    app.wait_for("the shell still works", |s| s.contains("alive-still"));
}

#[test]
fn kill_server_inside_its_own_terminal_stops_with_yes() {
    let mut app = Harness::start();

    pane_kill_server(&mut app, "--yes", "gone");

    app.wait_exit("the window closes with the server");
}
