// Perception pipeline — full implementation in Phase 4.
use crate::errors::{SeeClawError, SeeClawResult};
use crate::perception::types::PerceptionContext;

/// Runs the full perception pipeline:
/// 1. Capture screenshot
/// 2. Try ONNX YOLO detection
/// 3. If zero elements detected, fall back to SoM Grid
pub async fn run() -> SeeClawResult<PerceptionContext> {
    Err(SeeClawError::Perception("Perception pipeline not implemented yet (Phase 4)".to_string()))
}
