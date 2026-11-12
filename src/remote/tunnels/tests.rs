use super::*;
use crate::test_util::TempDir;

fn kept_in(dir: &TempDir) -> PathBuf {
    dir.path().join("tunnels.json")
}

fn kept(port: u16, local: Option<u16>, pid: Option<i32>, closed: bool) -> Kept {
    Kept { port, local, pid, closed }
}

fn text(file: &Path) -> String {
    std::fs::read_to_string(file).expect("the file")
}

mod reading {
    use super::*;

    #[test]
    fn a_missing_file_or_a_top_level_that_is_not_an_object_is_empty() {
        let dir = TempDir::new();
        let file = kept_in(&dir);
        assert_eq!(read(&file, "devbox"), Vec::<Kept>::new());

        std::fs::write(&file, "[1, 2]").expect("write");

        assert_eq!(read(&file, "devbox"), Vec::<Kept>::new());
    }

    #[test]
    fn entries_come_per_destination_sorted_by_port() {
        let dir = TempDir::new();
        let file = kept_in(&dir);
        let json = r#"{"devbox":[{"port":9,"closed":true},{"port":3,"local":4,"pid":7}],"other":[{"port":1}]}"#;
        std::fs::write(&file, json).expect("write");

        assert_eq!(read(&file, "devbox"), [kept(3, Some(4), Some(7), false), kept(9, None, None, true)]);
        assert_eq!(read(&file, "other"), [kept(1, None, None, false)]);
        assert_eq!(read(&file, "third"), Vec::<Kept>::new());
    }

    #[test]
    fn broken_entries_port_zero_repeats_and_bad_destinations_are_skipped() {
        let dir = TempDir::new();
        let file = kept_in(&dir);
        let json = r#"{"devbox":[{"port":"x"},{"port":0},{"port":5,"local":6},{"port":5,"local":7},{"port":8}],
            "-bad":[{"port":1}],"words here":[{"port":1}],"notalist":3}"#;
        std::fs::write(&file, json).expect("write");

        assert_eq!(read(&file, "devbox"), [kept(5, Some(6), None, false), kept(8, None, None, false)]);
        assert_eq!(read(&file, "-bad"), Vec::<Kept>::new());
        assert_eq!(read(&file, "notalist"), Vec::<Kept>::new());
    }

    #[test]
    fn at_most_sixty_four_entries_are_kept_per_destination() {
        let dir = TempDir::new();
        let file = kept_in(&dir);
        let entries: Vec<String> = (1..=80).map(|port| format!(r#"{{"port":{port}}}"#)).collect();
        std::fs::write(&file, format!(r#"{{"devbox":[{}]}}"#, entries.join(","))).expect("write");

        assert_eq!(read(&file, "devbox").len(), LIMIT);
    }
}

mod updating {
    use super::*;

    #[test]
    fn writes_compact_json_leaving_out_what_is_empty() {
        let dir = TempDir::new();
        let file = kept_in(&dir);

        update(&file, "devbox", 5173, |_| Some(kept(5173, Some(5174), Some(42), false))).expect("written");

        assert_eq!(text(&file), r#"{"devbox":[{"port":5173,"local":5174,"pid":42}]}"#);
    }

    #[test]
    fn writes_a_closed_entry() {
        let dir = TempDir::new();
        let file = kept_in(&dir);

        update(&file, "devbox", 3000, |_| Some(kept(3000, Some(3000), None, true))).expect("written");

        assert_eq!(text(&file), r#"{"devbox":[{"port":3000,"local":3000,"closed":true}]}"#);
    }

    #[test]
    fn rereads_the_file_so_what_another_process_wrote_survives() {
        let dir = TempDir::new();
        let file = kept_in(&dir);
        update(&file, "devbox", 1, |_| Some(kept(1, None, None, false))).expect("written");
        std::fs::write(&file, r#"{"devbox":[{"port":1},{"port":2}],"other":[{"port":9}]}"#).expect("another window");

        update(&file, "devbox", 3, |_| Some(kept(3, None, None, false))).expect("written");

        let ports: Vec<u16> = read(&file, "devbox").iter().map(|entry| entry.port).collect();
        assert_eq!(ports, [1, 2, 3]);
        assert_eq!(read(&file, "other"), [kept(9, None, None, false)]);
    }

    #[test]
    fn the_same_entry_again_leaves_the_file_untouched() {
        let dir = TempDir::new();
        let file = kept_in(&dir);
        let by_hand = "{ \"devbox\": [ { \"port\": 5173, \"local\": 5174 } ] }\n";
        std::fs::write(&file, by_hand).expect("write");

        let wrote = update(&file, "devbox", 5173, |_| Some(kept(5173, Some(5174), None, false))).expect("ok");

        assert!(!wrote);
        assert_eq!(text(&file), by_hand);
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new();
        let file = kept_in(&dir);

        update(&file, "devbox", 1, |_| Some(kept(1, None, None, false))).expect("written");

        let mode = std::fs::metadata(&file).expect("stat").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

mod forgetting {
    use super::*;

    #[test]
    fn removes_the_port_and_drops_a_destination_left_empty() {
        let dir = TempDir::new();
        let file = kept_in(&dir);
        let json = r#"{"devbox":[{"port":1},{"port":2}],"other":[{"port":9}]}"#;
        std::fs::write(&file, json).expect("write");

        assert!(forget(&file, "devbox", 1).expect("ok"));
        assert_eq!(text(&file), r#"{"devbox":[{"port":2}],"other":[{"port":9}]}"#);

        assert!(forget(&file, "devbox", 2).expect("ok"));
        assert_eq!(text(&file), r#"{"other":[{"port":9}]}"#);
    }

    #[test]
    fn a_port_that_is_not_there_writes_nothing_and_creates_no_file() {
        let dir = TempDir::new();
        let file = kept_in(&dir);

        assert!(!forget(&file, "devbox", 1).expect("ok"));

        assert!(!file.exists());
    }
}
