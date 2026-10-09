use std::borrow::Cow;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::imageops::{self, FilterType as Filter};
use image::{ExtendedColorType, ImageEncoder, RgbaImage};

use super::{CellSize, Picture, Protocol, Tmux, iterm, kitty, sixel, tmux};

pub const DEFAULT_CELL: CellSize = CellSize { width: 8, height: 16 };
pub const ITERM_ROWS: u16 = 255;
pub const SIXEL_PIXELS: u32 = 1_200_000;
pub const BUDGET: usize = tmux::LIMIT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fit {
    pub cols: u16,
    pub rows: u16,
    pub width: u32,
    pub height: u32,
    pub cell: CellSize,
}

pub fn fit(width: u32, height: u32, cell: Option<CellSize>, room: (u16, u16), protocol: Protocol) -> Option<Fit> {
    let cell = cell.filter(|cell| cell.width > 0 && cell.height > 0).unwrap_or(DEFAULT_CELL);
    let (cols, rows) = match protocol {
        Protocol::Kitty => (room.0.min(kitty::MAX_CELLS), room.1.min(kitty::MAX_CELLS)),
        Protocol::Iterm => (room.0, room.1.min(ITERM_ROWS)),
        Protocol::Sixel { .. } => room,
    };
    if width == 0 || height == 0 || cols == 0 || rows == 0 {
        return None;
    }
    let (w, h) = (f64::from(width), f64::from(height));
    let room_width = f64::from(u32::from(cols) * u32::from(cell.width));
    let room_height = f64::from(u32::from(rows) * u32::from(cell.height));
    let mut scale = (room_width / w).min(room_height / h).min(1.0);
    if let Protocol::Sixel { max, .. } = protocol {
        if let Some((max_width, max_height)) = max.filter(|&(max_width, max_height)| max_width > 0 && max_height > 0) {
            scale = scale.min(f64::from(max_width) / w).min(f64::from(max_height) / h);
        }
        scale = scale.min((f64::from(SIXEL_PIXELS) / (w * h)).sqrt());
    }
    let (width, height) = (round(w * scale).max(1), round(h * scale).max(1));
    let (width, height) =
        if let Protocol::Sixel { .. } = protocol { sixel::bands(width, height) } else { (width, height) };
    let cells = |pixels: u32, size: u16, most: u16| {
        u16::try_from(pixels.div_ceil(u32::from(size))).unwrap_or(most).clamp(1, most)
    };
    Some(Fit { cols: cells(width, cell.width, cols), rows: cells(height, cell.height, rows), width, height, cell })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub picture: u64,
    pub protocol: Protocol,
    pub tmux: Tmux,
    pub fit: Fit,
    pub background: Option<(u8, u8, u8)>,
    pub kitty_id: u32,
}

pub fn encode(picture: &Picture, key: &Key) -> Vec<u8> {
    match key.protocol {
        Protocol::Kitty => join(&kitty::image(picture, key), key.tmux),
        Protocol::Iterm => {
            let build = |scale| join(&[iterm::image(picture, key, scale)], key.tmux);
            let out = build(1.0);
            if key.tmux == Tmux::Wrap { within(out, build) } else { out }
        }
        Protocol::Sixel { registers, .. } => {
            let build = |colours, scale| join(&[sixel::image(picture, key, colours, scale)], key.tmux);
            let most = sixel::colours(registers);
            let out = build(most, 1.0);
            if out.len() <= BUDGET || most <= sixel::FEWEST {
                return within(out, |scale| build(most, scale));
            }
            within(build(sixel::FEWEST, 1.0), |scale| build(sixel::FEWEST, scale))
        }
    }
}

fn join(sequences: &[Vec<u8>], tmux: Tmux) -> Vec<u8> {
    match tmux {
        Tmux::None => sequences.concat(),
        Tmux::Wrap => sequences.iter().flat_map(|sequence| tmux::wrap(sequence)).collect(),
    }
}

fn within(first: Vec<u8>, build: impl Fn(f64) -> Vec<u8>) -> Vec<u8> {
    let mut scale = 1.0;
    let mut out = first;
    for _ in 0..3 {
        if out.is_empty() || out.len() <= BUDGET {
            break;
        }
        scale *= share(BUDGET, out.len()).sqrt() * 0.9;
        out = build(scale);
    }
    out
}

pub(super) fn sized(picture: &Picture, width: u32, height: u32) -> Cow<'_, RgbaImage> {
    if picture.rgba.dimensions() == (width, height) {
        Cow::Borrowed(&picture.rgba)
    } else {
        Cow::Owned(resize(Cow::Borrowed(&picture.rgba), width, height))
    }
}

pub(super) fn resize(image: Cow<'_, RgbaImage>, width: u32, height: u32) -> RgbaImage {
    let (from_width, from_height) = image.dimensions();
    if (from_width, from_height) == (width, height) {
        return image.into_owned();
    }
    let opaque = image.pixels().all(|pixel| pixel[3] == u8::MAX);
    let source = if opaque {
        image
    } else {
        let mut owned = image.into_owned();
        premultiply(&mut owned);
        Cow::Owned(owned)
    };
    let mut out = if width <= from_width && height <= from_height {
        imageops::thumbnail(source.as_ref(), width, height)
    } else {
        imageops::resize(source.as_ref(), width, height, Filter::Triangle)
    };
    if !opaque {
        unpremultiply(&mut out);
    }
    out
}

fn premultiply(image: &mut RgbaImage) {
    for pixel in image.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = narrow((u16::from(*channel) * alpha + 127) / 255);
        }
    }
}

fn unpremultiply(image: &mut RgbaImage) {
    for pixel in image.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = (u16::from(*channel) * 255 + alpha / 2).checked_div(alpha).map_or(0, narrow);
        }
    }
}

pub(super) fn flatten(image: &mut RgbaImage, background: (u8, u8, u8)) {
    let (red, green, blue) = background;
    for pixel in image.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for (channel, under) in pixel.0[..3].iter_mut().zip([red, green, blue]) {
            *channel = narrow((u16::from(*channel) * alpha + u16::from(under) * (255 - alpha) + 127) / 255);
        }
        pixel[3] = u8::MAX;
    }
}

pub(super) fn png(image: &RgbaImage, opaque: bool) -> Vec<u8> {
    let mut out = Vec::new();
    let encoder = PngEncoder::new_with_quality(&mut out, CompressionType::Fast, FilterType::Adaptive);
    let written = if opaque {
        encoder.write_image(&rgb(image), image.width(), image.height(), ExtendedColorType::Rgb8)
    } else {
        encoder.write_image(image.as_raw(), image.width(), image.height(), ExtendedColorType::Rgba8)
    };
    if written.is_ok() { out } else { Vec::new() }
}

pub(super) fn jpeg(image: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    let encoder = JpegEncoder::new_with_quality(&mut out, 85);
    let written = encoder.write_image(&rgb(image), image.width(), image.height(), ExtendedColorType::Rgb8);
    if written.is_ok() { out } else { Vec::new() }
}

fn rgb(image: &RgbaImage) -> Vec<u8> {
    image.pixels().flat_map(|pixel| [pixel[0], pixel[1], pixel[2]]).collect()
}

fn narrow(value: u16) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "pixel sizes are positive and small")]
pub(super) fn round(value: f64) -> u32 {
    value.round().max(0.0) as u32
}

#[expect(clippy::cast_precision_loss, reason = "payload sizes stay far below 2^52 bytes")]
fn share(part: usize, whole: usize) -> f64 {
    part as f64 / whole as f64
}

#[cfg(test)]
pub mod tests {
    use image::Rgba;
    use rstest::rstest;

    use super::*;

    pub const RED: Rgba<u8> = Rgba([255, 0, 0, 255]);
    pub const CLEAR: Rgba<u8> = Rgba([0, 0, 0, 0]);
    pub const SIXEL: Protocol = Protocol::Sixel { registers: 256, max: None };

    pub fn picture(rgba: RgbaImage, format: &'static str) -> Picture {
        let (width, height) = rgba.dimensions();
        let opaque = rgba.pixels().all(|pixel| pixel[3] == u8::MAX);
        Picture { id: 1, format, width, height, bytes: 0, rgba, opaque }
    }

    pub fn cell(width: u16, height: u16) -> CellSize {
        CellSize { width, height }
    }

    pub fn key(protocol: Protocol, fit: Fit, background: Option<(u8, u8, u8)>) -> Key {
        Key { picture: 1, protocol, tmux: Tmux::None, fit, background, kitty_id: kitty::id(42, 0xf0) }
    }

    pub fn noise(width: u32, height: u32) -> RgbaImage {
        let mut seed = 0x2545_f491_u32;
        RgbaImage::from_fn(width, height, |_, _| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let [red, green, blue, _] = seed.to_le_bytes();
            Rgba([red, green, blue, 255])
        })
    }

    pub fn photo(width: u32, height: u32) -> RgbaImage {
        let grain = noise(width, height);
        RgbaImage::from_fn(width, height, |x, y| {
            let shade =
                |at: u32, of: u32, grain: u8| u8::try_from(at * 200 / of + u32::from(grain % 24)).unwrap_or(255);
            let speck = grain.get_pixel(x, y);
            Rgba([shade(x, width, speck[0]), shade(y, height, speck[1]), shade(x + y, width + height, speck[2]), 255])
        })
    }

    pub fn image_of(bytes: &[u8]) -> RgbaImage {
        image::load_from_memory(bytes).expect("a picture the terminal can read").into_rgba8()
    }

    mod fit {
        use super::*;

        #[rstest]
        #[case::keeps_a_small_picture_at_its_size((100, 50), Some(cell(10, 20)), (80, 40), Protocol::Kitty, (10, 3, 100, 50))]
        #[case::fills_whole_cells((200, 190), Some(cell(10, 19)), (40, 16), Protocol::Kitty, (20, 10, 200, 190))]
        #[case::shrinks_to_the_width((1200, 150), Some(cell(10, 19)), (60, 30), Protocol::Kitty, (60, 4, 600, 75))]
        #[case::shrinks_to_the_height((150, 900), Some(cell(10, 19)), (60, 30), Protocol::Kitty, (10, 30, 95, 570))]
        #[case::assumes_a_cell_when_unknown((100, 100), None, (80, 40), Protocol::Iterm, (13, 7, 100, 100))]
        #[case::assumes_a_cell_when_empty((100, 100), Some(cell(0, 20)), (80, 40), Protocol::Iterm, (13, 7, 100, 100))]
        #[case::gives_a_dot_one_cell((1, 1), Some(cell(10, 20)), (80, 40), Protocol::Kitty, (1, 1, 1, 1))]
        #[case::kitty_counts_at_most_297_columns((10_000, 100), Some(cell(10, 20)), (400, 400), Protocol::Kitty, (297, 2, 2970, 30))]
        #[case::iterm_takes_every_column((10_000, 100), Some(cell(10, 20)), (400, 400), Protocol::Iterm, (400, 2, 4000, 40))]
        #[case::iterm_counts_at_most_255_rows((100, 100_000), Some(cell(10, 20)), (100, 300), Protocol::Iterm, (1, 255, 5, 5100))]
        #[case::sixel_stays_within_the_terminal_geometry(
            (4000, 1000),
            Some(cell(10, 20)),
            (300, 100),
            Protocol::Sixel { registers: 256, max: Some((1000, 1000)) },
            (99, 13, 984, 246)
        )]
        #[case::sixel_stays_within_its_area((2000, 2000), Some(cell(10, 20)), (300, 150), SIXEL, (110, 55, 1092, 1092))]
        #[case::sixel_ignores_an_empty_geometry(
            (100, 50),
            Some(cell(10, 20)),
            (80, 40),
            Protocol::Sixel { registers: 256, max: Some((0, 0)) },
            (10, 3, 96, 48)
        )]
        fn sizes_the_picture_in_cells_and_pixels(
            #[case] size: (u32, u32),
            #[case] cell: Option<CellSize>,
            #[case] room: (u16, u16),
            #[case] protocol: Protocol,
            #[case] expected: (u16, u16, u32, u32),
        ) {
            let fit = fit(size.0, size.1, cell, room, protocol).expect("room for the picture");

            assert_eq!((fit.cols, fit.rows, fit.width, fit.height), expected);
            assert_eq!(fit.cell, cell.filter(|cell| cell.width > 0).unwrap_or(DEFAULT_CELL));
        }

        #[rstest]
        #[case::no_columns((100, 100), (0, 10))]
        #[case::no_rows((100, 100), (10, 0))]
        #[case::no_width((0, 100), (10, 10))]
        #[case::no_height((100, 0), (10, 10))]
        fn has_nothing_to_show_without_room_or_pixels(#[case] size: (u32, u32), #[case] room: (u16, u16)) {
            assert_eq!(fit(size.0, size.1, Some(cell(10, 20)), room, Protocol::Kitty), None);
        }

        #[test]
        fn never_enlarges_never_overflows_and_keeps_the_shape() {
            let sizes = [1, 7, 16, 99, 640, 1000, 2047, 4096, 16_384];
            let cells = [cell(7, 15), cell(10, 19), cell(16, 34)];
            let rooms = [(1, 1), (3, 40), (80, 24), (500, 400)];
            let protocols = [Protocol::Kitty, Protocol::Iterm, SIXEL];
            for (&width, &height, &cell, &room, &protocol) in itertools(&sizes, &cells, &rooms, &protocols) {
                let fit = fit(width, height, Some(cell), room, protocol).expect("room");
                let (cw, ch) = (u32::from(cell.width), u32::from(cell.height));
                let case = format!("{width}×{height} in {room:?} at {cw}×{ch} for {protocol:?}: {fit:?}");

                assert!(fit.cols >= 1 && fit.cols <= room.0 && fit.rows >= 1 && fit.rows <= room.1, "{case}");
                assert!(protocol != Protocol::Kitty || fit.cols.max(fit.rows) <= kitty::MAX_CELLS, "{case}");
                assert!(protocol != Protocol::Iterm || fit.rows <= ITERM_ROWS, "{case}");
                assert!(
                    protocol != SIXEL || fit.height.is_multiple_of(sixel::BAND) || fit.height < sixel::BAND,
                    "{case}"
                );
                assert!(fit.width <= width && fit.height <= height, "{case}");
                assert!(fit.width <= u32::from(fit.cols) * cw && fit.width > u32::from(fit.cols - 1) * cw, "{case}");
                assert!(fit.height <= u32::from(fit.rows) * ch && fit.height > u32::from(fit.rows - 1) * ch, "{case}");
                if fit.width > 1 && fit.height > 1 {
                    let skew =
                        (u64::from(fit.width) * u64::from(height)).abs_diff(u64::from(fit.height) * u64::from(width));
                    let banded = if protocol == SIXEL { u64::from(height) / 2 } else { 0 };
                    assert!(skew <= u64::from(width + height) / 2 + banded + 1, "{case}");
                }
            }
        }

        fn itertools<'a, A, B, C, D>(
            a: &'a [A],
            b: &'a [B],
            c: &'a [C],
            d: &'a [D],
        ) -> Vec<(&'a A, &'a A, &'a B, &'a C, &'a D)> {
            let mut out = Vec::new();
            for x in a {
                for y in a {
                    for z in b {
                        for w in c {
                            for v in d {
                                out.push((x, y, z, w, v));
                            }
                        }
                    }
                }
            }
            out
        }
    }

    mod pixels {
        use super::*;

        #[test]
        fn shrinks_see_through_pixels_without_darkening_their_edges() {
            let image = RgbaImage::from_fn(4, 2, |x, _| if x % 2 == 0 { Rgba([255, 255, 255, 255]) } else { CLEAR });

            let small = resize(Cow::Borrowed(&image), 2, 1);

            assert!(small.pixels().all(|pixel| pixel.0 == [255, 255, 255, 128]), "{:?}", small.as_raw());
        }

        #[test]
        fn enlarges_when_asked_for_more_pixels() {
            let image = RgbaImage::from_pixel(2, 2, RED);

            assert_eq!(resize(Cow::Borrowed(&image), 5, 3), RgbaImage::from_pixel(5, 3, RED));
        }

        #[rstest]
        #[case::solid(RED, (9, 9, 9), [255, 0, 0, 255])]
        #[case::invisible(CLEAR, (9, 9, 9), [9, 9, 9, 255])]
        #[case::half(Rgba([255, 0, 0, 128]), (0, 0, 255), [128, 0, 127, 255])]
        fn lays_a_picture_on_the_background(
            #[case] pixel: Rgba<u8>,
            #[case] background: (u8, u8, u8),
            #[case] expected: [u8; 4],
        ) {
            let mut image = RgbaImage::from_pixel(1, 1, pixel);

            flatten(&mut image, background);

            assert_eq!(image.get_pixel(0, 0).0, expected);
        }

        #[rstest]
        #[case::opaque_drops_the_alpha_channel(true, 2)]
        #[case::see_through_keeps_it(false, 6)]
        fn writes_a_png_with_only_the_channels_it_needs(#[case] opaque: bool, #[case] colour_type: u8) {
            let image = RgbaImage::from_pixel(3, 2, RED);

            let png = png(&image, opaque);

            assert_eq!(png[25], colour_type);
            assert_eq!(image_of(&png), image);
        }

        #[test]
        fn writes_a_jpeg() {
            let jpeg = jpeg(&RgbaImage::from_pixel(16, 16, RED));

            assert!(jpeg.starts_with(&[0xff, 0xd8, 0xff]));
            assert_eq!(image_of(&jpeg).dimensions(), (16, 16));
        }
    }

    mod budget {
        use super::*;

        #[test]
        fn shrinks_a_payload_until_tmux_takes_it() {
            let scales = std::cell::RefCell::new(Vec::new());
            let build = |scale: f64| {
                scales.borrow_mut().push(scale);
                vec![0; usize::try_from(round(3_000_000.0 * scale * scale)).expect("a size")]
            };

            let out = within(build(1.0), build);

            assert!(out.len() <= BUDGET, "{}", out.len());
            assert_eq!(scales.borrow().len(), 2);
        }

        #[test]
        fn leaves_a_payload_that_fits_alone() {
            assert_eq!(within(vec![0; BUDGET], |_| Vec::new()).len(), BUDGET);
        }

        #[test]
        fn gives_up_after_a_few_tries() {
            let tries = std::cell::Cell::new(0);

            let out = within(vec![0; BUDGET + 1], |_| {
                tries.set(tries.get() + 1);
                vec![0; BUDGET + 1]
            });

            assert_eq!((out.len(), tries.get()), (BUDGET + 1, 3));
        }
    }

    mod passthrough {
        use super::*;

        #[test]
        fn wraps_each_command_on_its_own() {
            let picture = picture(noise(64, 64), "PNG");
            let fit = fit(64, 64, Some(cell(8, 16)), (80, 40), Protocol::Kitty).expect("room");
            let plain = key(Protocol::Kitty, fit, None);
            let wrapped = Key { tmux: Tmux::Wrap, ..plain };

            let commands = kitty::image(&picture, &plain);
            let out = encode(&picture, &wrapped);

            assert!(commands.len() > 2);
            assert_eq!(out, commands.iter().flat_map(|command| tmux::wrap(command)).collect::<Vec<u8>>());
            assert_eq!(encode(&picture, &plain), commands.concat());
        }

        #[rstest]
        #[case::iterm(Protocol::Iterm)]
        #[case::sixel(SIXEL)]
        fn wraps_a_single_sequence_whole(#[case] protocol: Protocol) {
            let picture = picture(RgbaImage::from_pixel(8, 16, RED), "PNG");
            let fit = fit(8, 16, Some(cell(8, 16)), (80, 40), protocol).expect("room");
            let plain = key(protocol, fit, None);

            let wrapped = encode(&picture, &Key { tmux: Tmux::Wrap, ..plain });

            assert_eq!(wrapped, tmux::wrap(&encode(&picture, &plain)));
        }
    }
}
