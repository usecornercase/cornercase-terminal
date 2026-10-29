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
