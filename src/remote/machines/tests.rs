use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, unwritable_file_in};

fn kept_in(dir: &TempDir) -> PathBuf {
    dir.path().join("machines.json")
}

fn machine(destination: &str, command: Option<&str>) -> Machine {
    Machine { destination: destination.into(), command: command.map(Into::into), unreached: false }
}

fn remembered(destination: &str, command: Option<&str>) -> Machine {
    Machine::never_reached(destination.into(), command.map(Into::into))
}

fn destinations(file: &Path) -> Vec<String> {
    read(file).into_iter().map(|machine| machine.destination).collect()
}

mod remembering {
    use super::*;

    #[test]
    fn a_machine_is_kept_for_the_next_window() {
        let dir = TempDir::new();

        remember(&kept_in(&dir), machine("devbox", None)).expect("written");

        assert_eq!(read(&kept_in(&dir)), [machine("devbox", None)]);
    }

    #[test]
    fn machines_keep_the_order_they_were_first_reached_in() {
        let dir = TempDir::new();
        let file = kept_in(&dir);

        for destination in ["devbox", "gpu-box", "devbox", "me@winbox"] {
            remember(&file, machine(destination, None)).expect("written");
        }

        assert_eq!(destinations(&file), ["devbox", "gpu-box", "me@winbox"]);
    }

    #[test]
    fn the_last_command_used_for_a_machine_wins() {
        let dir = TempDir::new();
        let file = kept_in(&dir);
        remember(&file, machine("devbox", Some("~/tools/cornercase"))).expect("written");

        remember(&file, machine("devbox", None)).expect("written");

        assert_eq!(read(&file), [machine("devbox", None)]);
    }

    #[test]
    fn reaching_a_machine_again_writes_nothing() {
        let dir = TempDir::new();
        let file = kept_in(&dir);
        remember(&file, machine("devbox", None)).expect("written");
        let by_hand = r#"[ { "destination": "devbox" } ]"#;
        std::fs::write(&file, by_hand).expect("the same list, written by hand");

        remember(&file, machine("devbox", None)).expect("written");

        assert_eq!(std::fs::read_to_string(&file).expect("the list"), by_hand);
    }

    #[test]
    fn forgetting_one_keeps_the_others() {
        let dir = TempDir::new();
        let file = kept_in(&dir);
        remember(&file, machine("devbox", None)).expect("written");
        remember(&file, machine("gpu-box", None)).expect("written");

        assert!(forget(&file, "devbox").expect("written"));

        assert_eq!(destinations(&file), ["gpu-box"]);
    }

    #[test]
    fn forgetting_a_machine_it_never_reached_creates_no_file() {
        let dir = TempDir::new();

        assert!(!forget(&kept_in(&dir), "devbox").expect("nothing to write"));

        assert!(!kept_in(&dir).exists());
    }
}

mod the_mark {
    use super::*;

    fn text(file: &Path) -> String {
        std::fs::read_to_string(file).expect("the list")
    }

    #[test]
    fn a_machine_a_program_remembered_waits_for_a_window() {
        let dir = TempDir::new();
        let file = kept_in(&dir);

        remember(&file, remembered("newbox", Some("/opt/cc"))).expect("written");

        assert_eq!(read(&file), [remembered("newbox", Some("/opt/cc"))]);
        assert_eq!(text(&file), r#"[{"destination":"newbox","command":"/opt/cc","unreached":true}]"#);
    }

    #[test]
    fn a_link_drops_the_mark_and_writes_it_no_more() {
        let dir = TempDir::new();
        let file = kept_in(&dir);
        remember(&file, remembered("newbox", Some("/opt/cc"))).expect("written");

        remember(&file, machine("newbox", Some("/opt/cc"))).expect("written");

        assert_eq!(read(&file), [machine("newbox", Some("/opt/cc"))]);
        assert_eq!(text(&file), r#"[{"destination":"newbox","command":"/opt/cc"}]"#);
    }

    #[test]
    fn remembering_again_takes_the_last_command_and_keeps_the_mark() {
        let dir = TempDir::new();
        let file = kept_in(&dir);
        remember(&file, remembered("newbox", Some("/opt/cc"))).expect("written");

        let kept = remember(&file, remembered("newbox", None)).expect("written");

        assert_eq!(kept, remembered("newbox", None));
        assert_eq!(read(&file), [remembered("newbox", None)]);
    }

    #[test]
    fn remembering_a_machine_a_window_reached_adds_no_mark() {
        let dir = TempDir::new();
        let file = kept_in(&dir);
        remember(&file, machine("devbox", None)).expect("written");

        let kept = remember(&file, remembered("devbox", Some("/opt/cc"))).expect("written");

        assert!(!kept.unreached);
        assert_eq!(read(&file), [machine("devbox", Some("/opt/cc"))]);
    }

    #[test]
    fn the_mark_of_one_machine_outlives_a_link_to_another() {
        let dir = TempDir::new();
        let file = kept_in(&dir);
        remember(&file, remembered("newbox", None)).expect("written");

        remember(&file, machine("devbox", None)).expect("written");

        assert_eq!(read(&file), [remembered("newbox", None), machine("devbox", None)]);
    }

    #[test]
    fn a_list_that_is_there_but_broken_is_not_readable_and_a_missing_one_is() {
        let dir = TempDir::new();
        let file = kept_in(&dir);
        assert!(readable(&file).is_ok());
        std::fs::write(&file, "[").expect("a broken list");
        assert!(readable(&file).is_err());
        std::fs::write(&file, r#"[{"destination":"devbox"},42]"#).expect("a list");
        assert!(readable(&file).is_ok());
    }

    #[test]
    fn a_list_it_cannot_write_is_an_error() {
        let dir = TempDir::new();
        let file = unwritable_file_in(&dir, "machines.json");

        assert!(remember(&file, machine("devbox", None)).is_err());
    }
}

mod reading {
    use super::*;

    #[test]
    fn a_missing_file_knows_no_machine() {
        let dir = TempDir::new();

        assert_eq!(read(&kept_in(&dir)), []);
        assert!(!kept_in(&dir).exists());
    }

    #[test]
    fn entries_are_read_in_their_order_with_or_without_a_command() {
        let dir = TempDir::new();
        let written = r#"[{"destination":"gpu-box","command":"/opt/cc"},{"destination":"devbox"},
            {"destination":"me@10.0.0.5","command":"~/bin/cornercase","added":"by a newer release"}]"#;
        std::fs::write(kept_in(&dir), written).expect("an edited file");

        assert_eq!(
            read(&kept_in(&dir)),
            [
                machine("gpu-box", Some("/opt/cc")),
                machine("devbox", None),
                machine("me@10.0.0.5", Some("~/bin/cornercase"))
            ]
        );
    }

    #[test]
    fn a_broken_entry_is_skipped_when_read() {
        let dir = TempDir::new();
        let written =
            r#"[42,{"destination":"devbox"},{"destination":["x"]},{"command":"/opt/cc"},{"destination":"dev box"}]"#;
        std::fs::write(kept_in(&dir), written).expect("an edited file");

        assert_eq!(read(&kept_in(&dir)), [machine("devbox", None)]);
    }

    #[test]
    fn an_unreadable_file_knows_no_machine() {
        let dir = TempDir::new();
        std::fs::write(kept_in(&dir), "[").expect("a broken file");

        assert_eq!(read(&kept_in(&dir)), []);
    }

    #[test]
    fn a_broken_entry_leaves_the_others_in_place() {
        let dir = TempDir::new();
        let written = r#"[{"destination":null},{"destination":"devbox"},{"command":"/opt/cc"}]"#;
        std::fs::write(kept_in(&dir), written).expect("an edited file");

        remember(&kept_in(&dir), machine("gpu-box", None)).expect("written");

        assert_eq!(destinations(&kept_in(&dir)), ["devbox", "gpu-box"]);
    }

    #[test]
    fn a_destination_edited_into_an_option_is_left_out() {
        let dir = TempDir::new();
        let written = r#"[{"destination":"-oProxyCommand=sh"},{"destination":"devbox","command":"/opt/cc"}]"#;
        std::fs::write(kept_in(&dir), written).expect("an edited file");

        assert_eq!(read(&kept_in(&dir)), [machine("devbox", Some("/opt/cc"))]);
    }
}

mod destinations {
    use super::*;

    #[rstest]
    #[case::a_host("devbox")]
    #[case::a_user_and_a_host("me@devbox.example.com")]
    #[case::a_port("ssh://me@devbox:2222")]
    #[case::an_address("10.0.0.7")]
    fn ssh_takes_it(#[case] destination: &str) {
        assert!(valid(destination));
    }

    #[rstest]
    #[case::empty("")]
    #[case::an_option("-oProxyCommand=sh")]
    #[case::two_words("devbox ls")]
    #[case::a_line_break("devbox\nls")]
    #[case::a_tab("dev\tbox")]
    fn it_is_refused(#[case] destination: &str) {
        assert!(!valid(destination));
    }

    #[test]
    fn a_destination_longer_than_a_host_name_is_refused() {
        assert!(!valid(&"a".repeat(LONGEST + 1)));
    }
}
