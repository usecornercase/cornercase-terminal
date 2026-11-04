use super::*;
use crate::test_util::{TempDir, clone_of, git, git_repo, head};

#[test]
fn starts_from_the_newest_commit_of_the_remote() {
    let origin = git_repo(&[("a", "1")]);
    let (_keep, clone) = clone_of(origin.path());
    std::fs::write(origin.path().join("b"), "2").expect("write");
    git(origin.path(), &["add", "--all"]);
    git(origin.path(), &["commit", "--quiet", "-m", "second"]);
    let target = TempDir::new();
    let path = target.path().join("run");
    create(&clone, "routine/x", &path).expect("create");
    assert_eq!(head(&path), head(origin.path()));
    assert_ne!(head(&clone), head(&path));
}

#[test]
fn uses_the_local_default_branch_without_a_remote() {
    let repo = git_repo(&[("a", "1")]);
    let target = TempDir::new();
    let path = target.path().join("run");
    create(repo.path(), "routine/x", &path).expect("create");
    assert_eq!(head(&path), head(repo.path()));
}

#[test]
fn fails_with_the_reason_when_the_fetch_fails() {
    let origin = git_repo(&[("a", "1")]);
    let (_keep, clone) = clone_of(origin.path());
    git(&clone, &["remote", "set-url", "origin", "/nonexistent/folder"]);
    let target = TempDir::new();
    let error = create(&clone, "routine/x", &target.path().join("run")).expect_err("fetch");
    assert!(error.to_string().contains("could not fetch origin"), "{error}");
}

#[test]
fn falls_back_to_origin_main_without_a_remote_head() {
    let origin = git_repo(&[("a", "1")]);
    let (_keep, clone) = clone_of(origin.path());
    git(&clone, &["remote", "set-head", "origin", "--delete"]);
    let target = TempDir::new();
    let path = target.path().join("run");
    create(&clone, "routine/x", &path).expect("create");
    assert_eq!(head(&path), head(origin.path()));
}
