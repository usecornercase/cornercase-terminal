use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::Picture;
use super::encode::{Key, png, sized};

pub const CHUNK: usize = 4096;
pub const MAX_CELLS: u16 = 297;
const PLACEHOLDER: char = '\u{10EEEE}';

const DIACRITICS: [char; 297] = [
    '\u{0305}',
    '\u{030D}',
    '\u{030E}',
    '\u{0310}',
    '\u{0312}',
    '\u{033D}',
    '\u{033E}',
    '\u{033F}',
    '\u{0346}',
    '\u{034A}',
    '\u{034B}',
    '\u{034C}',
    '\u{0350}',
    '\u{0351}',
    '\u{0352}',
    '\u{0357}',
    '\u{035B}',
    '\u{0363}',
    '\u{0364}',
    '\u{0365}',
    '\u{0366}',
    '\u{0367}',
    '\u{0368}',
    '\u{0369}',
    '\u{036A}',
    '\u{036B}',
    '\u{036C}',
    '\u{036D}',
    '\u{036E}',
    '\u{036F}',
    '\u{0483}',
    '\u{0484}',
    '\u{0485}',
    '\u{0486}',
    '\u{0487}',
    '\u{0592}',
    '\u{0593}',
    '\u{0594}',
    '\u{0595}',
    '\u{0597}',
    '\u{0598}',
    '\u{0599}',
    '\u{059C}',
    '\u{059D}',
    '\u{059E}',
    '\u{059F}',
    '\u{05A0}',
    '\u{05A1}',
    '\u{05A8}',
    '\u{05A9}',
    '\u{05AB}',
    '\u{05AC}',
    '\u{05AF}',
    '\u{05C4}',
    '\u{0610}',
    '\u{0611}',
    '\u{0612}',
    '\u{0613}',
    '\u{0614}',
    '\u{0615}',
    '\u{0616}',
    '\u{0617}',
    '\u{0657}',
    '\u{0658}',
    '\u{0659}',
    '\u{065A}',
    '\u{065B}',
    '\u{065D}',
    '\u{065E}',
    '\u{06D6}',
    '\u{06D7}',
    '\u{06D8}',
    '\u{06D9}',
    '\u{06DA}',
    '\u{06DB}',
    '\u{06DC}',
    '\u{06DF}',
    '\u{06E0}',
    '\u{06E1}',
    '\u{06E2}',
    '\u{06E4}',
    '\u{06E7}',
    '\u{06E8}',
    '\u{06EB}',
    '\u{06EC}',
    '\u{0730}',
    '\u{0732}',
    '\u{0733}',
    '\u{0735}',
    '\u{0736}',
    '\u{073A}',
    '\u{073D}',
    '\u{073F}',
    '\u{0740}',
    '\u{0741}',
    '\u{0743}',
    '\u{0745}',
    '\u{0747}',
    '\u{0749}',
    '\u{074A}',
    '\u{07EB}',
    '\u{07EC}',
    '\u{07ED}',
    '\u{07EE}',
    '\u{07EF}',
    '\u{07F0}',
    '\u{07F1}',
    '\u{07F3}',
    '\u{0816}',
    '\u{0817}',
    '\u{0818}',
    '\u{0819}',
    '\u{081B}',
    '\u{081C}',
    '\u{081D}',
    '\u{081E}',
    '\u{081F}',
    '\u{0820}',
    '\u{0821}',
    '\u{0822}',
    '\u{0823}',
    '\u{0825}',
    '\u{0826}',
    '\u{0827}',
    '\u{0829}',
    '\u{082A}',
    '\u{082B}',
    '\u{082C}',
    '\u{082D}',
    '\u{0951}',
    '\u{0953}',
    '\u{0954}',
    '\u{0F82}',
    '\u{0F83}',
    '\u{0F86}',
    '\u{0F87}',
    '\u{135D}',
    '\u{135E}',
    '\u{135F}',
    '\u{17DD}',
    '\u{193A}',
    '\u{1A17}',
    '\u{1A75}',
    '\u{1A76}',
    '\u{1A77}',
    '\u{1A78}',
    '\u{1A79}',
    '\u{1A7A}',
    '\u{1A7B}',
    '\u{1A7C}',
    '\u{1B6B}',
    '\u{1B6D}',
    '\u{1B6E}',
    '\u{1B6F}',
    '\u{1B70}',
    '\u{1B71}',
    '\u{1B72}',
    '\u{1B73}',
    '\u{1CD0}',
    '\u{1CD1}',
    '\u{1CD2}',
    '\u{1CDA}',
    '\u{1CDB}',
    '\u{1CE0}',
    '\u{1DC0}',
    '\u{1DC1}',
    '\u{1DC3}',
    '\u{1DC4}',
    '\u{1DC5}',
    '\u{1DC6}',
    '\u{1DC7}',
    '\u{1DC8}',
    '\u{1DC9}',
    '\u{1DCB}',
    '\u{1DCC}',
    '\u{1DD1}',
    '\u{1DD2}',
    '\u{1DD3}',
    '\u{1DD4}',
    '\u{1DD5}',
    '\u{1DD6}',
    '\u{1DD7}',
    '\u{1DD8}',
    '\u{1DD9}',
    '\u{1DDA}',
    '\u{1DDB}',
    '\u{1DDC}',
    '\u{1DDD}',
    '\u{1DDE}',
    '\u{1DDF}',
    '\u{1DE0}',
    '\u{1DE1}',
    '\u{1DE2}',
    '\u{1DE3}',
    '\u{1DE4}',
    '\u{1DE5}',
    '\u{1DE6}',
    '\u{1DFE}',
    '\u{20D0}',
    '\u{20D1}',
    '\u{20D4}',
    '\u{20D5}',
    '\u{20D6}',
    '\u{20D7}',
    '\u{20DB}',
    '\u{20DC}',
    '\u{20E1}',
    '\u{20E7}',
    '\u{20E9}',
    '\u{20F0}',
    '\u{2CEF}',
    '\u{2CF0}',
    '\u{2CF1}',
    '\u{2DE0}',
    '\u{2DE1}',
    '\u{2DE2}',
    '\u{2DE3}',
    '\u{2DE4}',
    '\u{2DE5}',
    '\u{2DE6}',
    '\u{2DE7}',
    '\u{2DE8}',
    '\u{2DE9}',
    '\u{2DEA}',
    '\u{2DEB}',
    '\u{2DEC}',
    '\u{2DED}',
    '\u{2DEE}',
    '\u{2DEF}',
    '\u{2DF0}',
    '\u{2DF1}',
    '\u{2DF2}',
    '\u{2DF3}',
    '\u{2DF4}',
    '\u{2DF5}',
    '\u{2DF6}',
    '\u{2DF7}',
    '\u{2DF8}',
    '\u{2DF9}',
    '\u{2DFA}',
    '\u{2DFB}',
    '\u{2DFC}',
    '\u{2DFD}',
    '\u{2DFE}',
    '\u{2DFF}',
    '\u{A66F}',
    '\u{A67C}',
    '\u{A67D}',
    '\u{A6F0}',
    '\u{A6F1}',
    '\u{A8E0}',
    '\u{A8E1}',
    '\u{A8E2}',
    '\u{A8E3}',
    '\u{A8E4}',
    '\u{A8E5}',
    '\u{A8E6}',
    '\u{A8E7}',
    '\u{A8E8}',
    '\u{A8E9}',
    '\u{A8EA}',
    '\u{A8EB}',
    '\u{A8EC}',
    '\u{A8ED}',
    '\u{A8EE}',
    '\u{A8EF}',
    '\u{A8F0}',
    '\u{A8F1}',
    '\u{AAB0}',
    '\u{AAB2}',
    '\u{AAB3}',
    '\u{AAB7}',
    '\u{AAB8}',
    '\u{AABE}',
    '\u{AABF}',
    '\u{AAC1}',
    '\u{FE20}',
    '\u{FE21}',
    '\u{FE22}',
    '\u{FE23}',
    '\u{FE24}',
    '\u{FE25}',
    '\u{FE26}',
    '\u{10A0F}',
    '\u{10A38}',
    '\u{1D185}',
    '\u{1D186}',
    '\u{1D187}',
    '\u{1D188}',
    '\u{1D189}',
    '\u{1D1AA}',
    '\u{1D1AB}',
    '\u{1D1AC}',
    '\u{1D1AD}',
    '\u{1D242}',
    '\u{1D243}',
    '\u{1D244}',
];

pub fn id(hi: u8, lo: u8) -> u32 {
    (u32::from(hi) << 24) | u32::from(lo)
}

pub fn image(picture: &Picture, key: &Key) -> Vec<Vec<u8>> {
    let fit = key.fit;
    let (width, height) = picture.rgba.dimensions();
    let pixels = if fit.width < width && fit.height < height {
        sized(picture, fit.width, fit.height)
    } else {
        sized(picture, width, height)
    };
    let mut out = transmit(key.kitty_id, &png(&pixels, picture.opaque));
    out.push(place(key.kitty_id, fit.cols, fit.rows));
    out
}

pub fn transmit(id: u32, png: &[u8]) -> Vec<Vec<u8>> {
    let data = STANDARD.encode(png);
    let mut chunks: Vec<&[u8]> = data.as_bytes().chunks(CHUNK).collect();
    if chunks.is_empty() {
        chunks.push(b"");
    }
    let last = chunks.len() - 1;
    chunks
        .into_iter()
        .enumerate()
        .map(|(at, chunk)| {
            let keys = match at {
                0 if last == 0 => format!("a=t,q=2,i={id},f=100"),
                0 => format!("a=t,q=2,i={id},f=100,m=1"),
                at if at == last => "m=0,q=2".to_string(),
                _ => "m=1,q=2".to_string(),
            };
            command(&keys, chunk)
        })
        .collect()
}

pub fn place(id: u32, cols: u16, rows: u16) -> Vec<u8> {
    command(&format!("a=p,U=1,q=2,i={id},p=1,c={cols},r={rows}"), b"")
}

pub fn delete(id: u32) -> Vec<u8> {
    command(&format!("a=d,d=I,q=2,i={id}"), b"")
}

fn command(keys: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(keys.len() + payload.len() + 6);
    out.extend_from_slice(b"\x1b_G");
    out.extend_from_slice(keys.as_bytes());
    if !payload.is_empty() {
        out.push(b';');
        out.extend_from_slice(payload);
    }
    out.extend_from_slice(b"\x1b\\");
    out
}

pub fn cell(id: u32, row: u16, col: u16) -> (String, u8) {
    let [hi, _, _, lo] = id.to_be_bytes();
    let symbol = [PLACEHOLDER, diacritic(row), diacritic(col), diacritic(u16::from(hi))].into_iter().collect();
    (symbol, lo)
}

fn diacritic(value: u16) -> char {
    DIACRITICS.get(usize::from(value)).copied().unwrap_or(DIACRITICS[DIACRITICS.len() - 1])
}

#[cfg(test)]
mod tests {
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

        #[test]
        fn every_cell_is_one_column_wide() {
            for hi in [1, 42, 255] {
                for row in 0..MAX_CELLS {
                    for col in 0..MAX_CELLS {
                        let (symbol, _) = cell(id(hi, 0xf0), row, col);

                        assert_eq!(Span::raw(symbol.as_str()).width(), 1, "row {row} col {col} hi {hi}");
                    }
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
}
