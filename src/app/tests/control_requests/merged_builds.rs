use std::path::{Path, PathBuf};

use super::worktrees_remove::{Closed, closed_repo};
use super::*;
use crate::test_util::{git, give_upstream};

fn with_builds(path: &Path) {
    std::fs::write(path.join(".gitignore"), "node_modules/\n").expect("ignore");
    std::fs::create_dir_all(path.join("node_modules")).expect("mkdir");
    std::fs::write(path.join("node_modules/x.js"), vec![7_u8; 300_000]).expect("write");
}

fn temp_only(s: &Closed) {
    let dir = s.app.config.worktrees_dir(s.app.home.as_deref());
    assert!(dir.starts_with(s.worktrees.path()), "{}", dir.display());
}

struct Trees {
    s: Closed,
    merged: PathBuf,
    fresh: PathBuf,
    ahead: PathBuf,
}

fn merged_and_others() -> Trees {
    let s = closed_repo();
    let (merged, fresh, ahead) = (s.add("merged"), s.add("fresh"), s.add("ahead"));
    give_upstream(s.repo.path(), "merged");
    git(&ahead, &["commit", "--quiet", "--allow-empty", "-m", "more"]);
    for path in [&merged, &fresh, &ahead] {
        with_builds(path);
    }
    Trees { s, merged, fresh, ahead }
}

impl Trees {
    fn ask(&mut self, command: &str, args: &Value) -> Value {
        temp_only(&self.s);
        call_later(&mut self.s.app, &self.s.rx, command, args).expect("an answer")
    }

    fn listing(&mut self, args: &Value) -> Value {
        self.ask("worktrees", args)
    }

    fn sweep(&mut self, check: bool) -> Value {
        self.ask("free-merged-builds", &json!({"project": null, "check": check}))
    }
}

fn row<'a>(listing: &'a Value, branch: &str) -> &'a Value {
    let rows = listing["worktrees"].as_array().expect("a list");
    rows.iter().find(|row| row["branch"] == branch).unwrap_or_else(|| panic!("no {branch}: {listing}"))
}

#[test]
fn the_list_adds_up_the_builds_of_merged_worktrees_only() {
    let mut m = merged_and_others();

    let listing = m.listing(&json!({}));

    assert_eq!(row(&listing, "merged")["merged"], true, "{listing}");
    assert_eq!(
        (row(&listing, "fresh")["merged"].clone(), row(&listing, "ahead")["merged"].clone()),
        (json!(false), json!(false))
    );
    let total = &listing["merged_builds"];
    assert_eq!(total["count"], 1, "{listing}");
    let bytes = total["bytes"].as_u64().expect("bytes");
    assert!((300_000..600_000).contains(&bytes), "{bytes}");
    let without = m.listing(&json!({"sizes": false}));
    assert!(without.get("merged_builds").is_none(), "{without}");
}

#[test]
fn freeing_merged_builds_frees_only_the_merged_worktrees() {
    let mut m = merged_and_others();
    std::fs::write(m.merged.join("mine.txt"), "mine").expect("write");

    let answer = m.sweep(false);

    let entries = answer["worktrees"].as_array().expect("a list");
    assert_eq!(entries.len(), 1, "{answer}");
    assert_eq!(
        (entries[0]["outcome"].clone(), entries[0]["path"].clone()),
        (json!("freed"), json!(m.merged.display().to_string()))
    );
    assert!(answer["bytes"].as_u64() >= Some(300_000), "{answer}");
    assert!(!m.merged.join("node_modules").exists() && m.merged.join("mine.txt").exists());
    assert!(m.merged.join(".gitignore").exists() && m.s.has_branch("merged"));
    assert!(m.fresh.join("node_modules").exists() && m.ahead.join("node_modules").exists());
}

#[test]
fn a_check_deletes_nothing_and_says_freeable() {
    let mut m = merged_and_others();

    let answer = m.sweep(true);

    assert_eq!((answer["check"].clone(), answer["worktrees"][0]["outcome"].clone()), (json!(true), json!("freeable")));
    assert!(answer["bytes"].as_u64() >= Some(300_000), "{answer}");
    assert!(m.merged.join("node_modules").exists());
}

#[test]
fn a_merged_worktree_with_nothing_left_to_free_is_left_out() {
    let mut m = merged_and_others();
    let first = m.sweep(false);
    assert_eq!(first["worktrees"][0]["outcome"], "freed", "{first}");

    let again = m.sweep(false);

    assert_eq!((again["worktrees"].clone(), again["bytes"].clone()), (json!([]), json!(0)));
}

#[test]
fn a_worktree_being_freed_is_left_out() {
    let mut m = merged_and_others();
    let before = m.sweep(true);
    assert_eq!(before["worktrees"][0]["outcome"], "freeable", "{before}");
    assert_eq!(before["worktrees"][0]["path"], m.merged.display().to_string());
    m.s.app.git.frees.freeing(m.merged.clone());

    let answer = m.sweep(false);

    assert_eq!(answer["worktrees"], json!([]), "{answer}");
    assert!(m.merged.join("node_modules").exists());
}

#[test]
fn an_unknown_project_is_refused_and_nothing_is_deleted() {
    let mut m = merged_and_others();
    temp_only(&m.s);

    let refusal = refused(&mut m.s.app, "free-merged-builds", &json!({"project": 9999, "check": false}));

    assert_eq!(refusal.code, "not-found");
    assert!(m.merged.join("node_modules").exists());
}

#[cfg(unix)]
#[test]
fn a_program_in_a_merged_worktree_keeps_its_builds_and_the_rest_are_freed() {
    let mut m = merged_and_others();
    let other = m.s.add("other-merged");
    give_upstream(m.s.repo.path(), "other-merged");
    with_builds(&other);
    let sleeper =
        crate::test_util::Running::spawn(std::process::Command::new("/bin/sleep").arg("30").current_dir(&m.merged));
    let at = m.merged.clone();
    let in_folder = || process::cwd(sleeper.pid()).as_deref() == Some(at.as_path());
    wait_until("sleep works in the folder", in_folder);

    let answer = m.sweep(false);

    let entries = answer["worktrees"].as_array().expect("a list");
    let kept = entries.iter().find(|e| e["path"] == m.merged.display().to_string()).expect("kept");
    assert_eq!(kept["outcome"], "refused", "{answer}");
    assert!(kept["why"].as_str().is_some_and(|why| why.contains("sleep")), "{answer}");
    assert!(m.merged.join("node_modules").exists());
    let freed = entries.iter().find(|e| e["path"] == other.display().to_string()).expect("freed");
    assert_eq!(freed["outcome"], "freed", "{answer}");
    assert!(!other.join("node_modules").exists());
}
