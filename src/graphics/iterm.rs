use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{Rgba, RgbaImage, imageops};

use super::Picture;
use super::encode::{Key, flatten, jpeg, png, round, sized};

pub fn image(picture: &Picture, key: &Key, scale: f64) -> Vec<u8> {
    let fit = key.fit;
    let side = |pixels: u32| round(f64::from(pixels) * scale).max(1);
    let canvas_width = side(u32::from(fit.cols) * u32::from(fit.cell.width));
    let canvas_height = side(u32::from(fit.rows) * u32::from(fit.cell.height));
    let (width, height) = (side(fit.width).min(canvas_width), side(fit.height).min(canvas_height));
    let mut image = sized(picture, width, height).into_owned();
    let fill = match key.background {
        Some((red, green, blue)) => {
            flatten(&mut image, (red, green, blue));
            Rgba([red, green, blue, u8::MAX])
        }
        None => Rgba([0, 0, 0, 0]),
    };
    let mut canvas = RgbaImage::from_pixel(canvas_width, canvas_height, fill);
    let x = i64::from((canvas_width - width) / 2);
    let y = i64::from((canvas_height - height) / 2);
    imageops::replace(&mut canvas, &image, x, y);
    let opaque = key.background.is_some() || (picture.opaque && (width, height) == (canvas_width, canvas_height));
    file(&payload(&canvas, opaque, picture.format == "JPEG"), fit.cols, fit.rows)
}

fn payload(canvas: &RgbaImage, opaque: bool, lossy: bool) -> Vec<u8> {
    if !opaque {
        return png(canvas, false);
    }
    let jpeg = jpeg(canvas);
    if lossy {
        return jpeg;
    }
    let png = png(canvas, true);
    if png.len() > 2 * jpeg.len() { jpeg } else { png }
}

pub fn file(data: &[u8], cols: u16, rows: u16) -> Vec<u8> {
    let size = data.len();
    let data = STANDARD.encode(data);
    format!("\x1b]1337;File=inline=1;size={size};width={cols};height={rows};doNotMoveCursor=1:{data}\x07").into_bytes()
}

#[cfg(test)]
mod tests {
    use image::Rgba;
    use rstest::rstest;

    use super::super::Protocol;
    use super::super::encode::tests::{CLEAR, RED, cell, image_of, key, noise, photo, picture};
    use super::super::encode::{Fit, encode};
    use super::*;

    struct Sent {
        size: usize,
        cols: u16,
        rows: u16,
        data: Vec<u8>,
    }

    fn sent(out: &[u8]) -> Sent {
        let text = std::str::from_utf8(out).expect("ASCII");
        let body = text.strip_prefix("\x1b]1337;File=").and_then(|body| body.strip_suffix('\x07')).expect("one file");
        let (arguments, data) = body.split_once(':').expect("arguments then data");
        let value = |name: &str| {
            arguments.split(';').find_map(|pair| pair.strip_prefix(name)?.strip_prefix('=')).expect("an argument")
        };
        assert_eq!((value("inline"), value("doNotMoveCursor")), ("1", "1"));
        Sent {
            size: value("size").parse().expect("a size"),
            cols: value("width").parse().expect("cells"),
            rows: value("height").parse().expect("cells"),
            data: STANDARD.decode(data).expect("base64"),
        }
    }

    fn fit(cols: u16, rows: u16, width: u32, height: u32) -> Fit {
        Fit { cols, rows, width, height, cell: cell(8, 16) }
    }

    #[test]
    fn sends_one_inline_file_sized_in_cells() {
        assert_eq!(
            file(b"abc", 3, 2),
            b"\x1b]1337;File=inline=1;size=3;width=3;height=2;doNotMoveCursor=1:YWJj\x07".to_vec()
        );
    }

    #[test]
    fn fills_the_whole_cell_box_with_the_picture_centred_on_the_background() {
        let picture = picture(RgbaImage::from_pixel(30, 10, RED), "PNG");

        let sent = sent(&encode(&picture, &key(Protocol::Iterm, fit(4, 1, 30, 10), Some((1, 2, 3)))));

        let image = image_of(&sent.data);
        assert_eq!((sent.cols, sent.rows, sent.size), (4, 1, sent.data.len()));
        assert_eq!(image.dimensions(), (32, 16));
        assert_eq!(image.get_pixel(0, 0), &Rgba([1, 2, 3, 255]));
        assert_eq!(image.get_pixel(31, 15), &Rgba([1, 2, 3, 255]));
        assert_eq!((image.get_pixel(1, 3), image.get_pixel(30, 12)), (&RED, &RED));
        assert_eq!((image.get_pixel(1, 2), image.get_pixel(31, 3)), (&Rgba([1, 2, 3, 255]), &Rgba([1, 2, 3, 255])));
    }

    #[test]
    fn leaves_the_padding_see_through_when_the_background_is_unknown() {
        let picture = picture(RgbaImage::from_pixel(30, 10, RED), "JPEG");

        let sent = sent(&encode(&picture, &key(Protocol::Iterm, fit(4, 1, 30, 10), None)));

        assert!(sent.data.starts_with(b"\x89PNG"));
        let image = image_of(&sent.data);
        assert_eq!((image.get_pixel(0, 0), image.get_pixel(1, 3)), (&CLEAR, &RED));
    }

    #[test]
    fn lays_see_through_pixels_on_the_background() {
        let picture = picture(RgbaImage::from_pixel(8, 16, Rgba([255, 0, 0, 0])), "PNG");

        let sent = sent(&encode(&picture, &key(Protocol::Iterm, fit(1, 1, 8, 16), Some((10, 20, 30)))));

        assert_eq!(image_of(&sent.data), RgbaImage::from_pixel(8, 16, Rgba([10, 20, 30, 255])));
    }

    #[rstest]
    #[case::photo_stays_a_photo(RgbaImage::from_pixel(64, 64, RED), "JPEG", Some((0, 0, 0)), b"\xff\xd8\xff")]
    #[case::flat_graphic_stays_sharp(RgbaImage::from_pixel(64, 64, RED), "PNG", Some((0, 0, 0)), b"\x89PNG")]
    #[case::photo_in_a_lossless_file_goes_lighter(photo(64, 64), "PNG", Some((0, 0, 0)), b"\xff\xd8\xff")]
    #[case::noise_stays_sharp_when_jpeg_saves_little(noise(64, 64), "PNG", Some((0, 0, 0)), b"\x89PNG")]
    #[case::exact_cells_need_no_padding(noise(64, 64), "JPEG", None, b"\xff\xd8\xff")]
    #[case::see_through_needs_png(RgbaImage::from_pixel(64, 64, Rgba([255, 0, 0, 9])), "WebP", None, b"\x89PNG")]
    fn picks_the_lighter_format_that_keeps_the_picture(
        #[case] image: RgbaImage,
        #[case] format: &'static str,
        #[case] background: Option<(u8, u8, u8)>,
        #[case] magic: &[u8],
    ) {
        let picture = picture(image, format);

        let sent = sent(&encode(&picture, &key(Protocol::Iterm, fit(8, 4, 64, 64), background)));

        assert!(sent.data.starts_with(magic));
    }

    #[test]
    fn sends_fewer_pixels_in_the_same_cells_when_shrunk() {
        let picture = picture(RgbaImage::from_pixel(64, 64, RED), "PNG");

        let sent = sent(&image(&picture, &key(Protocol::Iterm, fit(8, 4, 64, 64), Some((0, 0, 0))), 0.5));

        assert_eq!((sent.cols, sent.rows), (8, 4));
        assert_eq!(image_of(&sent.data).dimensions(), (32, 32));
    }
}
