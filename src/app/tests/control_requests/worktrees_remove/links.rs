use super::second_look::after_the_look;
use super::*;
use crate::test_util::link_folder;

fn a_precious_folder() -> TempDir {
    let precious = TempDir::new();
    std::fs::write(precious.path().join("precious"), "x").expect("write");
    precious
}

#[test]
fn a_link_to_a_listed_folder_removes_that_folder_and_leaves_the_link() {
    let (mut s, at) = with_one("a");
    let elsewhere = TempDir::new();
    let link = elsewhere.path().join("link");
    link_folder(&at, &link);

    let folder = s.removes(&json!({"paths": [link]}));

    assert_eq!(folder["path"], json!(at));
    assert!(!at.exists());
    assert!(std::fs::symlink_metadata(&link).is_ok_and(|meta| meta.file_type().is_symlink()), "the link went too");
}

#[test]
fn a_link_to_a_folder_that_is_not_listed_is_refused_and_its_target_stays() {
    let (mut s, _) = with_one("a");
    let precious = a_precious_folder();
    let elsewhere = TempDir::new();
    let link = elsewhere.path().join("link");
    link_folder(precious.path(), &link);

    let folder = s.one(&json!({"paths": [link], "force": true, "unlock": true}));

    assert_eq!((outcome(&folder), &folder["path"]), ("refused", &json!(precious.path())));
    assert!(precious.path().join("precious").exists());
}

#[test]
fn a_link_in_the_worktrees_folder_to_something_like_a_leftover_is_never_a_target() {
    let mut s = closed_repo();
    let outside = a_precious_folder();
    std::fs::write(outside.path().join(".git"), "gitdir: /nonexistent/repo/.git/worktrees/x\n").expect("dot git");
    let parent = s.worktrees.path().join("gone");
    std::fs::create_dir_all(&parent).expect("mkdir");
    let link = parent.join("linked");
    link_folder(outside.path(), &link);

    let answer = s.asked(&json!({"paths": [link, outside.path()], "force": true, "unlock": true}));

    assert_eq!(outcomes(&answer), ["refused", "refused"]);
    assert!(outside.path().join("precious").exists() && link.exists());
}

#[test]
fn a_link_inside_a_leftover_is_deleted_and_not_followed() {
    let mut s = closed_repo();
    let precious = a_precious_folder();
    let at = s.leftover_folder("/nonexistent/repo/.git/worktrees/x");
    link_folder(precious.path(), &at.join("link"));

    s.removes(&json!({"paths": [at], "force": true}));

    assert!(!at.exists() && precious.path().join("precious").exists());
}

#[test]
fn a_prunable_entry_named_through_a_linked_parent_is_found() {
    let (mut s, at) = with_one("a");
    s.add("stays");
    std::fs::remove_dir_all(&at).expect("remove");
    let elsewhere = TempDir::new();
    let parent = elsewhere.path().join("parent");
    link_folder(at.parent().expect("a parent"), &parent);

    let folder = s.removes(&json!({"paths": [parent.join("a")]}));

    assert_eq!(folder["path"], json!(at));
}

fn moved_away(at: &Path, elsewhere: &TempDir) -> PathBuf {
    let kept = elsewhere.path().join("kept");
    std::fs::rename(at, &kept).expect("move");
    link_folder(&kept, at);
    kept
}

#[test]
fn a_folder_swapped_for_a_link_after_the_look_is_refused_and_its_target_stays() {
    let (mut s, at) = with_one("a");
    let elsewhere = TempDir::new();
    let swapped = at.clone();

    let folder = after_the_look(&mut s, &json!({"paths": [at]}), || {
        moved_away(&swapped, &elsewhere);
    });

    assert_eq!(outcome(&folder), "refused");
    assert!(why(&folder).contains("changed since it was listed"), "{folder}");
    assert!(elsewhere.path().join("kept/README").exists());
}

#[test]
fn a_worktree_git_records_at_a_link_is_refused_and_the_links_target_stays() {
    let (mut s, at) = with_one("a");
    s.add("stays");
    let elsewhere = TempDir::new();
    let kept = moved_away(&at, &elsewhere);

    let answer = s.asked(&json!({"paths": [at, kept], "force": true, "unlock": true}));

    assert_eq!(outcomes(&answer), ["refused", "refused"]);
    assert!(why(&answer["folders"][0]).contains("git records it at the symbolic link"), "{answer}");
    assert!(kept.join("README").exists() && s.has_branch("a"));
}
