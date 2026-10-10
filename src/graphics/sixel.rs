use icy_sixel::{EncodeOptions, QuantizeMethod, SixelImage};
use image::RgbaImage;

use super::Picture;
use super::encode::{Key, flatten, round, sized};

pub const COLOURS: u16 = 256;
pub const FEWEST: u16 = 64;
pub const DIFFUSION: f32 = 0.875;
pub const BAND: u32 = 6;

pub fn image(picture: &Picture, key: &Key, colours: u16, scale: f64) -> Vec<u8> {
    let fit = key.fit;
    let side = |pixels: u32| round(f64::from(pixels) * scale).max(1);
    let (width, height) = bands(side(fit.width), side(fit.height));
    let mut image = sized(picture, width, height).into_owned();
    if let Some(background) = key.background {
        flatten(&mut image, background);
    }
    encode(image, colours)
}

pub fn bands(width: u32, height: u32) -> (u32, u32) {
    let whole = height - height % BAND;
    if whole == 0 || whole == height {
        return (width, height);
    }
    (round(f64::from(width) * f64::from(whole) / f64::from(height)).max(1), whole)
}

pub fn colours(registers: u16) -> u16 {
    if registers == 0 { COLOURS } else { registers.clamp(2, COLOURS) }
}

pub fn encode(image: RgbaImage, colours: u16) -> Vec<u8> {
    let (width, height) = (image.width() as usize, image.height() as usize);
    let options = EncodeOptions { max_colors: colours, diffusion: DIFFUSION, quantize_method: QuantizeMethod::Wu };
    let Ok(text) = SixelImage::from_rgba(image.into_raw(), width, height).encode_with(&options) else {
        return Vec::new();
    };
    let mut out = text.into_bytes();
    if out.ends_with(b"-\x1b\\") {
        out.remove(out.len() - 3);
    }
    out
}

#[cfg(test)]
mod tests {
    use image::Rgba;
    use rstest::rstest;

    use super::super::Protocol;
    use super::super::encode::tests::{CLEAR, RED, SIXEL, cell, key, noise, photo, picture};
    use super::super::encode::{BUDGET, Fit, encode, fit};
    use super::*;

    const BLUE: Rgba<u8> = Rgba([0, 0, 255, 255]);

    fn shown(sixel: &[u8]) -> RgbaImage {
        let image = SixelImage::decode(sixel).expect("a sixel image");
        RgbaImage::from_raw(
            u32::try_from(image.width).expect("small"),
            u32::try_from(image.height).expect("small"),
            image.pixels,
        )
        .expect("whole rows")
    }

    fn exact(width: u32, height: u32) -> Fit {
        Fit { cols: 1, rows: 1, width, height, cell: cell(10, 20) }
    }

    #[test]
    fn writes_a_tiny_picture_with_its_raster_size_and_palette() {
        let picture = picture(RgbaImage::from_fn(2, 2, |x, y| if x == y { RED } else { BLUE }), "PNG");

        let out = encode(&picture, &key(SIXEL, exact(2, 2), Some((0, 0, 0))));

        assert_eq!(
            std::str::from_utf8(&out).expect("ASCII"),
            "\x1bP9;1;0q\"1;1;2;2#0;2;100;0;0#1;2;0;0;100#0@A$#1A@$\x1b\\"
        );
    }

    #[test]
    fn draws_from_the_cursor_and_leaves_no_line_feed_after_the_last_band() {
        let picture = picture(photo(42, 42), "PNG");

        let out = encode(&picture, &key(SIXEL, exact(42, 42), Some((0, 0, 0))));

        assert!(out.starts_with(b"\x1bP9;1;0q\"1;1;42;42#"));
        assert!(out.ends_with(b"$\x1b\\") && !out.ends_with(b"-\x1b\\"));
        for mode in [&b"\x1b[?80"[..], b"8452", b"1070"] {
            assert!(!out.windows(mode.len()).any(|window| window == mode));
        }
    }

    #[test]
    fn draws_exactly_the_pixels_of_the_fit() {
        let picture = picture(RgbaImage::from_fn(400, 300, |x, _| if x < 200 { RED } else { BLUE }), "PNG");
        let fit = fit(400, 300, Some(cell(10, 20)), (10, 3), SIXEL).expect("room");

        let image = shown(&encode(&picture, &key(SIXEL, fit, Some((0, 0, 0)))));

        assert_eq!((fit.width, fit.height), (80, 60));
        assert_eq!(image.dimensions(), (80, 60));
        assert_eq!((image.get_pixel(0, 0), image.get_pixel(79, 59)), (&RED, &BLUE));
    }

    #[rstest]
    #[case::whole_bands(60, 42, (60, 42))]
    #[case::cut_to_whole_bands(100, 50, (96, 48))]
    #[case::thinner_than_a_band(9, 4, (9, 4))]
    #[case::one_band_and_a_row(7, 7, (6, 6))]
    fn keeps_a_picture_in_whole_bands(#[case] width: u32, #[case] height: u32, #[case] expected: (u32, u32)) {
        assert_eq!(bands(width, height), expected);
    }

    #[test]
    fn never_draws_a_band_it_cannot_fill() {
        let picture = picture(photo(100, 50), "PNG");

        let out = encode(&picture, &key(SIXEL, exact(100, 50), None));

        assert_eq!(raster(&out), "1;1;96;48");
    }

    #[test]
    fn lays_see_through_pixels_on_the_background() {
        let picture = picture(RgbaImage::from_fn(6, 6, |x, _| if x < 3 { CLEAR } else { RED }), "PNG");

        let image = shown(&encode(&picture, &key(SIXEL, exact(6, 6), Some((51, 102, 153)))));

        assert_eq!((image.get_pixel(0, 0), image.get_pixel(5, 5)), (&Rgba([51, 102, 153, 255]), &RED));
    }

    #[test]
    fn leaves_see_through_pixels_unpainted_when_the_background_is_unknown() {
        let picture = picture(RgbaImage::from_fn(6, 6, |x, _| if x < 3 { CLEAR } else { RED }), "PNG");

        let image = shown(&encode(&picture, &key(SIXEL, exact(6, 6), None)));

        assert_eq!((image.get_pixel(0, 0)[3], image.get_pixel(5, 5)), (0, &RED));
    }

    #[rstest]
    #[case::unknown_registers(0, 256)]
    #[case::too_few_registers(1, 2)]
    #[case::a_vt340(16, 16)]
    #[case::more_than_needed(1024, 256)]
    fn uses_at_most_the_registers_the_terminal_has(#[case] registers: u16, #[case] most: usize) {
        let picture = picture(photo(64, 64), "PNG");
        let protocol = Protocol::Sixel { registers, max: None };

        let out = encode(&picture, &key(protocol, exact(64, 64), Some((0, 0, 0))));

        let defined = palette(&out);
        assert_eq!(colours(registers), u16::try_from(most).expect("small"));
        assert!(defined <= most && defined > 1, "{defined}");
    }

    fn palette(sixel: &[u8]) -> usize {
        let text = std::str::from_utf8(sixel).expect("ASCII");
        text.split('#').filter(|colour| colour.split(';').nth(1) == Some("2")).count()
    }

    fn raster(sixel: &[u8]) -> &str {
        let text = std::str::from_utf8(sixel).expect("ASCII");
        text.split_once('"').and_then(|(_, rest)| rest.split('#').next()).expect("raster attributes")
    }

    #[test]
    fn spends_fewer_colours_before_fewer_pixels() {
        let picture = picture(photo(1000, 1000), "PNG");
        let fit = fit(1000, 1000, Some(cell(10, 20)), (100, 50), SIXEL).expect("room");

        let out = encode(&picture, &key(SIXEL, fit, Some((0, 0, 0))));

        assert!(image(&picture, &key(SIXEL, fit, Some((0, 0, 0))), COLOURS, 1.0).len() > BUDGET);
        assert!(out.len() <= BUDGET, "{}", out.len());
        assert_eq!(raster(&out), "1;1;996;996");
        assert!(palette(&out) <= usize::from(FEWEST));
    }

    #[test]
    fn shrinks_a_picture_too_heavy_even_in_few_colours() {
        let picture = picture(noise(700, 700), "PNG");
        let fit = fit(700, 700, Some(cell(10, 20)), (100, 50), SIXEL).expect("room");

        let out = encode(&picture, &key(SIXEL, fit, Some((0, 0, 0))));

        assert!(out.len() <= BUDGET, "{}", out.len());
        let width: u32 = raster(&out).split(';').nth(2).and_then(|width| width.parse().ok()).expect("a width");
        assert!((200..700).contains(&width), "{width}");
    }
}
