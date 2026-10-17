use super::*;

fn numbered(lines: usize) -> Emulator {
    let mut emu = Emulator::new(4, 20, 100, &HostTheme::default(), Box::new(|_| {})).expect("emulator");
    let text: Vec<String> = (1..=lines).map(|i| format!("line-{i}")).collect();
    emu.feed(text.join("\r\n").as_bytes());
    emu
}

fn shown(emu: &mut Emulator) -> String {
    emu.snapshot().expect("snapshot").contents()
}

#[test]
fn at_the_bottom_nothing_is_below() {
    let mut emu = numbered(10);

    assert_eq!((emu.below(), shown(&mut emu)), (0, "line-7\nline-8\nline-9\nline-10".to_string()));
}

#[test]
fn scrolling_up_shows_earlier_lines() {
    let mut emu = numbered(10);

    emu.scroll(-3);

    assert_eq!(shown(&mut emu), "line-4\nline-5\nline-6\nline-7");
}

#[test]
fn the_snapshot_says_how_many_rows_are_below_the_view() {
    let mut emu = numbered(10);

    emu.scroll(-3);

    assert_eq!((emu.below(), emu.snapshot().expect("snapshot").below), (3, 3));
}

#[test]
fn the_scrollbar_counts_the_history_and_where_the_view_is() {
    let mut emu = numbered(10);
    assert_eq!(emu.scrollbar(), Scrollbar { total: 10, offset: 6, rows: 4 });

    emu.scroll(-3);

    let snap = emu.snapshot().expect("snapshot");
    assert_eq!(snap.scroll, Scrollbar { total: 10, offset: 3, rows: 4 });
    assert_eq!(snap.below, 3);
}

#[test]
fn scrolling_past_the_top_stops_at_the_first_line() {
    let mut emu = numbered(10);

    emu.scroll(-100);

    assert_eq!(shown(&mut emu), "line-1\nline-2\nline-3\nline-4");
}

#[test]
fn new_output_keeps_the_view_and_grows_below() {
    let mut emu = numbered(10);
    emu.scroll(-3);

    emu.feed(b"\r\nline-11\r\nline-12");

    assert_eq!((shown(&mut emu), emu.below()), ("line-4\nline-5\nline-6\nline-7".to_string(), 5));
}

#[test]
fn scrolling_down_to_the_end_leaves_nothing_below() {
    let mut emu = numbered(10);
    emu.scroll(-3);

    emu.scroll(3);

    assert_eq!((emu.below(), shown(&mut emu)), (0, "line-7\nline-8\nline-9\nline-10".to_string()));
}

#[test]
fn scrolling_to_the_bottom_shows_the_last_lines_again() {
    let mut emu = numbered(10);
    emu.scroll(-3);

    emu.scroll_to_bottom();

    assert_eq!((emu.below(), shown(&mut emu)), (0, "line-7\nline-8\nline-9\nline-10".to_string()));
}

#[test]
fn launches_read_the_active_screen_even_when_scrolled() {
    let mut emu = numbered(10);
    emu.scroll(-3);

    let active = emu.active_contents().expect("text");

    assert_eq!(active, "line-7\nline-8\nline-9\nline-10");
    assert_eq!(shown(&mut emu), "line-4\nline-5\nline-6\nline-7", "the view stays where it was");
}

#[test]
fn a_program_that_turns_on_the_mouse_brings_the_view_back_down() {
    let mut emu = numbered(10);
    emu.scroll(-3);

    emu.feed(b"\x1b[?1000h");

    let snap = emu.snapshot().expect("snapshot");
    assert_eq!((snap.below, snap.contents()), (0, "line-7\nline-8\nline-9\nline-10".to_string()));
}

#[test]
fn the_alternate_screen_has_nothing_below() {
    let mut emu = numbered(10);
    emu.scroll(-3);

    emu.feed(b"\x1b[?1049h");

    assert_eq!((emu.alternate_screen(), emu.snapshot().expect("snapshot").below), (true, 0));
}

#[rstest]
#[case::on_by_default(b"", true)]
#[case::turned_off(b"\x1b[?1007l", false)]
fn alternate_scroll_follows_the_program(#[case] output: &[u8], #[case] expected: bool) {
    let mut emu = numbered(1);
    emu.feed(output);

    assert_eq!(emu.alt_scroll(), expected);
}
