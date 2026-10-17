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
