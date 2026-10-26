use std::borrow::Cow;
use std::io::{Cursor, ErrorKind};

use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageError, ImageFormat, ImageReader, Limits, RgbaImage};

use super::Picture;
use super::encode::{resize, round};

#[cfg(test)]
pub(crate) const HEAD: usize = 32;
pub(crate) const MAX_SIDE: u32 = 2048;
pub(crate) const LIMIT_SIDE: u32 = 16_384;
pub(crate) const LIMIT_BYTES: u64 = 256 * 1024 * 1024;

pub(crate) fn sniff(head: &[u8]) -> Option<&'static str> {
    kind(head).map(|(name, _)| name)
}

pub(crate) fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let (_, format) = kind(bytes)?;
    ImageReader::with_format(Cursor::new(bytes), format).into_dimensions().ok()
}

pub(crate) fn decode(bytes: &[u8], id: u64) -> Result<Picture, String> {
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
mod tests;
