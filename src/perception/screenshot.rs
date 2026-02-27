// Screenshot capture — full implementation in Phase 4.
use crate::errors::{SeeClawError, SeeClawResult};
use crate::perception::types::ScreenshotMeta;

pub struct ScreenshotResult {
    pub image_bytes: Vec<u8>,
    pub image_base64: String,
    pub meta: ScreenshotMeta,
}

/// Captures the primary monitor screenshot. Full implementation in Phase 4.
pub async fn capture_primary() -> SeeClawResult<ScreenshotResult> {
    Err(SeeClawError::Perception("Screenshot not implemented yet (Phase 4)".to_string()))
}
