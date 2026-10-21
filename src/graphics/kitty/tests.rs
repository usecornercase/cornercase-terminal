use image::RgbaImage;
use ratatui::text::Span;
use rstest::rstest;

use super::super::encode::tests::{RED, cell as cell_size, image_of, key, picture};
use super::super::encode::{Fit, Key, encode, fit};
use super::super::{Protocol, Tmux};
use super::*;

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("ASCII commands")
}

fn payload(command: &[u8]) -> &[u8] {
    let start = command.iter().position(|&byte| byte == b';').map_or(command.len() - 2, |at| at + 1);
    &command[start..command.len() - 2]
}

mod commands {
    use super::*;

    #[rstest]
    #[case::high_byte_first(42, 0xf0, 0x2a00_00f0)]
    #[case::the_other_low_byte(42, 0xf1, 0x2a00_00f1)]
    #[case::largest(255, 255, 0xff00_00ff)]
    fn builds_an_id_from_two_bytes(#[case] hi: u8, #[case] lo: u8, #[case] expected: u32) {
        assert_eq!(id(hi, lo), expected);
    }

    #[test]
    fn sends_a_small_picture_in_one_quiet_command() {
        assert_eq!(transmit(7, b"\0\0\0"), [b"\x1b_Ga=t,q=2,i=7,f=100;AAAA\x1b\\".to_vec()]);
    }

    #[test]
    fn places_the_picture_once_for_its_cells() {
        assert_eq!(text(&place(0x2a00_00f0, 20, 10)), "\x1b_Ga=p,U=1,q=2,i=704643312,p=1,c=20,r=10\x1b\\");
    }

    #[test]
    fn deletes_the_picture_and_its_data() {
        assert_eq!(text(&delete(0x2a00_00f0)), "\x1b_Ga=d,d=I,q=2,i=704643312\x1b\\");
    }

    #[rstest]
    #[case::fits_one_chunk(3072, 1)]
    #[case::just_over_one_chunk(3073, 2)]
    #[case::two_whole_chunks(6144, 2)]
    #[case::three_chunks(6145, 3)]
    #[case::many_chunks(100_000, 33)]
    fn cuts_the_data_into_chunks_of_at_most_4096(#[case] size: usize, #[case] count: usize) {
        let data: Vec<u8> = (0..size).map(|at| u8::try_from(at % 251).expect("a byte")).collect();

        let commands = transmit(9, &data);

        assert_eq!(commands.len(), count);
        let chunks: Vec<&[u8]> = commands.iter().map(|command| payload(command)).collect();
        assert!(chunks.iter().all(|chunk| chunk.len() <= CHUNK));
        assert!(chunks[..count - 1].iter().all(|chunk| chunk.len() % 4 == 0));
        assert_eq!(STANDARD.decode(chunks.concat()).expect("base64"), data);
        if count > 1 {
            assert!(text(&commands[0]).starts_with("\x1b_Ga=t,q=2,i=9,f=100,m=1;"));
            assert!(commands[1..count - 1].iter().all(|command| text(command).starts_with("\x1b_Gm=1,q=2;")));
            assert!(text(&commands[count - 1]).starts_with("\x1b_Gm=0,q=2;"));
        }
    }
}

mod cells {
    use super::*;

    #[rstest]
    #[case::top_left(0, 0, ['\u{0305}', '\u{0305}'])]
    #[case::second_row(1, 0, ['\u{030D}', '\u{0305}'])]
    #[case::second_column(0, 1, ['\u{0305}', '\u{030D}'])]
    #[case::last_row_and_column(296, 296, ['\u{1D244}', '\u{1D244}'])]
    #[case::beyond_the_table(400, 297, ['\u{1D244}', '\u{1D244}'])]
    fn marks_row_and_column_with_diacritics(#[case] row: u16, #[case] col: u16, #[case] marks: [char; 2]) {
        let (symbol, _) = cell(id(1, 0xf0), row, col);

        assert_eq!(symbol.chars().collect::<Vec<_>>(), ['\u{10EEEE}', marks[0], marks[1], '\u{030D}']);
    }

    #[rstest]
    #[case::low_high_byte(1, 0xf0, '\u{030D}')]
    #[case::middle_high_byte(42, 0xf1, DIACRITICS[42])]
    #[case::highest_high_byte(255, 0xf0, DIACRITICS[255])]
    fn carries_the_id_in_the_colour_and_the_third_diacritic(#[case] hi: u8, #[case] lo: u8, #[case] mark: char) {
        let (symbol, colour) = cell(id(hi, lo), 3, 4);

        assert_eq!(colour, lo);
        assert_eq!(symbol.chars().last(), Some(mark));
    }

    #[test]
    fn takes_kitty_s_table_in_its_order() {
        assert_eq!(DIACRITICS.len(), usize::from(MAX_CELLS));
        assert_eq!((DIACRITICS[0], DIACRITICS[1], DIACRITICS[296]), ('\u{0305}', '\u{030D}', '\u{1D244}'));
        assert!(DIACRITICS.windows(2).all(|pair| pair[0] < pair[1]));
    }

    fn assert_row_is_one_column_wide(hi: u8, row: u16) {
        for col in 0..MAX_CELLS {
            let (symbol, _) = cell(id(hi, 0xf0), row, col);

            assert_eq!(Span::raw(symbol.as_str()).width(), 1, "row {row} col {col} hi {hi}");
        }
    }

    #[test]
    fn every_cell_is_one_column_wide() {
        for hi in [1, 42, 255] {
            for row in 0..MAX_CELLS {
                assert_row_is_one_column_wide(hi, row);
            }
        }
    }
}

mod pictures {
    use super::*;

    fn sent(out: &[u8]) -> RgbaImage {
        let commands: Vec<&[u8]> = out.split_inclusive(|&byte| byte == b'\\').collect();
        let data: Vec<u8> =
            commands[..commands.len() - 1].iter().flat_map(|command| payload(command).to_vec()).collect();
        image_of(&STANDARD.decode(data).expect("base64"))
    }

    #[test]
    fn sends_a_dot_and_places_it() {
        let picture = picture(RgbaImage::from_pixel(1, 1, RED), "PNG");
        let fit = fit(1, 1, Some(cell_size(10, 20)), (80, 40), Protocol::Kitty).expect("room");

        let out = encode(&picture, &key(Protocol::Kitty, fit, None));

        assert_eq!(
            text(&out),
            "\x1b_Ga=t,q=2,i=704643312,f=100;iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAAD0lEQVR4AQEEAPv/AP8AAAMBAQCNHeWCAAAAAElFTkSuQmCC\x1b\\\x1b_Ga=p,U=1,q=2,i=704643312,p=1,c=1,r=1\x1b\\"
        );
    }

    #[test]
    fn sends_the_pixels_the_cells_show() {
        let picture = picture(RgbaImage::from_pixel(400, 200, RED), "PNG");
        let fit = Fit { cols: 10, rows: 3, width: 100, height: 50, cell: cell_size(10, 20) };

        let out = encode(&picture, &key(Protocol::Kitty, fit, Some((1, 2, 3))));

        assert_eq!(sent(&out), RgbaImage::from_pixel(100, 50, RED));
        assert!(text(&out).ends_with("\x1b_Ga=p,U=1,q=2,i=704643312,p=1,c=10,r=3\x1b\\"));
    }

    #[test]
    fn never_enlarges_the_pixels_it_has() {
        let picture = picture(RgbaImage::from_pixel(2048, 8, RED), "PNG");
        let fit = Fit { cols: 300, rows: 1, width: 3000, height: 12, cell: cell_size(10, 20) };

        assert_eq!(sent(&encode(&picture, &key(Protocol::Kitty, fit, None))).dimensions(), (2048, 8));
    }

    #[test]
    fn keeps_see_through_pixels_for_the_terminal_to_blend() {
        let picture = picture(RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 255, 100])), "PNG");
        let fit = fit(4, 4, Some(cell_size(10, 20)), (80, 40), Protocol::Kitty).expect("room");

        assert_eq!(sent(&encode(&picture, &key(Protocol::Kitty, fit, Some((9, 9, 9))))), picture.rgba);
    }

    #[test]
    fn wraps_for_tmux_only_when_asked() {
        let picture = picture(RgbaImage::from_pixel(1, 1, RED), "PNG");
        let fit = fit(1, 1, Some(cell_size(10, 20)), (80, 40), Protocol::Kitty).expect("room");
        let plain = key(Protocol::Kitty, fit, None);

        let wrapped = encode(&picture, &Key { tmux: Tmux::Wrap, ..plain });

        assert!(wrapped.starts_with(b"\x1bPtmux;\x1b\x1b_Ga=t,q=2,"));
        assert_eq!(wrapped.windows(7).filter(|window| window == b"\x1bPtmux;").count(), 2);
    }
}
