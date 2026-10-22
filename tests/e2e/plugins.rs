use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::*;

const MANIFEST: &str = r#"id = "acme.e2e"
name = "E2E"
version = "0.1.0"
permissions = ["read"]

[[service]]
id = "serve"
command = ["./run", "service"]

[[window]]
id = "panel"
command = ["./run", "window"]

[[actions]]
id = "note"
title = "take a note"
contexts = ["pane", "tab"]
command = ["/bin/sh", "-c", "echo $$ > \"$CORNERCASE_PLUGIN_STATE/note\""]

[[actions]]
id = "fail"
title = "break"
contexts = ["pane"]
command = ["/bin/sh", "-c", "exit 3"]
"#;

fn plugin_folder(name: &str) -> PathBuf {
    let dir = temp_dir_named(name);
    std::fs::write(dir.join("cornercase-plugin.toml"), MANIFEST).expect("manifest");
    let run = dir.join("run");
    std::fs::write(&run, "#!/bin/sh\necho $$ > \"$CORNERCASE_PLUGIN_STATE/$1\"\nexec sleep 60\n").expect("script");
    std::fs::set_permissions(&run, std::fs::Permissions::from_mode(0o755)).expect("executable");
    dir
}

fn pid_in(path: &Path) -> Option<i32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn alive(pid: i32) -> bool {
    std::process::Command::new("kill").args(["-0", &pid.to_string()]).status().is_ok_and(|s| s.success())
}

fn wait_for_pid(path: &Path, what: &str) -> i32 {
    let mut found = None;
    wait_until(what, || {
        found = pid_in(path).filter(|pid| alive(*pid));
        found.is_some()
    });
    found.expect("a live pid")
}

#[test]
fn a_linked_plugin_runs_once_it_is_enabled_and_stops_once_disabled() {
    let app = Harness::start();
    let folder = plugin_folder(&format!("ccplug-{}", std::process::id()));
    let path = folder.display().to_string();

    let linked = app.session.says(&["plugin", "link", &path]);
    let listed = app.session.says(&["plugin", "list"]);
    assert!(linked.starts_with("linked E2E 0.1.0 (acme.e2e), turned off"), "{linked}");
    assert!(listed.starts_with("acme.e2e  0.1.0  off  E2E"), "{listed}");
    assert!(listed.contains("runs 1 service, 1 window process"), "{listed}");

    let enabled = app.session.says(&["plugin", "enable", "acme.e2e"]);
    assert!(enabled.contains("runs as you"), "{enabled}");
    let state = app.session.dir.join("plugins/state/acme.e2e");
    let service = wait_for_pid(&state.join("service"), "the service starts");
    let window = wait_for_pid(&state.join("window"), "the window process starts");

    let json: serde_json::Value = serde_json::from_str(&app.session.says(&["plugin", "list", "--json"])).expect("json");
    assert_eq!(json["plugins"][0]["enabled"], true);
    assert!(app.session.says(&["plugin", "logs", "acme.e2e"]).contains("service started"));

    app.session.says(&["plugin", "disable", "acme.e2e"]);
    wait_until("the plugin's processes stop", || !alive(service) && !alive(window));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn an_unknown_plugin_is_an_error() {
    let app = Harness::start();
    let out = app.session.cli(&["plugin", "enable", "acme.missing"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no plugin called `acme.missing`"), "{out:?}");
}

fn write_plugin(folder: &Path, version: &str) {
    let manifest = MANIFEST.replace("0.1.0", version);
    std::fs::write(folder.join("cornercase-plugin.toml"), manifest).expect("manifest");
}

fn plugin_repository(base: &Path) -> (PathBuf, PathBuf, String) {
    let work = base.join("work");
    std::fs::create_dir_all(&work).expect("work");
    git(&work, &["init", "--quiet", "--initial-branch=main"]);
    let folder = work.join("plugins/e2e");
    std::fs::create_dir_all(&folder).expect("plugin folder");
    write_plugin(&folder, "0.1.0");
    std::fs::write(folder.join("run"), "#!/bin/sh\nexec sleep 60\n").expect("script");
    std::fs::set_permissions(folder.join("run"), std::fs::Permissions::from_mode(0o755)).expect("executable");
    git(&work, &["add", "--all"]);
    git(&work, &["commit", "--quiet", "-m", "one"]);
    git(base, &["clone", "--quiet", "--bare", "work", "repo.git"]);
    let repo = base.join("repo.git").display().to_string();
    (work, folder, repo)
}

fn assert_install_asks_first(session: &Session, plugin: &str) {
    let asked = session.cli(&["plugin", "install", plugin]);
    let (out, err) = (String::from_utf8_lossy(&asked.stdout), String::from_utf8_lossy(&asked.stderr));
    assert!(!asked.status.success(), "{out}{err}");
    assert!(out.contains("service serve: ./run service"), "{out}");
    assert!(err.contains("pass --yes"), "{err}");
    assert!(session.says(&["plugin", "list"]).starts_with("no plugins"));
}

fn assert_installed_and_current(session: &Session, plugin: &str, repo: &str) {
    let installed = session.says(&["plugin", "install", plugin, "--yes"]);
    assert!(installed.contains("installed E2E 0.1.0 (acme.e2e), turned off"), "{installed}");
    let listed = session.says(&["plugin", "list"]);
    assert!(listed.starts_with("acme.e2e  0.1.0  off  E2E"), "{listed}");
    assert!(listed.contains(&format!("installed from {repo}, folder plugins/e2e, at")), "{listed}");
    assert!(session.says(&["plugin", "update", "acme.e2e"]).contains("acme.e2e is up to date"));
}

#[test]
fn a_plugin_installs_from_a_repository_after_its_preview_updates_and_goes() {
    let app = Harness::start();
    let base = temp_dir_named(&format!("ccinstall-{}", std::process::id()));
    let (work, folder, repo) = plugin_repository(&base);
    let plugin = format!("{repo}//plugins/e2e");

    assert_install_asks_first(&app.session, &plugin);
    assert_installed_and_current(&app.session, &plugin, &repo);

    write_plugin(&folder, "0.2.0");
    git(&work, &["commit", "--quiet", "-am", "two"]);
    git(&work, &["push", "--quiet", &repo, "HEAD:main"]);
    let updated = app.session.says(&["plugin", "update", "--yes"]);
    assert!(updated.contains("E2E 0.2.0 (acme.e2e)") && updated.contains("updated acme.e2e to 0.2.0"), "{updated}");

    let unlink = app.session.cli(&["plugin", "unlink", "acme.e2e"]);
    assert!(String::from_utf8_lossy(&unlink.stderr).contains("plugin remove acme.e2e"), "{unlink:?}");
    let removed = app.session.says(&["plugin", "remove", "acme.e2e"]);
    assert!(removed.starts_with("removed acme.e2e"), "{removed}");
    assert!(!app.session.dir.join("plugins/installed/acme.e2e").exists());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn another_client_lists_and_runs_the_actions_as_the_menu_does() {
    let app = Harness::start();
    let folder = plugin_folder(&format!("ccact-{}", std::process::id()));
    let path = folder.display().to_string();
    app.session.says(&["plugin", "link", &path]);
    app.session.says(&["plugin", "enable", "acme.e2e"]);

    let listed: serde_json::Value =
        serde_json::from_str(&app.session.says(&["plugin", "actions", "--context", "pane", "--json"])).expect("json");
    assert_eq!((&listed["actions"][0]["action"], &listed["actions"][1]["action"]), (&"note".into(), &"fail".into()));
    assert!(listed["revision"].as_u64().is_some_and(|revision| revision > 0), "{listed}");
    let text = app.session.says(&["plugin", "actions"]);
    assert!(text.starts_with("plugin\taction\tcontexts\ttitle\nacme.e2e\tnote\tpane,tab\ttake a note"), "{text}");

    let rows: serde_json::Value =
        serde_json::from_str(&app.session.says(&["status", "--panes", "--json"])).expect("rows");
    let pane = rows["panes"][0]["pane"].as_u64().expect("a pane").to_string();
    let ran = app.session.says(&["plugin", "run", "acme.e2e", "note", "--pane", &pane]);
    assert_eq!(ran, format!("ran acme.e2e note on pane {pane}: exit status: 0"));
    let written = std::fs::read_to_string(app.session.dir.join("plugins/state/acme.e2e/note")).expect("its file");
    assert!(written.trim().parse::<i32>().is_ok(), "{written}");

    let failed = app.session.cli(&["plugin", "run", "acme.e2e", "fail", "--pane", &pane]);
    assert_eq!(failed.status.code(), Some(1), "{failed:?}");
    let said = String::from_utf8_lossy(&failed.stdout);
    assert!(said.contains("exit status: 3") && said.contains("cornercase plugin logs acme.e2e"), "{said}");

    let stale = app.session.cli(&["plugin", "run", "acme.e2e", "note", "--pane", &pane, "--revision", "0"]);
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("changed"), "{stale:?}");
    let _ = std::fs::remove_dir_all(&folder);
}

const POPUP_MANIFEST: &str = r#"id = "acme.popup"
name = "Popper"
version = "0.1.0"

[button]
title = "Popper"

[[actions]]
id = "ask"
title = "ask a line"
contexts = ["button"]
placement = "popup"
width = 40
height = 6
command = ["/bin/sh", "-c", 'echo popup-""ready; read x; echo "$x" > "$CORNERCASE_PLUGIN_STATE/said"']
"#;

fn popup_plugin(session: &Session, test: &str) -> PathBuf {
    let folder = temp_dir_named(&format!("ccpopup-{test}-{}", std::process::id()));
    std::fs::write(folder.join("cornercase-plugin.toml"), POPUP_MANIFEST).expect("manifest");
    session.says(&["plugin", "link", &folder.display().to_string()]);
    session.says(&["plugin", "enable", "acme.popup"]);
    folder
}

fn pane_text(app: &Harness) -> String {
    let p = pane();
    (p.y..p.bottom()).map(|y| app.row(y).chars().skip(usize::from(p.x)).collect::<String>() + "\n").collect()
}

#[test]
fn a_button_opens_its_popup_on_every_client_and_its_keys_never_reach_the_pane() {
    let mut app = Harness::start();
    app.send(b"echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs in the pane", |s| s.contains("cat-starts"));
    let folder = popup_plugin(&app.session, "every-client");
    app.wait_for("the plugin's button shows", |s| s.contains("Popper"));

    app.click(app.position_of("Popper"));
    app.wait_for("the popup shows its program", |s| s.contains("popup-ready") && s.contains("Popper: ask a line"));
    let ready = app.row(app.position_of("popup-ready").y);
    assert!(ready.split("popup-ready").next().is_some_and(|left| left.contains('│')), "{ready}");

    let mut other = app.attach(ROWS, COLS);
    other.wait_for("another client sees the popup", |s| s.contains("popup-ready"));
    other.send(b"ok\r");
    app.wait_for("the popup closes", |s| !s.contains("popup-ready"));
    let said = app.session.dir.join("plugins/state/acme.popup/said");
    wait_until("the popup's program read the line", || std::fs::read_to_string(&said).is_ok_and(|s| s == "ok\n"));

    app.send(b"after\r");
    app.wait_for("cat echoes what follows", |s| s.matches("after").count() == 2);
    assert!(!pane_text(&app).contains("ok"), "{}", app.text());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_popup_stays_open_when_the_last_client_leaves() {
    let mut app = Harness::start();
    app.send(b"echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs in the pane", |s| s.contains("cat-starts"));
    let folder = popup_plugin(&app.session, "stays-open");
    app.wait_for("the plugin's button shows", |s| s.contains("Popper"));
    app.click(app.position_of("Popper"));
    app.wait_for("the popup shows its program", |s| s.contains("popup-ready"));

    let session = Arc::clone(&app.session);
    drop(app);
    let mut back = Harness::open(session, ROWS, COLS);
    back.wait_for("the popup is still there", |s| s.contains("popup-ready"));
    back.send(b"ok\r");
    back.wait_for("the popup closes", |s| !s.contains("popup-ready"));
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn stopping_the_server_answers_a_run_waiting_on_its_popup() {
    let mut app = Harness::start();
    let folder = popup_plugin(&app.session, "server-stops");
    app.wait_for("the plugin's button shows", |s| s.contains("Popper"));
    let waiting = app
        .session
        .command()
        .args(["plugin", "run", "acme.popup", "ask"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("plugin run");
    app.wait_for("the popup shows its program", |s| s.contains("popup-ready"));

    assert!(app.session.run("kill-server").status.success());
    let ran = waiting.wait_with_output().expect("plugin run ends");
    let said = String::from_utf8_lossy(&ran.stdout);
    assert!(said.starts_with("ran acme.popup ask: the popup was closed"), "{ran:?}");
    let _ = std::fs::remove_dir_all(&folder);
}
