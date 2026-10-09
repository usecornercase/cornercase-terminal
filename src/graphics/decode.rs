use std::borrow::Cow;
use std::io::{Cursor, ErrorKind};

use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageError, ImageFormat, ImageReader, Limits, RgbaImage};

use super::Picture;
use super::encode::{resize, round};

pub const HEAD: usize = 32;
pub const MAX_SIDE: u32 = 2048;
pub const LIMIT_SIDE: u32 = 16_384;
pub const LIMIT_BYTES: u64 = 256 * 1024 * 1024;

pub fn sniff(head: &[u8]) -> Option<&'static str> {
    kind(head).map(|(name, _)| name)
}

pub fn decode(bytes: &[u8], id: u64) -> Result<Picture, String> {
    let (format, kind) = kind(bytes).ok_or("not an image")?;
    let damaged = |error| reason(&error, format);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), kind);
    let mut limits = Limits::default();
    limits.max_alloc = Some(LIMIT_BYTES);
    reader.limits(limits.clone());
    let mut decoder = reader.into_decoder().map_err(damaged)?;
    let (width, height) = decoder.dimensions();
    let too_large = || format!("too large to preview ({width}×{height})");
    if width > LIMIT_SIDE || height > LIMIT_SIDE || limits.reserve(decoder.total_bytes()).is_err() {
        return Err(too_large());
    }
    limits.max_image_width = Some(LIMIT_SIDE);
    limits.max_image_height = Some(LIMIT_SIDE);
    decoder.set_limits(limits).map_err(damaged)?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let image = DynamicImage::from_decoder(decoder).map_err(|error| match error {
        ImageError::Limits(_) => too_large(),
        error => reason(&error, format),
    })?;
    let mut image = DynamicImage::ImageRgba8(shrink(image));
    image.apply_orientation(orientation);
    let rgba = image.into_rgba8();
    let (width, height) = if turns(orientation) { (height, width) } else { (width, height) };
    let opaque = rgba.pixels().all(|pixel| pixel[3] == u8::MAX);
    Ok(Picture { id, format, width, height, bytes: bytes.len() as u64, rgba, opaque })
}

fn kind(head: &[u8]) -> Option<(&'static str, ImageFormat)> {
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(("PNG", ImageFormat::Png))
    } else if head.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(("JPEG", ImageFormat::Jpeg))
    } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        Some(("GIF", ImageFormat::Gif))
    } else if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
        Some(("WebP", ImageFormat::WebP))
    } else if bmp(head) {
        Some(("BMP", ImageFormat::Bmp))
    } else if ico(head) {
        Some(("ICO", ImageFormat::Ico))
    } else {
        None
    }
}

fn bmp(head: &[u8]) -> bool {
    let header = head.get(14..18).map(|size| u32::from_le_bytes([size[0], size[1], size[2], size[3]]));
    head.starts_with(b"BM") && matches!(header, Some(12 | 16 | 40 | 52 | 56 | 64 | 108 | 124))
}

fn ico(head: &[u8]) -> bool {
    let count = head.get(4..6).map_or(0, |count| u16::from_le_bytes([count[0], count[1]]));
    head.starts_with(&[0, 0, 1, 0])
        && count > 0
        && head.get(9) == Some(&0)
        && matches!(head.get(10..12), Some([0 | 1, 0]))
}

fn shrink(image: DynamicImage) -> RgbaImage {
    let (width, height) = (image.width(), image.height());
    if width.max(height) <= MAX_SIDE {
        return image.into_rgba8();
    }
    let scale = f64::from(MAX_SIDE) / f64::from(width.max(height));
    let side = |length: u32| round(f64::from(length) * scale).clamp(1, MAX_SIDE);
    let (to_width, to_height) = (side(width), side(height));
    match image {
        DynamicImage::ImageRgba8(_) | DynamicImage::ImageRgba16(_) => {
            resize(Cow::Owned(image.into_rgba8()), to_width, to_height)
        }
        image => image.thumbnail_exact(to_width, to_height).into_rgba8(),
    }
}

fn turns(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90 | Orientation::Rotate270 | Orientation::Rotate90FlipH | Orientation::Rotate270FlipH
    )
}

fn reason(error: &ImageError, format: &str) -> String {
    match error {
        ImageError::Limits(_) => "too large to preview".to_string(),
        ImageError::Unsupported(_) => format!("this kind of {format} is not supported"),
        ImageError::IoError(error) if error.kind() == ErrorKind::UnexpectedEof => "the file ends too early".to_string(),
        _ => format!("the {format} data is damaged"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::codecs::gif::GifEncoder;
    use image::{Frame, Rgba};
    use rstest::rstest;

    use super::*;

    const RED: Rgba<u8> = Rgba([255, 0, 0, 255]);
    const GREEN: Rgba<u8> = Rgba([0, 255, 0, 255]);
    const BLUE: Rgba<u8> = Rgba([0, 0, 255, 255]);
    const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);

    fn pattern() -> RgbaImage {
        RgbaImage::from_fn(4, 2, |x, y| [[RED, GREEN, BLUE, WHITE], [WHITE, BLUE, GREEN, RED]][y as usize][x as usize])
    }

    fn halves(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, _| if x < width / 2 { RED } else { BLUE })
    }

    fn file(image: &RgbaImage, format: ImageFormat) -> Vec<u8> {
        let image = DynamicImage::ImageRgba8(image.clone());
        let image = if format == ImageFormat::Jpeg { DynamicImage::ImageRgb8(image.to_rgb8()) } else { image };
        let mut out = Cursor::new(Vec::new());
        image.write_to(&mut out, format).expect("encodes the sample");
        out.into_inner()
    }

    fn close(pixel: Rgba<u8>, expected: Rgba<u8>) -> bool {
        pixel.0.iter().zip(expected.0).all(|(&got, want)| got.abs_diff(want) <= 40)
    }

    fn crc(bytes: &[u8]) -> u32 {
        !bytes.iter().fold(u32::MAX, |crc, &byte| {
            (0..8).fold(crc ^ u32::from(byte), |crc, _| if crc & 1 == 1 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 })
        })
    }

    fn png_claiming(width: u32, height: u32) -> Vec<u8> {
        let chunk = |kind: &[u8], data: &[u8]| {
            let body = [kind, data].concat();
            let length = u32::try_from(data.len()).expect("a small chunk").to_be_bytes();
            [&length[..], &body, &crc(&body).to_be_bytes()].concat()
        };
        let header = [&width.to_be_bytes()[..], &height.to_be_bytes(), &[8, 6, 0, 0, 0]].concat();
        let data = [0x78, 0x01, 0x01, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01];
        [&b"\x89PNG\r\n\x1a\n"[..], &chunk(b"IHDR", &header), &chunk(b"IDAT", &data), &chunk(b"IEND", b"")].concat()
    }

    fn with_exif(jpeg: &[u8], orientation: u8) -> Vec<u8> {
        let tiff = [
            &b"MM\0*\0\0\0\x08\0\x01"[..],
            &[0x01, 0x12, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, orientation, 0x00, 0x00],
            &[0, 0, 0, 0],
        ]
        .concat();
        let length = u16::try_from(2 + 6 + tiff.len()).expect("a small segment").to_be_bytes();
        [&jpeg[..2], &[0xff, 0xe1], &length, b"Exif\0\0", &tiff, &jpeg[2..]].concat()
    }

    mod sniff {
        use super::*;

        #[rstest]
        #[case::png(ImageFormat::Png, "PNG")]
        #[case::jpeg(ImageFormat::Jpeg, "JPEG")]
        #[case::gif(ImageFormat::Gif, "GIF")]
        #[case::webp(ImageFormat::WebP, "WebP")]
        #[case::bmp(ImageFormat::Bmp, "BMP")]
        #[case::ico(ImageFormat::Ico, "ICO")]
        fn names_an_image_by_its_first_bytes(#[case] format: ImageFormat, #[case] name: &str) {
            assert_eq!(sniff(&file(&pattern(), format)[..HEAD]), Some(name));
        }

        #[rstest]
        #[case::text(b"fn main() {}\n")]
        #[case::text_starting_like_a_bitmap(b"BMW and Audi are car brands, not bitmaps\n")]
        #[case::bitmap_magic_alone(b"BM")]
        #[case::svg(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>")]
        #[case::tiff(b"II*\0\x08\0\0\0\0\0\0\0")]
        #[case::wave(b"RIFF\x24\0\0\0WAVEfmt ")]
        #[case::icon_directory_without_icons(b"\0\0\x01\0\0\0\x10\x10\0\0\x01\0\x20\0")]
        #[case::cut_short_png(b"\x89PNG")]
        #[case::empty(b"")]
        fn leaves_other_files_alone(#[case] head: &[u8]) {
            assert_eq!(sniff(head), None);
        }

        #[test]
        fn ignores_the_file_name() {
            assert_eq!(sniff(&file(&pattern(), ImageFormat::Png)), Some("PNG"));
            assert_eq!(kind(b"GIF87a\x01\0\x01\0").map(|(name, _)| name), Some("GIF"));
        }
    }

    mod formats {
        use super::*;

        #[rstest]
        #[case::png(ImageFormat::Png)]
        #[case::gif(ImageFormat::Gif)]
        #[case::webp(ImageFormat::WebP)]
        #[case::bmp(ImageFormat::Bmp)]
        #[case::ico(ImageFormat::Ico)]
        fn keeps_every_pixel_of_a_lossless_file(#[case] format: ImageFormat) {
            let picture = decode(&file(&pattern(), format), 1).expect("decodes");

            assert_eq!(picture.rgba, pattern());
        }

        #[test]
        fn reads_a_jpeg_close_to_its_colours() {
            let picture = decode(&file(&halves(16, 8), ImageFormat::Jpeg), 1).expect("decodes");

            assert!(close(*picture.rgba.get_pixel(0, 0), RED));
            assert!(close(*picture.rgba.get_pixel(15, 7), BLUE));
        }

        #[test]
        fn describes_the_file_for_the_header() {
            let bytes = file(&pattern(), ImageFormat::Png);

            let picture = decode(&bytes, 42).expect("decodes");

            assert_eq!(
                (picture.id, picture.format, picture.width, picture.height, picture.bytes),
                (42, "PNG", 4, 2, bytes.len() as u64)
            );
        }

        #[rstest]
        #[case::every_pixel_solid(255, true)]
        #[case::one_pixel_see_through(254, false)]
        #[case::one_pixel_invisible(0, false)]
        fn tells_whether_anything_shows_through(#[case] alpha: u8, #[case] opaque: bool) {
            let mut image = pattern();
            image.put_pixel(3, 1, Rgba([10, 20, 30, alpha]));

            assert_eq!(decode(&file(&image, ImageFormat::Png), 1).expect("decodes").opaque, opaque);
        }

        #[test]
        fn shows_the_first_frame_of_an_animation() {
            let mut bytes = Vec::new();
            let frames = [halves(4, 4), RgbaImage::from_pixel(4, 4, GREEN)].map(Frame::new);
            GifEncoder::new(&mut bytes).encode_frames(frames).expect("encodes the animation");

            let picture = decode(&bytes, 1).expect("decodes");

            assert_eq!(picture.rgba, halves(4, 4));
        }
    }

    mod orientation {
        use super::*;

        #[rstest]
        #[case::upright(1, (16, 8), [RED, BLUE])]
        #[case::upside_down(3, (16, 8), [BLUE, RED])]
        #[case::turned_clockwise(6, (8, 16), [RED, BLUE])]
        #[case::turned_anticlockwise(8, (8, 16), [BLUE, RED])]
        #[case::mirrored(2, (16, 8), [BLUE, RED])]
        fn turns_a_photo_the_way_its_camera_held_it(
            #[case] exif: u8,
            #[case] size: (u32, u32),
            #[case] corners: [Rgba<u8>; 2],
        ) {
            let picture = decode(&with_exif(&file(&halves(16, 8), ImageFormat::Jpeg), exif), 1).expect("decodes");

            assert_eq!((picture.width, picture.height), size);
            assert_eq!(picture.rgba.dimensions(), size);
            assert!(close(*picture.rgba.get_pixel(0, 0), corners[0]));
            assert!(close(*picture.rgba.get_pixel(size.0 - 1, size.1 - 1), corners[1]));
        }
    }

    mod size {
        use super::*;

        #[test]
        fn shrinks_a_large_picture_and_keeps_its_real_size_for_the_header() {
            let picture = decode(&file(&halves(3000, 12), ImageFormat::Png), 1).expect("decodes");

            assert_eq!((picture.width, picture.height), (3000, 12));
            assert_eq!(picture.rgba.dimensions(), (2048, 8));
            assert_eq!(picture.rgba.get_pixel(0, 0), &RED);
            assert_eq!(picture.rgba.get_pixel(2047, 7), &BLUE);
        }

        #[test]
        fn keeps_the_colour_of_see_through_pixels_when_shrinking() {
            let image = RgbaImage::from_fn(4096, 2, |x, _| if x % 2 == 0 { WHITE } else { Rgba([0, 0, 0, 0]) });

            let picture = decode(&file(&image, ImageFormat::Png), 1).expect("decodes");

            assert_eq!(picture.rgba.dimensions(), (2048, 1));
            assert!(picture.rgba.pixels().all(|pixel| pixel.0 == [255, 255, 255, 128]));
        }

        #[test]
        fn leaves_a_small_picture_as_it_is() {
            assert_eq!(decode(&file(&halves(2048, 4), ImageFormat::Png), 1).expect("decodes").rgba, halves(2048, 4));
        }
    }

    mod refusals {
        use super::*;

        fn patched(mut bytes: Vec<u8>, at: usize, with: &[u8]) -> Vec<u8> {
            bytes[at..at + with.len()].copy_from_slice(with);
            bytes
        }

        fn start_of_frame(jpeg: &[u8]) -> usize {
            jpeg.windows(2).position(|pair| pair == [0xff, 0xc0]).expect("a baseline frame")
        }

        #[rstest]
        #[case::wider_than_any_screen(png_claiming(50_000, 50_000), "too large to preview (50000×50000)")]
        #[case::taller_than_the_limit(png_claiming(1, 16_385), "too large to preview (1×16385)")]
        #[case::more_memory_than_allowed(png_claiming(9000, 9000), "too large to preview (9000×9000)")]
        #[case::jpeg_claiming_a_huge_frame(
            {
                let jpeg = file(&halves(16, 8), ImageFormat::Jpeg);
                let at = start_of_frame(&jpeg) + 5;
                patched(jpeg, at, &[0xea, 0x60, 0xea, 0x60])
            },
            "too large to preview (60000×60000)"
        )]
        #[case::gif_claiming_a_huge_screen(
            patched(file(&halves(4, 4), ImageFormat::Gif), 6, &[0xff, 0xff, 0xff, 0xff]),
            "too large to preview (65535×65535)"
        )]
        #[case::webp_claiming_the_largest_size(
            patched(file(&halves(4, 4), ImageFormat::WebP), 21, &(0x3ffe_u32 | 0x3ffe << 14).to_le_bytes()),
            "too large to preview (16383×16383)"
        )]
        fn refuses_a_small_file_that_claims_a_huge_picture(#[case] bytes: Vec<u8>, #[case] reason: &str) {
            assert!(bytes.len() < 1024);

            assert_eq!(decode(&bytes, 1), Err(reason.to_string()));
        }

        #[rstest]
        #[case::wider_than_the_limit(20_000, "too large to preview (20000×20000)")]
        #[case::more_memory_than_allowed(12_000, "too large to preview (12000×12000)")]
        #[case::beyond_what_bitmaps_hold(100_000, "the BMP data is damaged")]
        fn refuses_a_bitmap_claiming_a_huge_picture(#[case] side: u32, #[case] reason: &str) {
            let side = side.to_le_bytes();
            let bytes = patched(file(&halves(4, 4), ImageFormat::Bmp), 18, &[side, side].concat());

            assert_eq!(decode(&bytes, 1), Err(reason.to_string()));
        }

        #[test]
        fn refuses_an_icon_holding_a_huge_picture() {
            let png = png_claiming(50_000, 50_000);
            let size = u32::try_from(png.len()).expect("small").to_le_bytes();
            let directory = [&[0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0, 32, 0][..], &size, &22u32.to_le_bytes()].concat();

            assert_eq!(decode(&[directory, png].concat(), 1), Err("too large to preview (50000×50000)".to_string()));
        }

        #[rstest]
        #[case::png_cut_short(ImageFormat::Png, 60, "the file ends too early")]
        #[case::gif_cut_short(ImageFormat::Gif, 30, "the GIF data is damaged")]
        #[case::webp_cut_short(ImageFormat::WebP, 26, "the WebP data is damaged")]
        fn says_why_a_damaged_file_cannot_be_read(
            #[case] format: ImageFormat,
            #[case] keep: usize,
            #[case] reason: &str,
        ) {
            let bytes = file(&halves(64, 64), format);

            assert_eq!(decode(&bytes[..keep], 1), Err(reason.to_string()));
        }

        #[test]
        fn refuses_what_is_not_an_image() {
            assert_eq!(decode(b"plain text", 1), Err("not an image".to_string()));
        }
    }
}
