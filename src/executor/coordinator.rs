// DPI-aware coordinate mapping — full implementation in Phase 5.
use crate::perception::types::{ScreenshotMeta, UIElement};

/// Converts a normalized bbox center to physical screen pixel coordinates.
/// Handles DPI scaling and multi-monitor offsets.
pub fn normalized_to_physical(element: &UIElement, meta: &ScreenshotMeta) -> (i32, i32) {
    let center_x = (element.bbox[0] + element.bbox[2]) / 2.0;
    let center_y = (element.bbox[1] + element.bbox[3]) / 2.0;

    let physical_x = (center_x * meta.physical_width as f32) as i32;
    let physical_y = (center_y * meta.physical_height as f32) as i32;

    (physical_x, physical_y)
}
