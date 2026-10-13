use base64::{Engine as _, engine::general_purpose::STANDARD};
use image::{Rgba, RgbaImage, imageops};

use super::Picture;
use super::encode::{Key, flatten, jpeg, png, round, sized};

pub(crate) fn image(picture: &Picture, key: &Key, scale: f64) -> Vec<u8> {
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

pub(crate) fn file(data: &[u8], cols: u16, rows: u16) -> Vec<u8> {
    let size = data.len();
    let data = STANDARD.encode(data);
    format!("\x1b]1337;File=inline=1;size={size};width={cols};height={rows};doNotMoveCursor=1:{data}\x07").into_bytes()
}

#[cfg(test)]
mod tests;
