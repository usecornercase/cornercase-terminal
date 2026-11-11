use rstest::rstest;

use super::*;

fn wrapped(emu: &mut Emulator) -> Vec<u16> {
    emu.surface_snapshot().expect("snapshot").wrapped
}

#[rstest]
#[case::a_long_line(
    8,
    b"0000000001\r\n0000000002\r\nhttps://example.com/0000000000000\r\n",
    &[2, 3, 4]
)]
#[case::a_full_line_and_a_newline(8, b"0123456789\r\nx", &[])]
#[case::a_wide_character_that_does_not_fit(8, "012345678中x".as_bytes(), &[0])]
#[case::on_the_alternate_screen(8, b"\x1b[?1049h0123456789abc", &[0])]
fn the_rows_that_go_on_are_listed(#[case] rows: u16, #[case] output: &[u8], #[case] expected: &[u16]) {
    let mut emu = after(rows, 10, output);

    assert_eq!(wrapped(&mut emu), expected);
}

#[test]
fn the_window_snapshot_reads_no_wraps() {
    let mut emu = after(8, 10, b"https://example.com/0000000000000\r\n");

    assert_eq!(emu.snapshot().expect("snapshot").wrapped, Vec::<u16>::new());
}

#[test]
fn a_view_scrolled_up_lists_the_rows_it_shows() {
    let long = "x".repeat(25);
    let mut emu = after(3, 10, format!("a\r\nb\r\nc\r\nd\r\n{long}\r\ne\r\nf").as_bytes());

    assert_eq!(wrapped(&mut emu), Vec::<u16>::new());
    emu.scroll(-2);
    assert_eq!(wrapped(&mut emu), [0, 1]);
    emu.scroll(-1);
    assert_eq!(wrapped(&mut emu), [1, 2]);
}

#[test]
fn a_row_that_wraps_after_a_snapshot_is_listed_in_the_next() {
    let mut emu = after(5, 10, b"\r\n\r\n0123456789");

    assert_eq!(wrapped(&mut emu), Vec::<u16>::new());
    emu.feed(b"abc");
    assert_eq!(wrapped(&mut emu), [2]);
}

#[test]
fn a_narrower_terminal_lists_the_rows_a_line_reflows_onto() {
    let mut emu = after(6, 10, b"0123456789\r\nshort");

    emu.resize(6, 6).expect("resize");

    assert_eq!(wrapped(&mut emu), [0]);
}

#[test]
fn clearing_the_screen_lists_none() {
    let mut emu = after(5, 10, b"0123456789abc");

    emu.feed(b"\x1b[2J\x1b[H");

    assert_eq!(wrapped(&mut emu), Vec::<u16>::new());
}
