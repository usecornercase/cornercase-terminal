use super::*;
use cornercase::version;

fn serve(files: impl FnOnce(&str) -> Vec<(String, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a port");
    let url = format!("http://{}", listener.local_addr().expect("its address"));
    let files = files(&url);
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            while !request.ends_with(b"\r\n\r\n") {
                let Ok(n @ 1..) = stream.read(&mut buf) else { break };
                request.extend_from_slice(&buf[..n]);
            }
            let line = String::from_utf8_lossy(&request).into_owned();
            let path = line.split_whitespace().nth(1).unwrap_or_default();
            let found = files.iter().find(|(p, _)| p == path).map(|(_, body)| body.as_slice());
            let (status, body) = found.map_or(("404 Not Found", [].as_slice()), |body| ("200 OK", body));
            let head = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = stream.write_all(head.as_bytes()).and_then(|()| stream.write_all(body));
        }
    });
    url
}

fn fake_release(version: &str, script: &str) -> String {
    let target = update::target().expect("a released platform");
    let name = format!("cornercase-{target}");
    let build = temp_dir(&format!("release-{version}"));
    std::fs::create_dir_all(build.join(&name)).expect("archive folder");
    write_executable(&build.join(&name).join("cornercase"), script);
    let tar = std::process::Command::new("tar")
        .current_dir(&build)
        .args(["-czf", "release.tar.gz", &name])
        .status()
        .expect("run tar");
    assert!(tar.success(), "tar failed");
    let archive = std::fs::read(build.join("release.tar.gz")).expect("read the archive");
    let _ = std::fs::remove_dir_all(&build);
    let digest = Sha256::digest(&archive).iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    });
    let url = serve(|url| {
        let latest = serde_json::json!({"tag_name": format!("v{version}"), "assets": [
            {"name": format!("{name}.tar.gz"), "browser_download_url": format!("{url}/download")},
            {"name": format!("{name}.tar.gz.sha256"), "browser_download_url": format!("{url}/download.sha256")},
        ]});
        vec![
            ("/latest".to_string(), latest.to_string().into_bytes()),
            ("/download".to_string(), archive),
            ("/download.sha256".to_string(), format!("{digest} *{name}.tar.gz\n").into_bytes()),
        ]
    });
    format!("{url}/latest")
}

fn installed_copy(name: &str) -> PathBuf {
    let bin = temp_dir(name).join("cornercase");
    std::fs::copy(env!("CARGO_BIN_EXE_cornercase"), &bin).expect("copy cornercase");
    bin
}

#[test]
fn restart_says_what_stops_and_brings_the_client_back_with_new_shells() {
    let mut app = Harness::start();
    app.send(b"echo old-\"\"shell\r");
    app.wait_for("the old shell answers", |s| s.contains("old-shell"));
    app.send(b"sleep 600\r");
    app.session.wait_for_program("sleep");

    let out = app.session.cli(&["restart", "--yes"]);

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    assert!(stdout.contains("restarted the cornercase server") && stdout.contains("stopped 1 program"), "{stdout}");
    app.wait_for("the client comes back with a new shell", |s| !s.contains("old-shell") && s.contains(&first_entry()));
}

#[test]
fn a_restart_keeps_the_window_on_the_alternate_screen() {
    let bin = env!("CARGO_BIN_EXE_cornercase");
    let shell = std::path::Path::new("/bin/sh");
    let mut app =
        Harness::run(shell, &["-c", "\"$0\"; stty -a; echo stty-\"\"done", bin], Session::new(), (ROWS, COLS), &[]);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    app.send(b"echo old-\"\"shell\r");
    app.wait_for("the old shell answers", |s| s.contains("old-shell"));
    app.raw.lock().clear();

    app.session.says(&["restart", "--yes"]);

    app.wait_for("the client comes back with a new shell", |s| !s.contains("old-shell") && s.contains(&first_entry()));
    let raw = String::from_utf8_lossy(&app.raw.lock()).into_owned();
    assert!(!raw.contains("\x1b[?1049l"), "the restart left the alternate screen: {raw:?}");
    assert!(!raw.contains("\x1b[?1049h"), "the restart entered the alternate screen again: {raw:?}");
    assert_eq!(raw.matches("\x1b[<1u").count(), 1, "the keyboard flags are not popped once: {raw:?}");
    assert_eq!(raw.matches("\x1b[>1u").count(), 1, "the keyboard flags are not pushed once: {raw:?}");
    app.send(b"echo restarted=[$CORNERCASE_RESTARTED]\r");
    app.wait_for("the new shell does not inherit the restart mark", |s| s.contains("restarted=[]"));

    app.click(areas().quit.as_position());
    app.wait_for_raw("the shell reads the terminal's modes after the client", |raw| raw.contains("stty-done"));
    let raw = String::from_utf8_lossy(&app.raw.lock()).into_owned();
    let modes = raw.rsplit_once("\x1b[?1049l").map_or("", |(_, after)| after);
    let modes: Vec<&str> = modes.split_whitespace().collect();
    assert!(
        modes.contains(&"icanon") && modes.contains(&"echo"),
        "the restarted client left the terminal raw: {modes:?}"
    );
}

#[test]
fn a_restart_that_cannot_reach_the_new_server_restores_the_terminal_and_says_why() {
    let mut app = Harness::start();
    app.click(app.position_of(" Settings "));
    app.wait_for("settings open", |s| s.contains(" restart "));
    app.click(app.position_of(" restart "));
    app.wait_for("the restart dialog opens", |s| s.contains("restart now"));
    let dir = app.session.dir.clone();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).expect("open the socket folder");

    app.click(app.position_of("restart now"));

    app.wait_exit("the client gives up");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("close the socket folder");
    app.wait_for_raw("the error after the alternate screen", |raw| {
        raw.split_once("\x1b[?1049l").is_some_and(|(_, after)| after.contains("can be written by other users"))
    });
    let raw = String::from_utf8_lossy(&app.raw.lock()).into_owned();
    assert!(raw.contains("\x1b[?1000l") && raw.contains("\x1b[?2004l"), "mouse or paste left on: {raw:?}");
    assert!(!app.screen.lock().screen().alternate_screen(), "the terminal stayed on the alternate screen");
}

#[test]
fn restart_when_idle_waits_for_the_agent_to_end_its_turn() {
    let mut app = Harness::start();
    std::fs::create_dir(app.session.dir.join("bin")).expect("create bin");
    let agent = app.session.dir.join("bin").join("claude");
    write_executable(
        &agent,
        r#"#!/bin/sh
d="$CLAUDE_CONFIG_DIR"; mkdir -p "$d/sessions"; printf '{"pid":%s,"status":"busy"}' $$ > "$d/sessions/$$.json"
while [ ! -e "$d/finish" ]; do sleep 0.02; done
printf '{"pid":%s,"status":"idle"}' $$ > "$d/sessions/$$.json"; while :; do sleep 1; done
"#,
    );
    let pane = app.session.says(&["new-tab", "--", &agent.display().to_string()]);
    app.session.wait_for_working();

    let early = app.session.cli(&["restart", "--when-idle", "--yes", "--timeout", "0.5"]);
    let restart = app.session.spawn(&["restart", "--when-idle", "--yes", "--timeout", "30"]);
    app.wait_for("the window says a restart is pending", |s| s.contains("restart pending until 1 agent ends its turn"));
    std::fs::write(app.session.claude_dir().join("finish"), "").expect("let the agent finish");
    let out = Session::output(restart);

    assert!(refused(&early, 1, &format!("the agent in pane {pane} has not ended its turn")), "{early:?}");
    assert!(out.contains("restarted the cornercase server") && out.contains("stopped 1 agent"), "{out}");
    app.wait_for("the window comes back", |s| s.contains(&first_entry()) && !s.contains("restart pending"));
}

#[test]
fn restart_without_a_server_says_so() {
    let session = Session::new();

    let out = session.cli(&["restart", "--yes"]);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success() && stderr.contains("no cornercase server is running"), "{out:?}");
}

#[rstest]
#[case::at_once("99.0.0", &[][..])]
#[case::once_no_agent_works("99.0.1", &["--when-idle"][..])]
fn update_installs_the_latest_release_and_restarts_the_server(#[case] version: &str, #[case] when: &[&str]) {
    let bin = installed_copy(&format!("update-{version}"));
    let mut app = Harness::open_from(&bin, Session::new(), ROWS, COLS, &[]);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    let name = format!("ccup-{version}-{}", std::process::id());
    let dir = temp_dir_named(&name);
    app.open_project(1, &dir);
    app.wait_for("project 2 appears", |s| s.contains(&entry(&name)));
    app.send(b"echo old-\"\"shell\r");
    app.wait_for("the old shell answers", |s| s.contains("old-shell"));
    app.send(b"sleep 600\r");
    app.session.wait_for_program("sleep");
    let marker = bin.with_file_name("new-client-ran");
    let new = format!(
        "#!/bin/sh\n[ \"$1\" = --version ] && exec echo 'cornercase {version}'\n[ $# -eq 0 ] && touch '{}'\nexec '{}' \"$@\"\n",
        marker.display(),
        env!("CARGO_BIN_EXE_cornercase")
    );

    let out = app
        .session
        .command_of(&bin)
        .args([&["update", "--yes"][..], when].concat())
        .env(update::LATEST_ENV, fake_release(version, &new))
        .output()
        .expect("run update");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    assert!(stdout.contains(&format!("updated cornercase {} → {version}", version::CURRENT)), "{stdout}");
    assert!(stdout.contains("restarted the cornercase server"), "{stdout}");
    assert_eq!(stdout.contains("restarts once no agent is working"), !when.is_empty(), "{stdout}");
    assert!(stdout.contains("1 program (`sleep` in ") && stdout.contains("stopped 1 program"), "{stdout}");
    assert_eq!(std::fs::read_to_string(&bin).expect("the new binary"), new);
    app.wait_for("the client comes back with both projects and new shells", |s| {
        !s.contains("old-shell") && s.contains(&first_entry()) && s.contains(&entry(&name))
    });
    assert!(marker.exists(), "the client came back without running the new binary");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(bin.parent().expect("its folder"));
}

#[test]
fn update_leaves_an_up_to_date_install_and_its_server_alone() {
    let app = Harness::start();
    let bin = installed_copy("up-to-date");

    let out = app
        .session
        .command_of(&bin)
        .arg("update")
        .env(update::LATEST_ENV, fake_release(version::CURRENT, "#!/bin/sh\n"))
        .output()
        .expect("run update");

    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), format!("cornercase {} is up to date\n", version::CURRENT));
    assert_eq!(app.session.servers(), 1);
    let _ = std::fs::remove_dir_all(bin.parent().expect("its folder"));
}

#[test]
fn a_development_build_refuses_to_update() {
    let session = Session::new();

    let out = session.run("update");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success() && stderr.contains("this is a development build"), "{out:?}");
}
