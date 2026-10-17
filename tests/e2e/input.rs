use super::*;

#[test]
fn ctrl_b_reaches_the_shell() {
    let mut app = Harness::start();
    app.send(b"echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs", |s| s.contains("cat-starts"));

    app.send(b"\x02c\r");

    app.wait_for("cat echoes ctrl+b", |s| s.matches("^Bc").count() >= 2);
}

#[test]
fn the_prefix_key_opens_a_menu_whose_keys_run_actions() {
    let session = Session::new();
    std::fs::write(session.dir.join("config.json"), r#"{"prefix_key": "ctrl+]"}"#).expect("write config");
    let mut app = Harness::open(session, ROWS, COLS);
    app.wait_for("app starts with one terminal", |s| s.contains(&first_entry()));
    app.send(b"echo in-the-first\r");
    app.wait_for("output in the first tab", |s| s.contains("in-the-first"));

    app.send(b"\x1d");
    app.wait_for("the keys menu opens", |s| s.contains("agent that needs you"));
    app.send(b"c");

    app.wait_for("a second tab shows", |s| !s.contains("in-the-first") && !s.contains("agent that needs you"));
}

#[test]
fn focus_reporting_is_on_while_attached_and_off_once_it_leaves() {
    let mut app = Harness::start();
    app.wait_for_raw("focus reporting is turned on", |raw| raw.contains("\x1b[?1004h"));

    app.click(areas().quit.as_position());
    app.wait_exit("client exits");

    app.wait_for_raw("focus reporting is turned off after it was on", |raw| {
        raw.rfind("\x1b[?1004l").zip(raw.rfind("\x1b[?1004h")).is_some_and(|(off, on)| off > on)
    });
}

#[test]
fn ctrl_enter_reaches_a_program_that_asked_for_kitty_keys() {
    let mut app = Harness::start();
    app.wait_for_raw("the app asks the terminal for kitty keys", |raw| raw.contains("\x1b[>1u"));
    app.send(b"printf '\\033[>1u'; echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs", |s| s.contains("cat-starts"));

    app.send(b"\x1b[13;5u\r");

    app.wait_for("cat echoes ctrl+enter", |s| s.matches("^[[13;5u").count() >= 2);
}

#[rstest]
#[case::no_marker(b"".as_slice())]
#[case::late_sixel(b"\x1b[?1;0;256S")]
#[case::late_colors(b"\x1b]4;15;rgb:1111/2222/3333\x1b\\\x1bP>|ghostty 1.3.1\x1b\\\x1b[?62;22;52c")]
#[case::invalid_coordinates(b"\x1b[<0;0;0M\x1b[0;0R")]
fn a_probe_without_its_end_marker_keeps_keys_and_clicks(#[case] late: &[u8]) {
    let mut app = Harness::answering(b"\x1b]11;rgb:20/20/20\x1b\\");
    app.send(b"printf '\\033[?1000h\\033[?1006h'; echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs after the probe deadline", |s| s.contains("cat-starts"));
    app.raw.lock().clear();
    app.send(late);
    app.click(Position::new(pane().x + 7, pane().y + 4));
    app.send(b"echo zz-marker\r");
    app.wait_for("cat receives the click and all keys", |s| s.contains("^[[<0;8;5M^[[<0;8;5mecho zz-marker"));
    assert!(!app.text().contains("rgb:"), "the probe reply reached the pane");
    assert!(!app.text().contains("ghostty 1.3.1"), "the terminal name reached the pane");
    let raw = app.raw.lock();
    let raw = String::from_utf8_lossy(&raw);
    assert!(!raw.contains("\x1b[?1000l"), "input recovery disabled mouse capture");
    assert!(!raw.contains("\x1b[?1049l"), "input recovery left the alternate screen");
}

#[test]
fn a_late_palette_batch_is_not_typed_into_the_pane() {
    let mut app = Harness::answering(b"\x1b]11;rgb:20/20/20\x1b\\");
    app.send(b"echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs after the probe deadline", |s| s.contains("cat-starts"));
    app.raw.lock().clear();
    let mut late = String::new();
    for i in 0..256 {
        write!(late, "\x1b]4;{i};rgb:1111/2222/3333\x1b\\").expect("late palette reply");
    }
    late.push_str("\x1bP>|ghostty 1.3.1\x1b\\\x1b[?62;22;52c");
    app.send(late.as_bytes());
    app.send(b"zz-marker\r");
    app.wait_for("cat echoes only the intended text", |s| s.matches("zz-marker").count() >= 2);
    assert!(!String::from_utf8_lossy(&app.raw.lock()).contains("rgb:"), "the palette was typed into the pane");
}

#[test]
fn mouse_is_not_forwarded_when_the_program_did_not_ask() {
    let mut app = Harness::start();
    app.send(b"echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs", |s| s.contains("cat-starts"));

    app.click(Position::new(pane().x + 7, pane().y + 4));
    app.send(b"done\r");

    app.wait_for("cat echoes the text", |s| s.matches("done").count() >= 2);
    assert!(!app.text().contains("^[[<"), "mouse leaked into the program:\n{}", app.text());
}

#[test]
fn mouse_is_forwarded_relative_to_the_pane_when_requested() {
    let mut app = Harness::start();
    app.send(b"printf '\\033[?1000h\\033[?1006h'; echo cat-\"\"starts; cat -v\r");
    app.wait_for("cat runs with mouse mode on", |s| s.contains("cat-starts"));

    app.click(Position::new(pane().x + 7, pane().y + 4));

    app.wait_for("program receives the click", |s| s.contains("^[[<0;8;5M^[[<0;8;5m"));
}

#[test]
fn programs_see_the_host_background_color() {
    let mut app = Harness::start();

    app.send(b"stty -icanon -echo; printf '\\033]11;?\\033\\\\'; head -c 23 | tr '\\033' E; stty sane\r");

    app.wait_for("program reads the host background", |s| s.contains("E]11;rgb:1212/5656/9a9a"));
}
