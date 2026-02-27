use base64::Engine as _;
use xcap::Monitor;

use crate::errors::{SeeClawError, SeeClawResult};
use crate::perception::types::ScreenshotMeta;

pub struct ScreenshotResult {
    pub image_bytes: Vec<u8>,
    pub image_base64: String,
    pub meta: ScreenshotMeta,
}

/// Captures the primary monitor and returns PNG bytes + metadata.
/// Runs the sync xcap call on a blocking thread pool so as not to block the async runtime.
pub async fn capture_primary() -> SeeClawResult<ScreenshotResult> {
    tokio::task::spawn_blocking(capture_sync)
        .await
        .map_err(|e| SeeClawError::Perception(e.to_string()))?
}

fn capture_sync() -> SeeClawResult<ScreenshotResult> {
    let monitors =
        Monitor::all().map_err(|e| SeeClawError::Perception(format!("Monitor::all: {e}")))?;

    let primary = monitors
        .into_iter()
        .find(|m| m.is_primary())
        .ok_or_else(|| SeeClawError::Perception("no primary monitor found".into()))?;

    let img = primary
        .capture_image()
        .map_err(|e| SeeClawError::Perception(format!("capture_image: {e}")))?;

    let phys_w = img.width();
    let phys_h = img.height();

    let meta = ScreenshotMeta {
        monitor_index: 0,
        scale_factor: primary.scale_factor() as f64,
        physical_width: phys_w,
        physical_height: phys_h,
        logical_width: primary.width(),
        logical_height: primary.height(),
    };

    // Convert xcap RgbaImage to image::DynamicImage and encode as PNG
    let raw: Vec<u8> = img.into_raw();
    let rgba_img = image::RgbaImage::from_raw(phys_w, phys_h, raw)
        .ok_or_else(|| SeeClawError::Perception("image::from_raw failed".into()))?;

    let mut png_bytes = Vec::new();
    image::DynamicImage::ImageRgba8(rgba_img)
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .map_err(|e| SeeClawError::Perception(format!("PNG encode: {e}")))?;

    let image_base64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

    Ok(ScreenshotResult {
        image_bytes: png_bytes,
        image_base64,
        meta,
    })
}
