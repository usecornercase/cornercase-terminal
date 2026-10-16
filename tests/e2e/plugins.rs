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

#[test]
fn a_plugin_installs_from_a_repository_after_its_preview_updates_and_goes() {
    let app = Harness::start();
    let base = temp_dir_named(&format!("ccinstall-{}", std::process::id()));
    let work = base.join("work");
    std::fs::create_dir_all(&work).expect("work");
    git(&work, &["init", "--quiet", "--initial-branch=main"]);
    let folder = work.join("plugins/e2e");
    std::fs::create_dir_all(&folder).expect("plugin folder");
    let write = |version: &str| {
        let manifest = MANIFEST.replace("0.1.0", version);
        std::fs::write(folder.join("cornercase-plugin.toml"), manifest).expect("manifest");
    };
    write("0.1.0");
    std::fs::write(folder.join("run"), "#!/bin/sh\nexec sleep 60\n").expect("script");
    std::fs::set_permissions(folder.join("run"), std::fs::Permissions::from_mode(0o755)).expect("executable");
    git(&work, &["add", "--all"]);
    git(&work, &["commit", "--quiet", "-m", "one"]);
    git(&base, &["clone", "--quiet", "--bare", "work", "repo.git"]);
    let repo = base.join("repo.git").display().to_string();
    let plugin = format!("{repo}//plugins/e2e");

    let asked = app.session.cli(&["plugin", "install", &plugin]);
    let (out, err) = (String::from_utf8_lossy(&asked.stdout), String::from_utf8_lossy(&asked.stderr));
    assert!(!asked.status.success(), "{out}{err}");
    assert!(out.contains("service serve: ./run service"), "{out}");
    assert!(err.contains("pass --yes"), "{err}");
    assert!(app.session.says(&["plugin", "list"]).starts_with("no plugins"));

    let installed = app.session.says(&["plugin", "install", &plugin, "--yes"]);
    assert!(installed.contains("installed E2E 0.1.0 (acme.e2e), turned off"), "{installed}");
    let listed = app.session.says(&["plugin", "list"]);
    assert!(listed.starts_with("acme.e2e  0.1.0  off  E2E"), "{listed}");
    assert!(listed.contains(&format!("installed from {repo}, folder plugins/e2e, at")), "{listed}");
    assert!(app.session.says(&["plugin", "update", "acme.e2e"]).contains("acme.e2e is up to date"));

    write("0.2.0");
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
