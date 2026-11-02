#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};

use rstest::rstest;

use super::*;
use crate::test_util::{Checkouts, git};

const TWO_CONFLICTS: &[u8] = b"TREE\0f.txt\0g.txt\0\x001\0f.txt\0Auto-merging\0Auto-merging f.txt\n\0\
1\0f.txt\0CONFLICT (contents)\0CONFLICT (content): Merge conflict in f.txt\n\0\
1\0g.txt\0CONFLICT (modify/delete)\0CONFLICT (modify/delete): g.txt deleted in b and modified in a.  \
Version a of g.txt left in tree.\n\0";

fn conflict(path: &str, kind: &str, message: &str) -> Conflict {
    Conflict { path: path.into(), kind: kind.into(), message: message.into() }
}

#[rstest]
#[case::a_content_and_a_modify_delete_conflict(
    TWO_CONFLICTS,
    vec![
        conflict("f.txt", "contents", "CONFLICT (content): Merge conflict in f.txt"),
        conflict(
            "g.txt",
            "modify/delete",
            "CONFLICT (modify/delete): g.txt deleted in b and modified in a.  Version a of g.txt left in tree.",
        ),
    ],
)]
#[case::a_clean_merge(b"TREE\0", vec![])]
#[case::a_path_no_record_names(b"TREE\0f.txt\0\0", vec![conflict("f.txt", "conflict", "")])]
fn parse_reads_what_merge_tree_prints(#[case] out: &[u8], #[case] expected: Vec<Conflict>) {
    assert_eq!(parse(out), expected);
}

fn write(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).expect("write file");
}

fn pair(dir: &Path, other: &Path) -> Check {
    Check { dir: dir.to_path_buf(), base: None, with_base: false, others: vec![(2, other.to_path_buf())] }
}

fn into_base(dir: &Path) -> Check {
    Check { dir: dir.to_path_buf(), base: None, with_base: true, others: Vec::new() }
}

fn merged(checked: &Checked) -> &Merged {
    checked.merges[0].result.as_ref().expect("the check answers")
}

const LINES: &str = "one\ntwo\nthree\n";

fn two_editing(first_line: &str, second_line: &str) -> (Checkouts, PathBuf, PathBuf) {
    let mut project = Checkouts::new(&[("a", LINES)]);
    let (first, second) = (project.add("w1", "main"), project.add("w2", "main"));
    write(&first, "a", &format!("one\n{first_line}\nthree\n"));
    write(&second, "a", &format!("one\n{second_line}\nthree\n"));
    (project, first, second)
}

mod merging_two_workspaces {
    use super::*;

    #[test]
    fn names_a_conflict_in_uncommitted_edits() {
        let (_project, first, second) = two_editing("first", "second");

        let checked = check(&pair(&first, &second));

        let found = merged(&checked);
        assert_eq!(checked.merges[0].with, With::Workspace(2));
        assert_eq!((found.conflicts.len(), found.conflicts[0].path.as_str()), (1, "a"));
        assert_eq!(found.conflicts[0].kind, "contents");
    }

    #[test]
    fn merges_different_lines_of_one_file_cleanly() {
        let mut project = Checkouts::new(&[("a", "one\ntwo\nthree\nfour\nfive\nsix\nseven\n")]);
        let (first, second) = (project.add("w1", "main"), project.add("w2", "main"));
        write(&first, "a", "ONE\ntwo\nthree\nfour\nfive\nsix\nseven\n");
        write(&second, "a", "one\ntwo\nthree\nfour\nfive\nsix\nSEVEN\n");

        assert_eq!(merged(&check(&pair(&first, &second))).conflicts, Vec::<Conflict>::new());
    }

    #[test]
    fn names_an_add_add_conflict_in_new_files() {
        let mut project = Checkouts::new(&[("a", LINES)]);
        let (first, second) = (project.add("w1", "main"), project.add("w2", "main"));
        write(&first, "new", "first\n");
        write(&second, "new", "second\n");

        let checked = check(&pair(&first, &second));

        assert_eq!(merged(&checked).conflicts.iter().map(|c| c.kind.as_str()).collect::<Vec<_>>(), ["add/add"]);
    }

    #[test]
    fn answers_for_a_repository_without_an_identity() {
        let (project, first, second) = two_editing("first", "second");
        for key in ["user.name", "user.email"] {
            git(project.repo.path(), &["config", key, ""]);
        }
        git(project.repo.path(), &["config", "user.useConfigOnly", "true"]);

        assert_eq!(merged(&check(&pair(&first, &second))).conflicts.len(), 1);
    }
}

mod merging_into_the_base {
    use super::*;

    #[test]
    fn names_a_conflict_with_a_commit_the_base_gained() {
        let mut project = Checkouts::new(&[("a", LINES)]);
        let first = project.add("w1", "main");
        write(&first, "a", "one\nfirst\nthree\n");
        write(project.repo.path(), "a", "one\nmain\nthree\n");
        git(project.repo.path(), &["commit", "--quiet", "-am", "main edits"]);

        let checked = check(&into_base(&first));

        assert_eq!(checked.merges[0].with, With::Base(Some("main".into())));
        assert_eq!(merged(&checked).conflicts.len(), 1);
    }

    #[test]
    fn merges_cleanly_when_the_base_changed_other_files() {
        let mut project = Checkouts::new(&[("a", LINES), ("b", LINES)]);
        let first = project.add("w1", "main");
        write(&first, "a", "one\nfirst\nthree\n");
        write(project.repo.path(), "b", "one\nmain\nthree\n");
        git(project.repo.path(), &["commit", "--quiet", "-am", "main edits"]);

        assert_eq!(merged(&check(&into_base(&first))).conflicts, Vec::<Conflict>::new());
    }

    #[test]
    fn leaves_out_an_untracked_file_over_a_megabyte() {
        let mut project = Checkouts::new(&[("a", LINES)]);
        let first = project.add("w1", "main");
        std::fs::write(first.join("big"), vec![b'x'; usize::try_from(MAX_UNTRACKED_BYTES + 1).expect("fits")])
            .expect("write big");
        write(&first, "small", "kept\n");

        let checked = check(&into_base(&first));

        assert_eq!((merged(&checked).left_out, merged(&checked).conflicts.len()), (1, 0));
    }

    #[test]
    fn says_so_when_the_repository_has_no_base_branch() {
        let mut project = Checkouts::new(&[("a", LINES)]);
        git(project.repo.path(), &["branch", "-m", "main", "trunk"]);
        let first = project.add("w1", "trunk");

        let checked = check(&into_base(&first));

        assert_eq!(checked.merges[0].with, With::Base(None));
        assert_eq!(checked.merges[0].result, Err("no base branch to merge into".to_string()));
    }

    #[test]
    fn names_a_base_that_does_not_exist() {
        let mut project = Checkouts::new(&[("a", LINES)]);
        let first = project.add("w1", "main");
        let asked = Check { base: Some("nope".into()), ..into_base(&first) };

        assert_eq!(check(&asked).merges[0].result, Err("no branch nope".to_string()));
    }
}

fn git_output(dir: &Path, args: &[&str]) -> Vec<u8> {
    let out = Command::new("git").arg("-C").arg(dir).arg("--no-optional-locks").args(args).output().expect("run git");
    [out.stdout, out.stderr].concat()
}

fn index_of(dir: &Path) -> Vec<u8> {
    let git_dir = git_output(dir, &["rev-parse", "--absolute-git-dir"]);
    let git_dir = String::from_utf8(git_dir).expect("utf-8");
    std::fs::read(Path::new(git_dir.trim()).join("index")).expect("read the index")
}

fn state_of(dirs: &[&Path], repo: &Path) -> Vec<Vec<u8>> {
    let mut state = Vec::new();
    for dir in dirs {
        state.push(git_output(dir, &["status", "--porcelain=v2", "-z", "--untracked-files=all"]));
        state.push(index_of(dir));
    }
    state.push(git_output(repo, &["for-each-ref"]));
    state.push(git_output(repo, &["count-objects", "-v"]));
    state
}

#[test]
fn nothing_in_the_repository_changes() {
    let (project, first, second) = two_editing("first", "second");
    write(&first, "new", "untracked\n");
    write(&second, "new", "other\n");
    git(&second, &["add", "a"]);
    let before = state_of(&[&first, &second], project.repo.path());

    let checked = check(&Check { with_base: true, ..pair(&first, &second) });

    assert!(checked.merges.iter().all(|merge| merge.result.is_ok()), "{checked:?}");
    assert_ne!(merged(&Checked { merges: vec![checked.merges[1].clone()] }).conflicts, Vec::<Conflict>::new());
    assert_eq!(state_of(&[&first, &second], project.repo.path()), before);
}

#[test]
fn the_scratch_folder_is_new_and_private() {
    let root = crate::test_util::TempDir::new();
    let taken = root.path().join(format!("cornercase-merge-{}-5", std::process::id()));
    std::fs::create_dir(&taken).expect("a folder already there");
    let counter = AtomicU64::new(5);

    let scratch = Scratch::under(root.path(), &counter).expect("a scratch folder");

    let mode = std::fs::metadata(&scratch.dir).expect("metadata").permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
    assert_ne!(scratch.dir, taken);
    assert_eq!(std::fs::read_dir(&taken).map_or(0, Iterator::count), 0);
}

#[rstest]
#[case::plain("/repo/.git/objects", "/repo/.git/objects")]
#[case::a_colon("/a:b/.git/objects", "\"/a:b/.git/objects\"")]
#[case::a_quote("/a\"b/objects", "\"/a\\\"b/objects\"")]
fn the_alternates_are_quoted_as_git_reads_them(#[case] path: &str, #[case] expected: &str) {
    assert_eq!(quoted(Path::new(path)), OsString::from(expected));
}

#[test]
fn a_repository_whose_path_has_a_colon_is_checked() {
    let tmp = crate::test_util::TempDir::new();
    let repo = tmp.path().join("a:b");
    std::fs::create_dir(&repo).expect("create the repository folder");
    git(&repo, &["init", "--quiet"]);
    write(&repo, "a", LINES);
    git(&repo, &["add", "--all"]);
    git(&repo, &["commit", "--quiet", "-m", "init"]);
    let (first, second) = (tmp.path().join("w1"), tmp.path().join("w2"));
    for (branch, tree) in [("w1", &first), ("w2", &second)] {
        git(&repo, &["worktree", "add", "--quiet", "-b", branch, tree.to_str().expect("utf-8")]);
    }
    write(&first, "a", "one\nfirst\nthree\n");
    write(&second, "a", "one\nsecond\nthree\n");

    let checked = check(&pair(&first, &second));

    assert_eq!(merged(&checked).conflicts.len(), 1);
}

#[test]
fn the_scratch_folder_goes_away_with_the_check() {
    let scratch = Scratch::new().expect("a scratch folder");
    let dir = scratch.dir.clone();
    assert!(dir.join("objects").is_dir());

    drop(scratch);

    assert!(!dir.exists());
}

fn failed_with(code: i32, stderr: &str) -> Output {
    Output { status: ExitStatus::from_raw(code << 8), stdout: Vec::new(), stderr: stderr.as_bytes().to_vec() }
}

#[rstest]
#[case::a_usage_error(failed_with(129, "usage: git merge-tree [<options>]"))]
#[case::an_unknown_option(failed_with(128, "error: unknown option `write-tree'"))]
fn an_old_git_is_named(#[case] out: Output) {
    assert_eq!(failure(out).to_string(), "the merge check needs git 2.38 or later");
}

#[test]
fn another_failure_keeps_gits_own_message() {
    assert_eq!(
        failure(failed_with(128, "fatal: not a valid object name abc")).to_string(),
        "not a valid object name abc"
    );
}
