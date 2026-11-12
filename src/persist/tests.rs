use super::*;
use crate::test_util::TempDir;

mod back_up {
    use super::*;

    #[test]
    fn a_second_copy_keeps_the_first_one() {
        let tmp = TempDir::new();
        let path = tmp.path().join("session.json");
        std::fs::write(&path, "first").expect("write the session");
        back_up(&path).expect("first copy");
        std::fs::write(&path, "second").expect("write the session again");

        let backup = back_up(&path).expect("second copy");

        let read = |name: &str| std::fs::read_to_string(tmp.path().join(name)).ok();
        assert_eq!(backup, tmp.path().join("session.json.bak"));
        assert_eq!(
            (read("session.json.bak"), read("session.json.bak.1")),
            (Some("second".into()), Some("first".into()))
        );
    }
}

mod mark {
    use super::*;

    #[test]
    fn a_write_returns_the_mark_the_file_has_afterwards() {
        let tmp = TempDir::new();
        let path = tmp.path().join("config.json");
        let written = write_marked(&path, &"text", false).expect("write");
        assert_eq!((written.is_some(), written), (true, mark(&path)));
    }

    #[test]
    fn another_write_changes_it() {
        let tmp = TempDir::new();
        let path = tmp.path().join("config.json");
        let first = write_marked(&path, &"short", false).expect("write");
        let second = write_marked(&path, &"a longer text", false).expect("write");
        assert_ne!(first, second);
    }

    #[test]
    fn a_missing_file_has_none() {
        assert_eq!(mark(Path::new("/nonexistent/x.json")), None);
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read the folder")
        .map(|entry| entry.expect("read an entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

mod writing {
    use std::sync::Barrier;

    use rstest::rstest;

    use super::*;

    const WRITERS: u8 = 8;
    const WRITES: usize = 100;

    fn text_of(writer: u8) -> String {
        char::from(b'a' + writer).to_string().repeat(1000 * usize::from(writer + 1))
    }

    fn failures(path: &Path, writer: u8, start: &Barrier) -> usize {
        start.wait();
        let text = text_of(writer);
        (0..WRITES).filter(|_| write(path, &text, false).is_err()).count()
    }

    fn write_at_once(path: &Path) -> usize {
        let start = Barrier::new(WRITERS.into());
        let start = &start;
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..WRITERS).map(|w| scope.spawn(move || failures(path, w, start))).collect();
            handles.into_iter().map(|handle| handle.join().expect("a writer panicked")).sum()
        })
    }

    #[test]
    fn writers_of_one_file_at_once_all_succeed_and_leave_one_whole_file() {
        let tmp = TempDir::new();
        let path = tmp.path().join("machines.json");

        let failed = write_at_once(&path);

        let text: String = serde_json::from_slice(&std::fs::read(&path).expect("read the file")).expect("whole JSON");
        assert_eq!(failed, 0);
        assert!((0..WRITERS).any(|w| text == text_of(w)));
        assert_eq!(names(tmp.path()), ["machines.json"]);
    }

    #[rstest]
    #[case::public(false)]
    #[case::private(true)]
    fn a_write_that_fails_leaves_no_temporary_file(#[case] private: bool) {
        let tmp = TempDir::new();
        let path = tmp.path().join("config.json");
        std::fs::create_dir(&path).expect("make a folder of the file's name");
        std::fs::write(path.join("kept"), "kept").expect("fill it");

        assert!(write(&path, &"text", private).is_err());

        assert_eq!(names(tmp.path()), ["config.json"]);
        assert_eq!(std::fs::read_to_string(path.join("kept")).expect("read kept"), "kept");
    }

    #[test]
    fn a_temporary_name_a_crash_left_is_skipped() {
        let tmp = TempDir::new();
        let path = tmp.path().join("machines.json");
        let left = temporary(&path, 0).expect("a name");
        std::fs::write(&left, "left by a crash").expect("leave a file");

        let (made, file) = create_temporary(&path, Kind::Public, &AtomicU64::new(0)).expect("create");
        drop(file);

        let name = made.file_name().expect("a name").to_string_lossy().into_owned();
        assert_ne!(made, left);
        assert_eq!(made.parent(), Some(tmp.path()));
        assert!(name.starts_with("machines.json.") && Path::new(&name).extension().is_some_and(|ext| ext == "tmp"));
        assert_eq!(std::fs::read_to_string(&left).expect("read it"), "left by a crash");
    }

    #[cfg(unix)]
    #[test]
    fn a_private_file_is_the_owner_s_alone_and_a_public_one_follows_the_umask() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new();
        let (private, public, plain) =
            (tmp.path().join("a.json"), tmp.path().join("b.json"), tmp.path().join("c.json"));
        write(&private, &"x", true).expect("private");
        write(&public, &"x", false).expect("public");
        std::fs::write(&plain, "x").expect("plain");

        let mode = |path: &Path| std::fs::metadata(path).expect("stat").permissions().mode() & 0o777;
        assert_eq!((mode(&private), mode(&public)), (0o600, mode(&plain)));
    }
}
