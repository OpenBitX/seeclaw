use async_trait::async_trait;

use crate::errors::SeeClawResult;
use crate::perception::types::{PerceptionContext, ScreenshotMeta};

/// Strategy trait for UI element detection.
/// Three implementations: ONNX/YOLO, OS Accessibility tree, SoM Grid fallback.
#[async_trait]
pub trait VisionParser: Send + Sync {
    async fn parse(
        &self,
        image_bytes: &[u8],
        meta: &ScreenshotMeta,
    ) -> SeeClawResult<PerceptionContext>;
}
