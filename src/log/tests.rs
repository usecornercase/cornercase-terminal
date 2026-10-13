use rstest::rstest;

use super::*;
use crate::test_util::TempDir;

fn at(secs: u64, millis: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs) + Duration::from_millis(millis)
}

mod line {
    use super::*;

    #[test]
    fn starts_with_the_time_the_level_and_the_target() {
        let ms = 812;
        let text = line(at(1_791_407_391, 482), Level::Info, "worktree", "add done", &[("ms", &ms)]);
        assert_eq!(text, "2026-10-07T21:09:51.482Z INFO  worktree: add done ms=812\n");
    }

    #[rstest]
    #[case::a_word("main", "main")]
    #[case::a_path("/home/me/my-app", "/home/me/my-app")]
    #[case::spaces("my app", "\"my app\"")]
    #[case::quotes("say \"hi\"", "\"say \\\"hi\\\"\"")]
    #[case::a_newline("one\ntwo", "\"one\\ntwo\"")]
    #[case::an_escape("\x1b[2J", "\"\\u{1b}[2J\"")]
    #[case::nothing("", "\"\"")]
    #[case::accents("café", "café")]
    fn quotes_values_only_when_they_need_it(#[case] value: &str, #[case] shown: &str) {
        let text = line(at(0, 0), Level::Debug, "app", "x", &[("v", &value)]);
        assert_eq!(text, format!("1970-01-01T00:00:00.000Z DEBUG app: x v={shown}\n"));
    }
}

mod stamp {
    use super::*;

    #[rstest]
    #[case::the_epoch(0, "1970-01-01T00:00:00.000Z")]
    #[case::a_leap_day(951_782_400, "2000-02-29T00:00:00.000Z")]
    #[case::new_year(1_798_761_599, "2026-12-31T23:59:59.000Z")]
    #[case::far_ahead(4_102_444_800, "2100-01-01T00:00:00.000Z")]
    fn is_utc(#[case] secs: u64, #[case] shown: &str) {
        assert_eq!(Stamp(at(secs, 0)).to_string(), shown);
    }

    #[rstest]
    #[case::the_epoch(0, 0)]
    #[case::a_leap_day(951_782_400, 7)]
    #[case::new_year(1_798_761_599, 999)]
    #[case::far_ahead(4_102_444_800, 120)]
    fn reads_back_what_it_writes(#[case] secs: u64, #[case] millis: u64) {
        let shown = Stamp(at(secs, millis)).to_string();

        assert_eq!(Stamp::parse(&shown), Some(at(secs, millis)));
    }

    #[rstest]
    #[case::seconds_only("2026-10-08T20:23:20Z", Some(at(1_791_491_000, 0)))]
    #[case::microseconds("2026-10-08T20:23:20.000749Z", Some(at(1_791_491_000, 0) + Duration::from_micros(749)))]
    #[case::an_offset("2026-10-08T22:23:20.749+02:00", None)]
    #[case::no_time("2026-10-08", None)]
    #[case::a_thirteenth_month("2026-13-08T20:23:20.749Z", None)]
    #[case::a_sign("2026-10-08T20:23:+2.749Z", None)]
    #[case::too_many_parts("2026-10-08-01T20:23:20Z", None)]
    #[case::before_the_epoch("0000-01-01T00:00:00Z", None)]
    #[case::empty("", None)]
    fn reads_utc_times_of_other_programs(#[case] text: &str, #[case] expected: Option<SystemTime>) {
        assert_eq!(Stamp::parse(text), expected);
    }
}

mod level {
    use super::*;

    #[rstest]
    #[case::debug("debug", Some(Level::Debug))]
    #[case::any_case(" WARN ", Some(Level::Warn))]
    #[case::unknown("loud", None)]
    fn parses_the_variable(#[case] text: &str, #[case] level: Option<Level>) {
        assert_eq!(Level::parse(text), level);
    }
}

mod sink {
    use super::*;

    #[test]
    fn rotates_once_the_file_would_pass_its_cap() {
        let tmp = TempDir::new();
        let path = tmp.path().join("server.log");
        let mut sink = Sink::open(path.clone(), 20, false).expect("open");
        sink.write("first line\n");
        sink.write("second line\n");
        sink.write("third\n");

        let read = |path: &Path| std::fs::read_to_string(path).expect("read");
        assert_eq!(read(&rotated(&path)), "first line\n");
        assert_eq!(read(&path), "second line\nthird\n");
    }

    #[test]
    fn keeps_one_old_file() {
        let tmp = TempDir::new();
        let path = tmp.path().join("server.log");
        let mut sink = Sink::open(path.clone(), 10, false).expect("open");
        for line in ["one 12345\n", "two 12345\n", "three 123\n"] {
            sink.write(line);
        }

        assert_eq!(std::fs::read_to_string(rotated(&path)).expect("read"), "two 12345\n");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "three 123\n");
    }
}

#[test]
fn only_its_owner_can_read_the_log() {
    let tmp = TempDir::new();
    let path = tmp.path().join("server.log");
    std::fs::write(&path, "old\n").expect("write");
    std::fs::set_permissions(&path, Permissions::from_mode(0o644)).expect("chmod");
    let mut sink = Sink::open(path.clone(), 10, false).expect("open");
    sink.write("a new line\n");

    let mode = |path: &Path| std::fs::metadata(path).expect("stat").mode() & 0o777;
    assert_eq!((mode(&rotated(&path)), mode(&path)), (PRIVATE, PRIVATE));
}

#[test]
fn an_older_log_left_readable_is_made_private_too() {
    let tmp = TempDir::new();
    let path = tmp.path().join("server.log");
    std::fs::write(rotated(&path), "older\n").expect("write");
    std::fs::set_permissions(rotated(&path), Permissions::from_mode(0o644)).expect("chmod");

    Sink::open(path.clone(), 10, false).expect("open");

    assert_eq!(std::fs::metadata(rotated(&path)).expect("stat").mode() & 0o777, PRIVATE);
}

#[test]
fn starts_over_when_the_old_log_cannot_be_kept() {
    let tmp = TempDir::new();
    let path = tmp.path().join("server.log");
    std::fs::create_dir(rotated(&path)).expect("a folder in the way");
    std::fs::write(rotated(&path).join("inside"), "").expect("fill it");
    let mut sink = Sink::open(path.clone(), 20, false).expect("open");
    sink.write("first line\n");
    sink.write("second line\n");

    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("could not keep the old log") && text.ends_with("second line\n"), "{text}");
    assert!(!text.contains("first line"), "{text}");
}

mod tail {
    use super::*;

    #[test]
    fn gives_the_last_lines() {
        let tmp = TempDir::new();
        let path = tmp.path().join("server.log");
        std::fs::write(&path, "a\nb\nc\n").expect("write");

        assert_eq!(tail(&path, 2).expect("tail"), "b\nc\n");
    }

    #[test]
    fn reaches_into_the_old_file_when_the_new_one_is_short() {
        let tmp = TempDir::new();
        let path = tmp.path().join("server.log");
        std::fs::write(rotated(&path), "a\nb\n").expect("write");
        std::fs::write(&path, "c\n").expect("write");

        assert_eq!(tail(&path, 2).expect("tail"), "b\nc\n");
    }
}

mod follower {
    use super::*;

    #[test]
    fn reads_what_was_added_since() {
        let tmp = TempDir::new();
        let path = tmp.path().join("server.log");
        std::fs::write(&path, "old\n").expect("write");
        let mut follower = Follower::at_end(&path).expect("follow");
        OpenOptions::new().append(true).open(&path).expect("open").write_all(b"new\n").expect("append");

        assert_eq!(follower.read().expect("read"), b"new\n");
        assert_eq!(follower.read().expect("read"), b"");
    }

    #[test]
    fn goes_on_in_the_new_file_after_a_rotation() {
        let tmp = TempDir::new();
        let path = tmp.path().join("server.log");
        std::fs::write(&path, "old\n").expect("write");
        let mut follower = Follower::at_end(&path).expect("follow");
        OpenOptions::new().append(true).open(&path).expect("open").write_all(b"last\n").expect("append");
        std::fs::rename(&path, rotated(&path)).expect("rotate");
        std::fs::write(&path, "first\n").expect("write");

        assert_eq!(follower.read().expect("read"), b"last\nfirst\n");
    }
}

mod job {
    use super::*;

    #[test]
    fn keeps_no_fields_while_nothing_logs() {
        let job = Job::new(Level::Info, "git", "fetch").with("repo", "/tmp/a");
        assert_eq!(job.fields.len(), 0);
    }
}
