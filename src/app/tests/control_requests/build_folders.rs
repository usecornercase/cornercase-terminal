use super::worktrees::{made, repo};
use super::*;
use crate::config::ProjectSettings;

fn write(path: &Path, bytes: usize) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, vec![7_u8; bytes]).expect("write");
}

pub(super) fn with_builds(path: &Path) {
    std::fs::write(path.join(".gitignore"), "node_modules/\n").expect("ignore");
    write(&path.join("node_modules/x.js"), 300_000);
}

#[test]
fn a_workspaces_size_says_its_total_and_its_build_folders() {
    let (mut app, rx, _repo, _worktrees, _config) = repo();
    let (id, path) = made(&mut app, &rx);
    with_builds(&path);

    let sized = asked(&mut app, &rx, "worktree-size", &json!({"workspace": id})).expect("an answer");

    assert_eq!(sized["path"], path.display().to_string());
    assert_eq!(sized["builds"][0]["path"], "node_modules");
    assert!(sized["builds_bytes"].as_u64() >= Some(300_000));
    assert!(sized["bytes"].as_u64() > sized["builds_bytes"].as_u64());
}

#[test]
fn a_worktree_is_sized_by_its_path_open_or_not_and_the_main_checkout_is_refused() {
    let (mut app, rx, repo, _worktrees, _config) = repo();
    let (_, path) = made(&mut app, &rx);
    with_builds(&path);

    let by_path = asked(&mut app, &rx, "worktree-size", &json!({"path": path})).expect("an answer");
    assert_eq!(by_path["builds"][0]["path"], "node_modules");

    let main = refused(&mut app, "worktree-size", &json!({"path": repo.path()}));
    assert_eq!(main.code, "conflict");
    assert!(main.message.contains("is not a linked worktree"), "{}", main.message);
}

#[test]
fn a_plain_workspace_and_two_targets_are_refused() {
    let (mut app, _rx, _repo, _worktrees, _config) = repo();
    let plain = app.projects[0].workspaces[0].id;
    let refusal = refused(&mut app, "worktree-size", &json!({"workspace": plain}));
    assert_eq!(refusal.code, "conflict");
    assert!(refusal.message.contains("has no worktree of its own"), "{}", refusal.message);
    let both = refused(&mut app, "free-builds", &json!({"workspace": plain, "path": "/x"}));
    assert_eq!(both.code, "malformed");
}

#[test]
fn a_check_deletes_nothing_and_a_free_deletes_the_ignored_folder() {
    let (mut app, rx, _repo, _worktrees, _config) = repo();
    let (id, path) = made(&mut app, &rx);
    with_builds(&path);
    write(&path.join("mine.txt"), 5);

    let check = asked(&mut app, &rx, "free-builds", &json!({"workspace": id, "check": true})).expect("an answer");
    assert_eq!((check["check"].clone(), check["folders"][0]["path"].clone()), (json!(true), json!("node_modules")));
    assert!(path.join("node_modules").exists());

    let freed = asked(&mut app, &rx, "free-builds", &json!({"workspace": id})).expect("an answer");
    assert_eq!(freed["check"], false);
    assert!(freed["bytes"].as_u64() >= Some(300_000));
    assert!(!path.join("node_modules").exists() && path.join("mine.txt").exists());
}

#[test]
fn a_program_in_the_worktree_refuses_without_force_and_force_frees() {
    let (mut app, rx, _repo, _worktrees, _config) = repo();
    let (id, path) = made(&mut app, &rx);
    with_builds(&path);
    let sleeper =
        crate::test_util::Running::spawn(std::process::Command::new("/bin/sleep").arg("30").current_dir(&path));
    wait_until("sleep works in the folder", || process::cwd(sleeper.pid()).as_deref() == Some(path.as_path()));

    let refusal = asked(&mut app, &rx, "free-builds", &json!({"workspace": id})).expect_err("refused");
    assert_eq!(refusal.code, "conflict");
    assert!(refusal.message.contains("sleep") && refusal.message.contains("--force"), "{}", refusal.message);
    assert!(path.join("node_modules").exists());

    let forced = asked(&mut app, &rx, "free-builds", &json!({"workspace": id, "force": true})).expect("an answer");
    assert!(forced["running"].as_array().is_some_and(|r| r.iter().any(|n| n == "sleep")));
    assert!(!path.join("node_modules").exists());
}

#[test]
fn a_git_checkout_inside_a_build_folder_refuses_without_force() {
    let (mut app, rx, repo, _worktrees, _config) = repo();
    let (id, path) = made(&mut app, &rx);
    std::fs::write(path.join(".gitignore"), ".venv/\n").expect("ignore");
    write(&path.join(".venv/src/foo/.git/HEAD"), 5);
    let key = repo.path().display().to_string();
    app.config.projects.insert(key, ProjectSettings { builds: Some(vec![".venv".into()]), ..Default::default() });

    let refusal = asked(&mut app, &rx, "free-builds", &json!({"workspace": id})).expect_err("refused");

    assert!(refusal.message.contains("is a git checkout"), "{}", refusal.message);
}

#[test]
fn the_config_request_sets_resets_and_refuses_build_folders() {
    let (mut app, _rx, repo, _worktrees, _config) = repo();
    let id = app.projects[0].id;
    let set = call(&mut app, "config", &json!({"action": "project", "entry": {"project": id, "builds": ["out"]}}));
    set.expect("saved");
    assert_eq!(app.config.project(repo.path(), None).builds(), ["out"]);
    let got = call(&mut app, "config", &json!({"action": "get"})).expect("settings");
    assert_eq!(got["projects"][0]["builds"], json!(["out"]));

    let defaults: Vec<String> = crate::config::DEFAULT_BUILDS.iter().map(|n| (*n).to_string()).collect();
    call(&mut app, "config", &json!({"action": "project", "entry": {"project": id, "builds": defaults}}))
        .expect("saved");
    assert!(app.config.projects.is_empty());

    let bad = call(&mut app, "config", &json!({"action": "project", "entry": {"project": id, "builds": ["a/b"]}}));
    assert_eq!(bad.expect_err("refused").code, "malformed");
}

#[test]
fn a_background_program_started_from_a_panes_shell_refuses_without_force() {
    let (mut app, rx, _repo, _worktrees, _config) = repo();
    let (id, path) = made(&mut app, &rx);
    with_builds(&path);
    let at = app.projects.workspace_index(id).expect("the workspace");
    let pane = app.projects[at.project()].workspaces[at.index()].tabs[0].pane().expect("a pane").id;
    ok(&mut app, "input", &json!({"pane": pane, "events": [{"text": "sleep 47 &"}, {"key": {"code": "enter"}}]}));
    wait_until("the background sleep works in the folder", || {
        process::all().into_iter().any(|pid| {
            process::cwd(pid).as_deref() == Some(path.as_path()) && process::name(pid).as_deref() == Some("sleep")
        })
    });

    let refusal = asked(&mut app, &rx, "free-builds", &json!({"workspace": id})).expect_err("refused");

    assert_eq!(refusal.code, "conflict");
    assert!(refusal.message.contains("sleep"), "{}", refusal.message);
    assert!(path.join("node_modules").exists());
    for pid in process::all() {
        if process::cwd(pid).as_deref() == Some(path.as_path()) && process::name(pid).as_deref() == Some("sleep") {
            let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
        }
    }
}
