use std::process::{Command, Stdio};

use rstest::rstest;

use super::*;
use crate::test_util::{TempDir, fake_program};

fn devbox() -> Remote {
    Remote { destination: "devbox".into(), command: None, option: "--remote-command" }
}

fn fed(pieces: &[&[u8]]) -> Vec<u8> {
    let remote = devbox();
    let mut hinting = Hinting::new(&remote, Vec::new());
    for piece in pieces {
        hinting.take(piece).expect("take");
    }
    hinting.finish().expect("finish")
}

fn text(pieces: &[&[u8]]) -> String {
    String::from_utf8(fed(pieces)).expect("utf-8")
}

mod a_line {
    use super::*;

    #[rstest]
    #[case::lf("see `cornercase status`\n", "see `cornercase --remote devbox status`\n")]
    #[case::crlf("see `cornercase status`\r\n", "see `cornercase --remote devbox status`\r\n")]
    #[case::none("nothing to say\n", "nothing to say\n")]
    fn names_the_remote_in_its_hints(#[case] said: &str, #[case] expected: &str) {
        assert_eq!(text(&[said.as_bytes()]), expected);
    }

    #[test]
    fn keeps_a_hint_for_the_machine_named() {
        assert_eq!(text(&[b"Run `cornercase update` there\n"]), "Run `cornercase update` there\n");
    }

    #[test]
    fn starts_its_backticks_again_on_the_next_line() {
        assert_eq!(
            text(&[b"a stray ` here\nsee `cornercase status`\n"]),
            "a stray ` here\nsee `cornercase --remote devbox status`\n"
        );
    }

    #[test]
    fn names_the_remote_in_the_update_command_on_its_own() {
        let said = "cornercase 0.14.0 is out (you have 0.13.0). Update it with:\ncornercase update\n";

        assert_eq!(
            text(&[said.as_bytes()]),
            "cornercase 0.14.0 is out (you have 0.13.0). Update it with:\ncornercase --remote devbox update\n"
        );
        assert_eq!(text(&[b"cornercase updated\ncornercase upd"]), "cornercase updated\ncornercase upd");
    }
}

mod a_split_stream {
    use super::*;

    const MIXED: &str = "Run `cornercase update` there.\nsee `cornercase status` é→ and `cornercase keys --pane 7 0`\r\nstray ` tick\ncornercase update\ncornercase update\r\nwith `cornercase update` thereafter\nlast `cornercase status`";

    #[test]
    fn gives_what_one_piece_gives_wherever_it_is_cut() {
        let whole = text(&[MIXED.as_bytes()]);
        let alone: String = MIXED.split_inclusive('\n').map(|line| text(&[line.as_bytes()])).collect();
        assert_eq!(whole, alone);

        for cut in 0..=MIXED.len() {
            let (first, second) = MIXED.as_bytes().split_at(cut);
            assert_eq!(text(&[first, second]), whole, "cut at {cut}");
        }
    }

    #[test]
    fn writes_a_question_before_its_answer() {
        let remote = devbox();
        let mut hinting = Hinting::new(&remote, Vec::new());

        hinting.take(b"Restart it now? [y/N] ").expect("take");

        assert_eq!(hinting.out, b"Restart it now? [y/N] ");
    }

    #[test]
    fn holds_an_open_hint_until_its_end() {
        let remote = devbox();
        let mut hinting = Hinting::new(&remote, Vec::new());

        hinting.take(b"Run `cornercase sta").expect("take");
        assert_eq!(hinting.out, b"Run ");
        hinting.take(b"tus` now\n").expect("take");

        assert_eq!(String::from_utf8_lossy(&hinting.out), "Run `cornercase --remote devbox status` now\n");
    }

    #[test]
    fn holds_a_hint_until_the_word_after_it_is_known() {
        let remote = devbox();
        let mut hinting = Hinting::new(&remote, Vec::new());

        hinting.take(b"Run `cornercase update` the").expect("take");
        assert_eq!(hinting.out, b"Run ");
        hinting.take(b"re\n").expect("take");

        assert_eq!(String::from_utf8_lossy(&hinting.out), "Run `cornercase update` there\n");
    }

    #[test]
    fn lets_a_hint_go_once_the_next_word_is_no_machine() {
        let remote = devbox();
        let mut hinting = Hinting::new(&remote, Vec::new());

        hinting.take(b"Run `cornercase status` and").expect("take");

        assert_eq!(String::from_utf8_lossy(&hinting.out), "Run `cornercase --remote devbox status` and");
    }

    #[test]
    fn writes_an_open_span_that_is_no_hint_at_the_end() {
        assert_eq!(text(&[b"Type `y"]), "Type `y");
    }

    #[test]
    fn holds_the_start_of_the_update_line() {
        let remote = devbox();
        let mut hinting = Hinting::new(&remote, Vec::new());

        hinting.take(b"cornercase upd").expect("take");

        assert_eq!(hinting.out, b"");
    }

    #[test]
    fn goes_on_after_bytes_that_are_not_utf8() {
        assert_eq!(
            text(&[b"bad \xff byte `cornercase status`\n"]),
            "bad \u{fffd} byte `cornercase --remote devbox status`\n"
        );
    }

    #[test]
    fn writes_what_was_held_at_the_end() {
        assert_eq!(text(&[b"Run `cornercase update` the"]), "Run `cornercase --remote devbox update` the");
        assert_eq!(text(&[b"Run `cornercase update` there"]), "Run `cornercase update` there");
        assert_eq!(text(&[b"cornercase update"]), "cornercase --remote devbox update");
    }
}

#[rstest]
#[case::logs(&["logs"], Printed::Data)]
#[case::follow(&["logs", "--follow", "--lines", "200"], Printed::Data)]
#[case::plugin_logs(&["plugin", "logs", "--lines", "200", "--", "acme.x"], Printed::Data)]
#[case::update_json(&["update", "--check", "--json"], Printed::Json)]
#[case::install_json(&["plugin", "install", "--yes", "--json", "--", "owner/repo"], Printed::Json)]
#[case::after_the_dashes(&["plugin", "remove", "--", "--json"], Printed::Messages)]
#[case::update(&["update"], Printed::Messages)]
#[case::restart(&["restart", "--when-idle"], Printed::Messages)]
#[case::kill(&["kill-server", "--yes"], Printed::Messages)]
#[case::plugin_list(&["plugin", "list"], Printed::Messages)]
fn what_a_command_prints_is_told_apart(#[case] words: &[&str], #[case] expected: Printed) {
    assert_eq!(printed(words), expected);
}

#[test]
fn both_streams_of_a_process_go_through_the_rewrite() {
    let dir = TempDir::new();
    let ssh = fake_program(dir.path(), "ssh");
    std::fs::write(ssh.with_extension("out"), "no plugins; `cornercase plugin install <owner/repo>` adds one\n")
        .expect("out");
    std::fs::write(ssh.with_extension("err"), "Error: `cornercase update` only updates released builds\n")
        .expect("err");
    std::fs::write(ssh.with_extension("code"), "3").expect("code");
    let mut child = Command::new(&ssh).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("spawn");
    let (mut out, mut err) = (Vec::new(), Vec::new());

    let status = relayed(&devbox(), &mut child, &mut out, &mut err).expect("wait");

    assert_eq!(status.code(), Some(3));
    assert_eq!(
        String::from_utf8_lossy(&out),
        "no plugins; `cornercase --remote devbox plugin install <owner/repo>` adds one\n"
    );
    assert_eq!(
        String::from_utf8_lossy(&err),
        "Error: `cornercase --remote devbox update` only updates released builds\n"
    );
}
