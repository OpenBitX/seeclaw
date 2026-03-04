//! VisualDecisionLayer trait — interface for each layer of the visual decision pipeline.
//!
//! Mirrors the `RouterLayer` trait design: each layer returns `Some(result)` if it
//! can make a confident decision, or `None` to fall through to the next layer.

use async_trait::async_trait;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::state::TodoStep;

/// Result from a visual decision layer.
#[derive(Debug, Clone)]
pub struct VisualDecisionResult {
    /// Whether a screenshot should be captured and passed to the vision model for summarization.
    pub needs_visual: bool,
    /// Confidence score (0.0 – 1.0).
    pub confidence: f32,
}

/// A single classification layer in the visual decision pipeline.
///
/// Returns `Some(VisualDecisionResult)` when the layer is confident, or `None`
/// to delegate to the next layer.
#[async_trait]
pub trait VisualDecisionLayer: Send + Sync {
    /// Human-readable name of this layer.
    fn name(&self) -> &str;

    /// Attempt to decide whether visual context is needed.
    async fn classify(
        &self,
        goal: &str,
        steps_log: &[String],
        todo_steps: &[TodoStep],
        ctx: &NodeContext,
    ) -> Option<VisualDecisionResult>;
}
