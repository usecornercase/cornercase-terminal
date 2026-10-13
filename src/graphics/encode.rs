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
pub mod tests;
