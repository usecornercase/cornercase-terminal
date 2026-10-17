use super::*;

fn after(rows: u16, cols: u16, output: &[u8]) -> Emulator {
    let mut emu = Emulator::new(rows, cols, 100_000, &HostTheme::default(), Box::new(|_| {})).expect("emulator");
    emu.feed(output);
    emu
}

fn needle(text: &str) -> Needle {
    Needle::new(text).expect("a needle")
}

fn selected(emu: &mut Emulator) -> Option<String> {
    emu.finish_selection().expect("finish")
}

#[test]
fn the_text_holds_the_scrollback_with_wrapped_lines_joined() {
    let emu = after(3, 10, b"one\r\ntwo\r\n0123456789abcde\r\nfour\r\nfive");

    assert_eq!(
        emu.text().expect("text").lines().collect::<Vec<_>>(),
        ["one", "two", "0123456789abcde", "four", "five"]
    );
}

#[test]
fn logical_lines_start_where_the_wrap_continuations_do_even_after_blank_lines() {
    let emu = after(10, 10, b"a\r\n\r\n\r\n0123456789abcdefghij0123\r\nlast\r\n\r\nend");
    let lines = emu.text().expect("text").lines().count();

    let rows: Vec<Option<usize>> = (0..lines).map(|line| emu.line_row(line).expect("row")).collect();

    assert_eq!(rows, [Some(0), Some(1), Some(2), Some(3), Some(6), Some(7), Some(8)]);
}

#[test]
fn selects_a_match_that_a_soft_wrap_cut_in_two() {
    let mut emu = after(4, 10, b"0123456789abcdefneedle\r\nnext");

    assert!(emu.reveal(&needle("needle"), None).expect("reveal"));

    assert_eq!(selected(&mut emu).as_deref(), Some("needle"));
}

#[test]
fn selects_a_match_after_wide_characters() {
    let mut emu = after(4, 10, "你好世界 needle".as_bytes());

    assert!(emu.reveal(&needle("needle"), None).expect("reveal"));

    assert_eq!(selected(&mut emu).as_deref(), Some("needle"));
}

#[test]
fn a_wide_character_that_does_not_fit_wraps_early() {
    let mut emu = after(4, 10, "012345678你好 needle".as_bytes());

    assert!(emu.reveal(&needle("needle"), None).expect("reveal"));

    assert_eq!(selected(&mut emu).as_deref(), Some("needle"));
}

#[test]
fn scrolls_to_an_old_line_and_marks_its_cells() {
    let mut emu = after(5, 20, b"first needle\r\n");
    for i in 0..40 {
        emu.feed(format!("filler {i}\r\n").as_bytes());
    }
    assert_eq!(emu.below(), 0);

    assert!(emu.reveal(&needle("needle"), None).expect("reveal"));

    assert!(emu.below() > 0);
    let snap = emu.snapshot().expect("snapshot");
    let row = snap.rows.iter().find(|row| row.iter().map(|c| c.symbol.as_str()).collect::<String>().contains("needle"));
    let row = row.expect("the line shows");
    let at = row.iter().position(|c| c.symbol == "n").expect("a cell");
    assert!(row[at].style.add_modifier.contains(Modifier::REVERSED));
}

#[test]
fn picks_the_occurrence_nearest_the_line_it_was_found_on() {
    let mut emu = after(4, 20, b"needle one\r\nmiddle\r\nneedle two\r\ntail");
    let marked = |emu: &mut Emulator| {
        let snap = emu.snapshot().expect("snapshot");
        let row = snap.rows.iter().find(|row| row.iter().any(|c| c.style.add_modifier.contains(Modifier::REVERSED)));
        row.map(|row| row.iter().map(|c| c.symbol.as_str()).collect::<String>().trim_end().to_string())
    };

    assert!(emu.reveal(&needle("needle"), Some(0)).expect("reveal"));
    assert_eq!(marked(&mut emu).as_deref(), Some("needle one"));
    assert!(emu.reveal(&needle("needle"), Some(3)).expect("reveal"));
    assert_eq!(marked(&mut emu).as_deref(), Some("needle two"));
    assert!(emu.reveal(&needle("needle"), None).expect("reveal"));
    assert_eq!(marked(&mut emu).as_deref(), Some("needle two"));
}

#[test]
fn without_a_match_it_does_nothing() {
    let mut emu = after(4, 20, b"nothing here");

    assert!(!emu.reveal(&needle("needle"), None).expect("reveal"));
    assert_eq!(selected(&mut emu), None);
}
