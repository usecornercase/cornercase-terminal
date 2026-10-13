use icy_sixel::{EncodeOptions, QuantizeMethod, SixelImage};
use image::RgbaImage;

use super::Picture;
use super::encode::{Key, flatten, round, sized};

pub(crate) const COLOURS: u16 = 256;
pub(crate) const FEWEST: u16 = 64;
pub(crate) const DIFFUSION: f32 = 0.875;
pub(crate) const BAND: u32 = 6;

pub(crate) fn image(picture: &Picture, key: &Key, colours: u16, scale: f64) -> Vec<u8> {
    let fit = key.fit;
    let side = |pixels: u32| round(f64::from(pixels) * scale).max(1);
    let (width, height) = bands(side(fit.width), side(fit.height));
    let mut image = sized(picture, width, height).into_owned();
    if let Some(background) = key.background {
        flatten(&mut image, background);
    }
    encode(image, colours)
}

pub(crate) fn bands(width: u32, height: u32) -> (u32, u32) {
    let whole = height - height % BAND;
    if whole == 0 || whole == height {
        return (width, height);
    }
    (round(f64::from(width) * f64::from(whole) / f64::from(height)).max(1), whole)
}

pub(crate) fn colours(registers: u16) -> u16 {
    if registers == 0 { COLOURS } else { registers.clamp(2, COLOURS) }
}

pub(crate) fn encode(image: RgbaImage, colours: u16) -> Vec<u8> {
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
mod tests;
