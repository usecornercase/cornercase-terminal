use std::fs;
#[cfg(unix)]
use std::os::unix::fs::{PermissionsExt, symlink};
#[cfg(unix)]
use std::time::{Duration, SystemTime};

#[cfg(unix)]
use rstest::rstest;

#[cfg(unix)]
use super::{MAX_FILES, ensure, list, remove, valid};
use super::{TASK, discard, ensure_task, make, write_task};
use crate::test_util::TempDir;

#[cfg(unix)]
const DAY: &str = "2025-10-09";

#[cfg(unix)]
fn mode(path: &std::path::Path) -> u32 {
    fs::metadata(path).expect("metadata").permissions().mode() & 0o777
}

#[cfg(unix)]
fn touch(path: &std::path::Path, age: u64) {
    fs::create_dir_all(path.parent().expect("parent")).expect("parents");
    let file = fs::File::create(path).expect("create");
    file.set_modified(SystemTime::now() - Duration::from_secs(age)).expect("set the time");
}

mod days {
    use super::*;

    fn name(dir: &std::path::Path) -> Option<&str> {
        dir.file_name().and_then(|n| n.to_str())
    }

    #[test]
    fn a_folder_takes_the_day_it_is_given() {
        let temp = TempDir::new();
        let dir = make(&temp.path().join("runs"), std::path::Path::new("/w/api"), "2026-11-13").expect("make");
        assert_eq!(name(&dir), Some("2026-11-13-api"));
    }

    #[test]
    fn the_number_counts_runs_of_the_same_day_only() {
        let temp = TempDir::new();
        let (root, path) = (temp.path().join("runs"), std::path::Path::new("/w/api"));
        let day = |day| make(&root, path, day).expect("make");
        assert_eq!(name(&day("2026-11-12")), Some("2026-11-12-api"));
        assert_eq!(name(&day("2026-11-13")), Some("2026-11-13-api"));
        assert_eq!(name(&day("2026-11-13")), Some("2026-11-13-api-2"));
    }
}

#[cfg(unix)]
mod naming {
    use super::*;

    #[test]
    fn a_folder_is_private_and_named_after_the_day_and_the_workspace() {
        let temp = TempDir::new();
        let dir = make(&temp.path().join("runs"), std::path::Path::new("/w/Fix Login"), DAY).expect("make");
        assert_eq!(dir.file_name().and_then(|n| n.to_str()), Some("2025-10-09-fix-login"));
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(dir.parent().expect("root")), 0o700);
    }

    #[test]
    fn a_second_run_the_same_day_gets_a_number() {
        let temp = TempDir::new();
        let (root, path) = (temp.path().join("runs"), std::path::Path::new("/w/api"));
        make(&root, path, DAY).expect("first");
        let second = make(&root, path, DAY).expect("second");
        assert_eq!(second.file_name().and_then(|n| n.to_str()), Some("2025-10-09-api-2"));
    }

    #[test]
    fn a_workspace_without_a_name_is_a_run() {
        let temp = TempDir::new();
        let dir = make(temp.path(), std::path::Path::new("/"), DAY).expect("make");
        assert_eq!(dir.file_name().and_then(|n| n.to_str()), Some("2025-10-09-run"));
    }

    #[rstest]
    #[case::plain("2025-10-09-api", true)]
    #[case::parent("..", false)]
    #[case::current(".", false)]
    #[case::nested("a/b", false)]
    #[case::absolute("/x", false)]
    #[case::empty("", false)]
    #[case::control("a\nb", false)]
    fn a_stored_name_is_one_plain_folder(#[case] name: &str, #[case] ok: bool) {
        assert_eq!(valid(name), ok);
    }
}

#[cfg(unix)]
mod ensuring {
    use super::*;

    #[test]
    fn a_missing_folder_is_made_again() {
        let temp = TempDir::new();
        let dir = temp.path().join("runs").join("x");
        assert!(ensure(&dir).expect("made"));
        assert_eq!(mode(&dir), 0o700);
        assert!(!ensure(&dir).expect("kept"));
    }

    #[test]
    fn an_existing_folder_with_a_loose_mode_becomes_private() {
        let temp = TempDir::new();
        let dir = temp.path().join("x");
        fs::create_dir(&dir).expect("dir");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("loosen");
        assert!(!ensure(&dir).expect("kept"));
        assert_eq!(mode(&dir), 0o700);
    }

    #[test]
    fn a_link_is_refused_and_not_followed() {
        let temp = TempDir::new();
        let (target, link) = (temp.path().join("target"), temp.path().join("link"));
        fs::create_dir(&target).expect("target");
        symlink(&target, &link).expect("link");
        assert!(ensure(&link).is_err());
    }
}

#[cfg(unix)]
mod listing {
    use super::*;

    #[test]
    fn files_come_newest_first_with_their_relative_names() {
        let temp = TempDir::new();
        touch(&temp.path().join("old.md"), 300);
        touch(&temp.path().join("sub/new.md"), 10);
        let listing = list(temp.path()).expect("list");
        let names: Vec<&str> = listing.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["sub/new.md", "old.md"]);
        assert!(!listing.more);
    }

    #[test]
    fn a_link_to_an_outside_folder_is_not_followed() {
        let (inside, outside) = (TempDir::new(), TempDir::new());
        touch(&outside.path().join("secret.md"), 0);
        symlink(outside.path(), inside.path().join("out")).expect("link");
        symlink(outside.path().join("secret.md"), inside.path().join("file")).expect("link");
        assert_eq!(list(inside.path()).expect("list").files, []);
    }

    #[test]
    fn a_long_list_is_cut_and_says_so() {
        let temp = TempDir::new();
        for i in 0..=MAX_FILES {
            touch(&temp.path().join(format!("f{i}.md")), 0);
        }
        let listing = list(temp.path()).expect("list");
        assert_eq!((listing.files.len(), listing.more), (MAX_FILES, true));
    }

    #[test]
    fn a_missing_folder_is_an_error() {
        assert!(list(&TempDir::new().path().join("gone")).is_err());
    }
}

#[cfg(unix)]
mod removing {
    use super::*;

    #[test]
    fn it_counts_what_it_removes() {
        let temp = TempDir::new();
        let dir = temp.path().join("run");
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join("a.md"), "abc").expect("write");
        fs::write(dir.join("b.md"), "de").expect("write");
        assert_eq!(remove(&dir).expect("remove"), (2, 5));
        assert!(!dir.exists());
    }

    #[test]
    fn a_missing_folder_is_nothing_to_remove() {
        assert_eq!(remove(&TempDir::new().path().join("gone")).expect("remove"), (0, 0));
    }
}

mod the_task {
    use super::*;

    #[test]
    fn it_is_written_with_the_exact_bytes() {
        let temp = TempDir::new();
        let task = "  Fix the login form\r\n\nKeep the API, \u{e9}\u{1f600}.\n\n";
        write_task(temp.path(), task).expect("write");
        assert_eq!(fs::read(temp.path().join(TASK)).expect("read"), task.as_bytes());
    }

    #[test]
    fn an_existing_file_is_never_overwritten() {
        let temp = TempDir::new();
        fs::write(temp.path().join(TASK), "mine").expect("write");
        let e = write_task(temp.path(), "theirs").expect_err("exists");
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
        assert!(!ensure_task(temp.path(), "theirs").expect("ensure"));
        assert_eq!(fs::read_to_string(temp.path().join(TASK)).expect("read"), "mine");
    }

    #[test]
    fn a_missing_file_is_written_again() {
        let temp = TempDir::new();
        assert!(ensure_task(temp.path(), "the task").expect("ensure"));
        assert_eq!(fs::read_to_string(temp.path().join(TASK)).expect("read"), "the task");
    }

    #[test]
    fn discarding_removes_the_task_and_an_empty_folder_only() {
        let temp = TempDir::new();
        let alone = temp.path().join("alone");
        fs::create_dir(&alone).expect("dir");
        write_task(&alone, "x").expect("write");
        discard(&alone);
        assert!(!alone.exists());

        let busy = temp.path().join("busy");
        fs::create_dir(&busy).expect("dir");
        write_task(&busy, "x").expect("write");
        fs::write(busy.join("coder-1.md"), "answer").expect("answer");
        discard(&busy);
        assert!(!busy.join(TASK).exists());
        assert!(busy.join("coder-1.md").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_named_task_is_not_written_through() {
        let temp = TempDir::new();
        let outside = temp.path().join("outside.txt");
        fs::write(&outside, "outside").expect("write");
        let dir = temp.path().join("run");
        fs::create_dir(&dir).expect("dir");
        symlink(&outside, dir.join(TASK)).expect("link");
        assert!(!ensure_task(&dir, "the task").expect("ensure"));
        assert_eq!(fs::read_to_string(&outside).expect("read"), "outside");
    }
}
