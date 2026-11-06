use std::path::PathBuf;

use serde_json::Value;

use super::*;

struct Opened {
    session: Arc<Session>,
    _app: Harness,
    repo: PathBuf,
    checkout: PathBuf,
}

fn open_worktree(name: &str, branch: &str) -> Opened {
    let session = Session::new();
    let worktrees = session.dir.join("worktrees");
    std::fs::write(session.dir.join("config.json"), format!("{{\"worktrees_dir\": \"{}\"}}", worktrees.display()))
        .expect("write config");
    let repo = temp_dir(name);
    git(&repo, &["init", "--quiet"]);
    std::fs::write(repo.join(".gitignore"), "node_modules/\n").expect("write .gitignore");
    std::fs::write(repo.join("tracked.txt"), "keep\n").expect("write a file");
    git(&repo, &["add", "--all"]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    let mut app = Harness::open(Arc::clone(&session), ROWS, COLS);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    let project = session.says(&["open", &repo.display().to_string(), "--focus"]);
    session.says(&["new-workspace", branch, "--worktree", "--project", &project]);
    let slug = branch.replace('/', "-");
    let checkout = worktrees.join(repo.file_name().expect("repo name")).join(slug);
    std::fs::create_dir_all(checkout.join("node_modules")).expect("make node_modules");
    std::fs::write(checkout.join("node_modules/x.js"), vec![7_u8; 300_000]).expect("write a file");
    Opened { session, _app: app, repo, checkout }
}

#[test]
fn a_worktrees_build_folders_are_measured_then_freed_and_the_rest_stays() {
    let Opened { session, repo, checkout, .. } = open_worktree("worktree-builds", "e2e/builds");
    let size = session.says(&["worktrees", "size", "--worktree", "e2e/builds", "--json"]);
    let size: Value = serde_json::from_str(&size).expect("the size");
    assert!(size["builds_bytes"].as_u64() >= Some(300_000), "{size}");
    assert_eq!(size["builds"][0]["path"], "node_modules");
    let check = session.says(&["worktrees", "free-builds", "--worktree", "e2e/builds", "--check"]);
    assert!(check.starts_with("would free ") && check.contains("node_modules"), "{check}");
    assert!(checkout.join("node_modules").is_dir(), "the check deleted the folder");

    let freed = session.says(&["worktrees", "free-builds", "--worktree", "e2e/builds"]);

    assert!(freed.starts_with("freed ") && freed.contains("node_modules"), "{freed}");
    assert!(!checkout.join("node_modules").exists(), "the folder is still there");
    assert!(checkout.join("tracked.txt").is_file(), "a tracked file went too");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn every_worktree_is_listed_with_its_reasons_and_size() {
    let Opened { session, repo, checkout, .. } = open_worktree("worktree-list", "e2e/list");
    std::fs::write(checkout.join("notes.txt"), "draft").expect("write a file");

    let listed = session.says(&["worktrees", "list", "--json"]);
    let listed: Value = serde_json::from_str(&listed).expect("the list");
    let row = listed["worktrees"].as_array().and_then(|rows| rows.iter().find(|r| r["branch"] == "e2e/list"));
    let row = row.unwrap_or_else(|| panic!("no row: {listed}"));
    assert_eq!((row["changed"].clone(), row["safe"].clone()), (1.into(), false.into()), "{row}");
    assert!(row["size"]["builds_bytes"].as_u64() >= Some(300_000), "{row}");
    assert!(row["workspace"].is_u64(), "{row}");

    let text = session.says(&["worktrees", "list"]);
    assert!(text.contains("› e2e/list") && text.contains("1 file changed"), "{text}");
    let bare = session.says(&["worktrees", "list", "--no-sizes", "--json"]);
    let bare: Value = serde_json::from_str(&bare).expect("the list");
    assert_eq!(bare["worktrees"][0]["size"], Value::Null, "{bare}");
    let _ = std::fs::remove_dir_all(&repo);
}

fn workspaces(session: &Session) -> Vec<Value> {
    let status: Value = serde_json::from_str(&session.says(&["status", "--json"])).expect("the status");
    let projects = status["projects"].as_array().expect("projects");
    projects.iter().flat_map(|project| project["workspaces"].as_array().expect("workspaces").clone()).collect()
}

fn workspace_on(session: &Session, branch: &str) -> Value {
    workspaces(session).into_iter().find(|w| w["branch"] == branch).expect("a workspace on the branch")
}

fn setup_of(session: &Session, branch: &str) -> Value {
    workspace_on(session, branch)["setup"].clone()
}

#[test]
fn a_failed_setup_runs_again_by_command_with_its_variables_and_its_mark_goes() {
    let Opened { session, _app: _window, repo, .. } = open_worktree("setup-again", "e2e/plain");
    let folder = repo.display().to_string();
    let script = "echo \"in [$CORNERCASE_WORKTREE]\"\ntest -f \"$CORNERCASE_ROOT/ok\"";
    session.says(&["config", "project", &folder, "--setup", script]);
    let project = session.says(&["open", &folder]);

    let made = session.cli(&["new-workspace", "feat/x", "--worktree", "--project", &project]);
    assert!(refused(&made, 1, "failed with exit status 1"), "{made:?}");
    let failed = workspace_on(&session, "feat/x");
    assert_eq!(failed["setup"], "failed", "{failed}");
    let id = failed["id"].to_string();

    let again = session.cli(&["setup", "--workspace", &id]);
    assert!(refused(&again, 1, "failed with exit status 1"), "{again:?}");
    assert!(String::from_utf8_lossy(&again.stderr).contains("cornercase read --pane"), "{again:?}");
    assert_eq!(setup_of(&session, "feat/x"), "failed");

    std::fs::write(repo.join("ok"), "").expect("write the marker");
    let pane = session.says(&["setup", "--worktree", "feat/x"]);

    let screen = session.says(&["read", "--pane", &pane]);
    let path = failed["path"].as_str().expect("its folder");
    assert!(screen.contains(&format!("in [{path}]")) && screen.contains("setup finished"), "{screen}");
    assert_eq!(setup_of(&session, "feat/x"), Value::Null);
    let printed: Value = serde_json::from_str(&session.says(&["setup", "--workspace", &id, "--json"])).expect("json");
    assert_eq!(printed["workspace"].to_string(), id);
    assert!(printed["project"].is_u64() && printed["tab"].is_u64() && printed["pane"].is_u64(), "{printed}");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn a_running_setup_is_not_started_twice_and_goes_on_without_the_command_that_asked() {
    let Opened { session, _app: _window, repo, .. } = open_worktree("setup-held", "e2e/held");
    let script = "while [ -e \"$CORNERCASE_ROOT/hold\" ]; do sleep 0.02; done";
    session.says(&["config", "project", &repo.display().to_string(), "--setup", script]);
    let hold = repo.join("hold");
    std::fs::write(&hold, "").expect("hold the setup");

    let mut left = session.spawn(&["setup", "--worktree", "e2e/held"]);
    wait_until("the setup runs", || setup_of(&session, "e2e/held") == "running");
    let twice = session.cli(&["setup", "--worktree", "e2e/held"]);
    assert!(refused(&twice, 1, "already running its setup"), "{twice:?}");
    assert!(left.try_wait().expect("look at the command").is_none(), "it answered before the setup ended");
    left.kill().expect("stop the command");
    left.wait().expect("wait for the command");
    assert_eq!(setup_of(&session, "e2e/held"), "running", "stopping the command stopped the setup");
    std::fs::remove_file(&hold).expect("let the setup end");
    wait_until("the setup ends", || setup_of(&session, "e2e/held") == Value::Null);

    std::fs::write(&hold, "").expect("hold the setup");
    let answering = session.spawn(&["setup", "--worktree", "e2e/held"]);
    wait_until("the setup runs again", || setup_of(&session, "e2e/held") == "running");
    std::fs::remove_file(&hold).expect("let the setup end");
    let pane = Session::output(answering);
    assert!(pane.parse::<u64>().is_ok(), "{pane}");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn what_the_menu_does_not_offer_is_refused_in_words() {
    let Opened { session, _app: _window, repo, .. } = open_worktree("setup-refused", "e2e/refused");
    let none = session.cli(&["setup", "--worktree", "e2e/refused"]);
    assert!(refused(&none, 1, "has no setup"), "{none:?}");
    session.says(&["config", "project", &repo.display().to_string(), "--setup", "true"]);
    let folder = repo.display().to_string();
    let all = workspaces(&session);
    let plain = all.iter().find(|w| w["path"] == folder.as_str()).expect("the project's own workspace");

    let refused_plain = session.cli(&["setup", "--workspace", &plain["id"].to_string()]);

    assert!(refused(&refused_plain, 1, "is not a worktree"), "{refused_plain:?}");
    let missing = session.cli(&["setup", "--workspace", "999"]);
    assert!(refused(&missing, 1, "there is no workspace 999"), "{missing:?}");
    let unknown = session.cli(&["setup", "--worktree", "no/such"]);
    assert!(refused(&unknown, 2, "no workspace"), "{unknown:?}");
    let _ = std::fs::remove_dir_all(&repo);
}
