/// Focus-crop: cut and upscale a region around a detected element
/// so the VLM can examine details at higher resolution.
///
/// This is an **optional** second-pass — adds one extra VLM call per step
/// but significantly improves click accuracy for small or dense UI elements.
use crate::errors::{SeeClawError, SeeClawResult};
use crate::perception::types::UIElement;

/// Result of a focus crop operation.
pub struct FocusCrop {
    /// PNG bytes of the cropped region.
    pub image_bytes: Vec<u8>,
    /// Base64-encoded PNG.
    pub image_base64: String,
    /// The pixel offset of the crop's top-left corner in the original image.
    pub origin_x: u32,
    pub origin_y: u32,
    /// Size of the crop in the original image (before upscaling).
    pub crop_w: u32,
    pub crop_h: u32,
}

/// Crop the area around `element` from the source image, with `padding_px`
/// pixels of context on each side, and upscale the crop to at least `min_size`.
///
/// `src_bytes`: original screenshot (JPEG/PNG).
/// `element`: the UI element whose surroundings we want to examine.
/// `padding_px`: extra pixels around the bounding box (default 80).
/// `min_size`: minimum width/height after upscaling (default 512).
pub fn crop_element(
    src_bytes: &[u8],
    element: &UIElement,
    padding_px: u32,
    min_size: u32,
) -> SeeClawResult<FocusCrop> {
    let img = image::load_from_memory(src_bytes)
        .map_err(|e| SeeClawError::Perception(format!("crop load: {e}")))?;
    let (w, h) = (img.width(), img.height());

    // Convert normalised bbox to pixel coordinates
    let [x1n, y1n, x2n, y2n] = element.bbox;
    let ex1 = (x1n * w as f32).round() as i32;
    let ey1 = (y1n * h as f32).round() as i32;
    let ex2 = (x2n * w as f32).round() as i32;
    let ey2 = (y2n * h as f32).round() as i32;

    // Padded region, clamped to image bounds
    let pad = padding_px as i32;
    let cx1 = (ex1 - pad).max(0) as u32;
    let cy1 = (ey1 - pad).max(0) as u32;
    let cx2 = (ex2 + pad).min(w as i32) as u32;
    let cy2 = (ey2 + pad).min(h as i32) as u32;
    let cw = cx2 - cx1;
    let ch = cy2 - cy1;

    if cw == 0 || ch == 0 {
        return Err(SeeClawError::Perception("zero-size crop".into()));
    }

    let cropped = img.crop_imm(cx1, cy1, cw, ch);

    // Upscale if smaller than min_size
    let scale = if cw < min_size || ch < min_size {
        let sw = min_size as f32 / cw as f32;
        let sh = min_size as f32 / ch as f32;
        sw.max(sh).max(1.0)
    } else {
        1.0
    };
    let out_w = (cw as f32 * scale).round() as u32;
    let out_h = (ch as f32 * scale).round() as u32;

    let result_img = if scale > 1.0 {
        cropped.resize_exact(out_w, out_h, image::imageops::FilterType::Lanczos3)
    } else {
        cropped
    };

    // Encode as PNG
    let mut png_bytes = Vec::new();
    result_img
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .map_err(|e| SeeClawError::Perception(format!("crop PNG encode: {e}")))?;

    let b64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &png_bytes,
    );

    Ok(FocusCrop {
        image_bytes: png_bytes,
        image_base64: b64,
        origin_x: cx1,
        origin_y: cy1,
        crop_w: cw,
        crop_h: ch,
    })
}

/// Given pixel coordinates *within the cropped image*, convert back to
/// physical coordinates in the full screenshot.
pub fn crop_to_physical(
    crop_x: f32,
    crop_y: f32,
    focus: &FocusCrop,
    upscaled_w: u32,
    upscaled_h: u32,
) -> (i32, i32) {
    let sx = focus.crop_w as f32 / upscaled_w as f32;
    let sy = focus.crop_h as f32 / upscaled_h as f32;
    let orig_x = (crop_x * sx + focus.origin_x as f32).round() as i32;
    let orig_y = (crop_y * sy + focus.origin_y as f32).round() as i32;
    (orig_x, orig_y)
}
