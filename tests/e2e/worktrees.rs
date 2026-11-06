use std::path::PathBuf;

use serde_json::Value;

use super::*;

struct Opened {
    session: Arc<Session>,
    _app: Harness,
    project: String,
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
    Opened { session, _app: app, project, repo, checkout }
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

fn has_branch(repo: &std::path::Path, branch: &str) -> bool {
    let reference = format!("refs/heads/{branch}");
    let mut found = std::process::Command::new("git");
    found.arg("-C").arg(repo).args(["show-ref", "--quiet", &reference]);
    found.status().expect("run git").success()
}

fn row_of(session: &Session, branch: &str) -> Option<Value> {
    let listed: Value =
        serde_json::from_str(&session.says(&["worktrees", "list", "--no-sizes", "--json"])).expect("the list");
    listed["worktrees"].as_array()?.iter().find(|row| row["branch"] == branch).cloned()
}

fn closed_with_a_worktree(name: &str, branch: &str) -> (Opened, String) {
    let opened = open_worktree(name, branch);
    let session = &opened.session;
    session.says(&["close", "--project", &opened.project]);
    wait_until("the project closes", || row_of(session, branch).is_some_and(|row| row["project"].is_null()));
    let row = row_of(session, branch).expect("the row");
    assert_eq!((&row["kind"], &row["workspace"]), (&Value::from("linked"), &Value::Null), "{row}");
    let folder = row["path"].as_str().expect("its folder").to_string();
    (opened, folder)
}

#[test]
fn closing_cannot_reach_a_closed_projects_worktree_and_a_check_stops_nothing() {
    let (Opened { session, repo, checkout, .. }, folder) = closed_with_a_worktree("worktree-reach", "feat/old");
    let closing = session.cli(&["close", "--worktree", "feat/old", "--remove-worktree"]);
    assert_eq!(closing.status.code(), Some(2), "{closing:?}");

    let look = session.says(&["worktrees", "remove", "--path", &folder, "--check", "--delete-branch"]);

    assert!(look.contains("› feat/old") && look.contains("nothing uncommitted or unpushed"), "{look}");
    assert!(checkout.join("tracked.txt").is_file() && has_branch(&repo, "feat/old"), "the check removed something");
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn a_closed_projects_worktree_and_its_branch_are_removed_from_the_command_line() {
    let (Opened { session, repo, checkout, .. }, folder) = closed_with_a_worktree("worktree-remove", "feat/old");

    let removed = session.says(&["worktrees", "remove", "--path", &folder, "--delete-branch"]);

    assert!(removed.starts_with("removed ") && removed.ends_with("and the branch feat/old"), "{removed}");
    assert!(!checkout.exists() && !has_branch(&repo, "feat/old"), "the folder or the branch is still there");
    assert!(row_of(&session, "feat/old").is_none(), "it is still listed");
    let again = session.cli(&["worktrees", "remove", "--path", &folder]);
    assert_eq!(again.status.code(), Some(3), "{again:?}");
    let said = String::from_utf8_lossy(&again.stdout);
    assert!(said.starts_with("did not remove ") && said.contains("nothing is there"), "{said}");
    let _ = std::fs::remove_dir_all(&repo);
}

fn with_a_draft(name: &str, branch: &str) -> (Opened, String, String) {
    let opened = open_worktree(name, branch);
    std::fs::write(opened.checkout.join("notes.txt"), "draft").expect("write a file");
    let (folder, main) = (opened.checkout.display().to_string(), opened.repo.display().to_string());
    (opened, folder, main)
}

#[test]
fn what_cannot_be_removed_is_named_line_by_line_and_exits_3() {
    let (Opened { session, repo, checkout, .. }, folder, main) = with_a_draft("worktree-refuse", "e2e/refuse");

    let refused = session.cli(&["worktrees", "remove", "--path", &folder, &main]);

    assert_eq!(refused.status.code(), Some(3), "{refused:?}");
    let said = String::from_utf8_lossy(&refused.stdout);
    let lines: Vec<&str> = said.lines().collect();
    assert_eq!(lines.len(), 2, "{said}");
    assert!(lines[0].contains("1 file changed") && lines[0].contains("--force"), "{said}");
    assert!(lines[1].contains("main checkout"), "{said}");
    assert!(checkout.join("notes.txt").is_file() && repo.join("tracked.txt").is_file());
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn a_check_takes_a_folder_relative_to_the_command_and_says_what_removing_it_takes() {
    let (Opened { session, repo, checkout, .. }, ..) = with_a_draft("worktree-relative", "e2e/relative");
    let mut relative = session.command();
    relative.current_dir(checkout.parent().expect("a parent"));
    relative.args(["worktrees", "remove", "--path", "e2e-relative", "--check"]);

    let look = relative.output().expect("run cornercase");

    let said = String::from_utf8_lossy(&look.stdout);
    assert!(said.contains("› e2e/relative") && said.contains("1 file changed"), "{look:?}");
    assert!(said.contains("removing it takes --force"), "{said}");
    assert!(checkout.join("notes.txt").is_file());
    let _ = std::fs::remove_dir_all(&repo);
}

#[test]
fn force_removes_an_open_workspace_and_keeps_its_branch() {
    let (Opened { session, repo, checkout, .. }, folder, _) = with_a_draft("worktree-force", "e2e/force");

    let forced = session.says(&["worktrees", "remove", "--path", &folder, "--force", "--json"]);

    let forced: Value = serde_json::from_str(&forced).expect("the answer");
    assert_eq!(forced["folders"][0]["outcome"], "removed", "{forced}");
    assert!(!checkout.exists() && has_branch(&repo, "e2e/force"), "the folder is still there or the branch went");
    let _ = std::fs::remove_dir_all(&repo);
}
