use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("list")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn installed(tmp: &TempDir) -> (PathBuf, PathBuf) {
    let exe = tmp.path().join("cornercase.exe");
    let new = tmp.path().join("new.exe");
    fs::write(&exe, "old").expect("old binary");
    fs::write(&new, "new").expect("new binary");
    (exe, new)
}

mod naming {
    use super::*;

    #[rstest]
    #[case::windows("cornercase.exe", 0, "cornercase.old.exe")]
    #[case::windows_spare("cornercase.exe", 2, "cornercase.old.2.exe")]
    #[case::unix("cornercase", 0, "cornercase.old")]
    #[case::unix_spare("cornercase", 1, "cornercase.old.1")]
    fn an_old_binary_keeps_the_extension_last(#[case] exe: &str, #[case] n: u32, #[case] expected: &str) {
        assert_eq!(Names::of(Path::new(exe)).expect("a name").old(n), expected);
    }

    #[rstest]
    #[case::old("cornercase.old.exe", true)]
    #[case::spare("cornercase.old.12.exe", true)]
    #[case::itself("cornercase.exe", false)]
    #[case::another_word("cornercase.older.exe", false)]
    #[case::no_number("cornercase.old..exe", false)]
    #[case::not_a_number("cornercase.old.x.exe", false)]
    #[case::another_extension("cornercase.old.txt", false)]
    #[case::another_program("other.old.exe", false)]
    fn only_old_copies_of_the_binary_count(#[case] name: &str, #[case] old: bool) {
        assert_eq!(Names::of(Path::new("cornercase.exe")).expect("a name").is_old(name), old);
    }
}

mod aside {
    use super::*;

    fn assert_swapped(tmp: &TempDir, exe: &Path) {
        assert_eq!(fs::read_to_string(exe).expect("new binary"), "new");
        assert_eq!(fs::read_to_string(tmp.path().join("cornercase.old.exe")).expect("old binary"), "old");
        assert_eq!(names(tmp.path()), ["cornercase.exe", "cornercase.old.exe"]);
    }

    #[test]
    fn the_new_binary_takes_the_name_and_the_old_one_is_set_aside() {
        let tmp = TempDir::new();
        let (exe, new) = installed(&tmp);

        swap_aside(&new, &exe).expect("swap");

        assert_swapped(&tmp, &exe);
    }

    #[test]
    fn an_old_copy_left_behind_is_removed_first() {
        let tmp = TempDir::new();
        let (exe, new) = installed(&tmp);
        fs::write(tmp.path().join("cornercase.old.exe"), "older").expect("a leftover");
        fs::write(tmp.path().join("cornercase.old.3.exe"), "older").expect("a leftover");

        swap_aside(&new, &exe).expect("swap");

        assert_swapped(&tmp, &exe);
    }

    #[test]
    fn the_old_binary_comes_back_when_the_new_one_cannot_take_its_place() {
        let tmp = TempDir::new();
        let (exe, _) = installed(&tmp);

        let result = swap_aside(&tmp.path().join("missing.exe"), &exe);

        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&exe).expect("old binary"), "old");
        assert_eq!(names(tmp.path()), ["cornercase.exe", "new.exe"]);
    }

    #[test]
    fn removing_old_copies_leaves_everything_else() {
        let tmp = TempDir::new();
        for name in
            ["cornercase.exe", "cornercase.old.exe", "cornercase.old.1.exe", "cornercase.older.exe", "other.exe"]
        {
            fs::write(tmp.path().join(name), name).expect("a file");
        }

        remove_old(&tmp.path().join("cornercase.exe"));

        assert_eq!(names(tmp.path()), ["cornercase.exe", "cornercase.older.exe", "other.exe"]);
    }
}

#[cfg(windows)]
mod on_windows {
    use std::process::{Command, Stdio};

    use super::*;
    use crate::test_util::{Running, verbatim, wait_until};

    fn run(exe: &Path) -> Running {
        Running::spawn(Command::new(exe).args(["-n", "30", "127.0.0.1"]).stdout(Stdio::null()))
    }

    fn ping_as(path: &Path) {
        let system = std::env::var_os("SystemRoot").expect("SystemRoot");
        fs::copy(Path::new(&system).join("System32").join("PING.EXE"), path).expect("copy ping");
    }

    #[test]
    fn a_running_binary_is_set_aside_and_removed_once_it_ends() {
        let tmp = TempDir::new();
        let dir = verbatim(tmp.path());
        let exe = dir.join("cornercase.exe");
        ping_as(&exe);
        let running = run(&exe);
        assert!(fs::remove_file(&exe).is_err(), "a running binary cannot be removed");
        fs::write(dir.join("new.exe"), "new").expect("new binary");

        swap_aside(&dir.join("new.exe"), &exe).expect("swap");
        remove_old(&exe);

        assert_eq!(fs::read_to_string(&exe).expect("new binary"), "new");
        assert_eq!(names(&dir), ["cornercase.exe", "cornercase.old.exe"]);
        drop(running);
        wait_until("the old copy is removed", || {
            remove_old(&exe);
            names(&dir) == ["cornercase.exe"]
        });
    }

    #[test]
    fn a_second_update_while_the_first_old_copy_runs_takes_a_spare_name() {
        let tmp = TempDir::new();
        let exe = tmp.path().join("cornercase.exe");
        ping_as(&tmp.path().join("cornercase.old.exe"));
        let running = run(&tmp.path().join("cornercase.old.exe"));
        fs::write(&exe, "second").expect("second binary");
        fs::write(tmp.path().join("new.exe"), "third").expect("third binary");

        swap_aside(&tmp.path().join("new.exe"), &exe).expect("swap");

        assert_eq!(fs::read_to_string(&exe).expect("third binary"), "third");
        assert_eq!(fs::read_to_string(tmp.path().join("cornercase.old.1.exe")).expect("second binary"), "second");
        drop(running);
    }
}
