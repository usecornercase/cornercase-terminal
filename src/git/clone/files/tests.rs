#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, git_repo};

fn file(path: &str, contents: &str, mode: u32) -> File {
    File { path: PathBuf::from(path), bytes: contents.as_bytes().to_vec(), mode }
}

fn read(root: &TempDir, path: &str) -> String {
    fs::read_to_string(root.path().join(path)).expect("read")
}

#[cfg(unix)]
fn mode_of(root: &TempDir, path: &str) -> u32 {
    fs::metadata(root.path().join(path)).expect("metadata").permissions().mode() & PERMISSIONS
}

fn whys(unwritten: &[Unwritten]) -> Vec<(String, String)> {
    unwritten.iter().map(|file| (file.path.display().to_string(), file.why.clone())).collect()
}

mod write {
    use super::*;

    #[test]
    fn puts_each_file_at_its_path_making_its_folders() {
        let clone = git_repo(&[("README.md", "hi\n")]);

        let unwritten =
            write(clone.path(), &[file(".env", "KEY=1\n", 0o644), file("config/secrets.dev.json", "{}", 0o644)]);

        assert_eq!(unwritten, []);
        assert_eq!((read(&clone, ".env"), read(&clone, "config/secrets.dev.json")), ("KEY=1\n".into(), "{}".into()));
    }

    #[cfg(unix)]
    #[rstest]
    #[case::private(0o600)]
    #[case::executable(0o755)]
    #[case::open_to_all(0o666)]
    fn keeps_the_files_permissions(#[case] mode: u32) {
        let clone = TempDir::new();

        write(clone.path(), &[file("run.sh", "echo hi\n", mode)]);

        assert_eq!(mode_of(&clone, "run.sh"), mode);
    }

    #[cfg(unix)]
    #[test]
    fn keeps_only_the_permission_bits() {
        let clone = TempDir::new();

        write(clone.path(), &[file(".env", "KEY=1\n", 0o104_600)]);

        assert_eq!(mode_of(&clone, ".env"), 0o600);
        assert_eq!(fs::metadata(clone.path().join(".env")).expect("metadata").permissions().mode() & 0o7000, 0);
    }

    #[test]
    fn never_overwrites_a_file_of_the_clone() {
        let clone = git_repo(&[("config/app.json", "{\"tracked\": true}")]);

        let unwritten = write(clone.path(), &[file("config/app.json", "{}", 0o644), file(".env", "KEY=1", 0o600)]);

        assert_eq!(whys(&unwritten), [("config/app.json".into(), "it is already in the clone".into())]);
        assert_eq!(read(&clone, "config/app.json"), "{\"tracked\": true}");
        assert_eq!(read(&clone, ".env"), "KEY=1");
    }

    #[test]
    fn writes_the_first_of_two_files_with_the_same_path() {
        let clone = TempDir::new();

        let unwritten = write(clone.path(), &[file(".env", "first", 0o600), file(".env", "second", 0o600)]);

        assert_eq!(whys(&unwritten), [(".env".into(), "it is already in the clone".into())]);
        assert_eq!(read(&clone, ".env"), "first");
    }

    #[rstest]
    #[case::absolute("/etc/cron.d/job", "it is not relative to the project")]
    #[case::going_up("../outside.env", "it goes up with `..`")]
    #[case::going_up_inside("config/../../outside.env", "it goes up with `..`")]
    #[case::into_git("config/.git/hooks/pre-commit", "it goes into `.git`")]
    #[case::into_the_clones_git(".git/hooks/pre-commit", "it goes into `.git`")]
    #[case::into_git_in_capitals(".GIT/config", "it goes into `.git`")]
    #[case::empty("", "its path is empty")]
    #[case::only_a_dot(".", "its path is empty")]
    fn refuses_a_path_outside_the_clone(#[case] path: &str, #[case] why: &str) {
        let clone = git_repo(&[("README.md", "hi\n")]);

        let unwritten = write(clone.path(), &[file(path, "x", 0o644)]);

        assert_eq!(whys(&unwritten), [(path.into(), why.into())]);
        assert!(!clone.path().parent().expect("a parent").join("outside.env").exists());
        assert!(!clone.path().join(".git/hooks/pre-commit").exists());
    }

    #[cfg(unix)]
    #[test]
    fn never_goes_through_a_link_of_the_clone() {
        let outside = TempDir::new();
        let clone = TempDir::new();
        std::os::unix::fs::symlink(outside.path(), clone.path().join("config")).expect("symlink");

        let unwritten = write(clone.path(), &[file("config/secrets.json", "{}", 0o600)]);

        assert_eq!(whys(&unwritten), [("config/secrets.json".into(), "`config` is not a folder in the clone".into())]);
        assert!(!outside.path().join("secrets.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn never_writes_through_a_link_in_place_of_the_file() {
        let outside = TempDir::new();
        let clone = TempDir::new();
        std::os::unix::fs::symlink(outside.path().join("stolen"), clone.path().join(".env")).expect("symlink");

        let unwritten = write(clone.path(), &[file(".env", "KEY=1", 0o600)]);

        assert_eq!(whys(&unwritten), [(".env".into(), "it is already in the clone".into())]);
        assert!(!outside.path().join("stolen").exists());
    }

    #[test]
    fn says_when_a_folder_of_the_path_is_a_file() {
        let clone = git_repo(&[("config", "a file")]);

        let unwritten = write(clone.path(), &[file("config/secrets.json", "{}", 0o600)]);

        assert_eq!(whys(&unwritten), [("config/secrets.json".into(), "`config` is not a folder in the clone".into())]);
    }

    #[test]
    fn writes_an_empty_file() {
        let clone = TempDir::new();

        assert_eq!(write(clone.path(), &[file("logs/.keep", "", 0o644)]), []);
        assert_eq!(read(&clone, "logs/.keep"), "");
    }
}
