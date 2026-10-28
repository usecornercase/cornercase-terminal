use serde_json::Value;

use super::*;

#[test]
fn a_worktrees_build_folders_are_measured_then_freed_and_the_rest_stays() {
    let session = Session::new();
    let worktrees = session.dir.join("worktrees");
    std::fs::write(session.dir.join("config.json"), format!("{{\"worktrees_dir\": \"{}\"}}", worktrees.display()))
        .expect("write config");
    let repo = temp_dir("worktree-builds");
    git(&repo, &["init", "--quiet"]);
    std::fs::write(repo.join(".gitignore"), "node_modules/\n").expect("write .gitignore");
    std::fs::write(repo.join("tracked.txt"), "keep\n").expect("write a file");
    git(&repo, &["add", "--all"]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    let mut app = Harness::open(Arc::clone(&session), ROWS, COLS);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    let project = session.says(&["open", &repo.display().to_string(), "--focus"]);
    session.says(&["new-workspace", "e2e/builds", "--worktree", "--project", &project]);
    let checkout = worktrees.join(repo.file_name().expect("repo name")).join("e2e-builds");
    std::fs::create_dir_all(checkout.join("node_modules")).expect("make node_modules");
    std::fs::write(checkout.join("node_modules/x.js"), vec![7_u8; 300_000]).expect("write a file");

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
