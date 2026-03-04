//! Visual decision pipeline — 3-layer classifier that decides whether the
//! SummarizerNode should capture a screenshot and use the vision model.
//!
//! Layers run top-to-bottom; the first to return `Some(result)` wins.
//! Mirrors the `RouterPipeline` design for consistency.

pub mod bayesian_layer;
pub mod layer;
pub mod llm_layer;
pub mod regex_layer;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::nodes::visual_router::bayesian_layer::VisualBayesianLayer;
use crate::agent_engine::nodes::visual_router::layer::{VisualDecisionLayer, VisualDecisionResult};
use crate::agent_engine::nodes::visual_router::llm_layer::VisualLlmLayer;
use crate::agent_engine::nodes::visual_router::regex_layer::VisualRegexLayer;
use crate::agent_engine::state::TodoStep;

// ── Layer enable/disable flags ─────────────────────────────────────────────

/// L1: Regex keyword matching.
const ENABLE_REGEX_LAYER: bool = true;

/// L2: Keyword-weighted Bayesian scoring.
const ENABLE_BAYESIAN_LAYER: bool = true;

/// L3: LLM fallback classification.
const ENABLE_LLM_LAYER: bool = true;

// ── VisualDecisionPipeline ─────────────────────────────────────────────────

/// Orchestrates the 3-layer visual-context decision pipeline.
pub struct VisualDecisionPipeline {
    layers: Vec<Box<dyn VisualDecisionLayer>>,
}

impl VisualDecisionPipeline {
    /// Build the pipeline from the enabled layers.
    pub fn new() -> Self {
        let mut layers: Vec<Box<dyn VisualDecisionLayer>> = Vec::new();

        if ENABLE_REGEX_LAYER {
            layers.push(Box::new(VisualRegexLayer::new()));
        }
        if ENABLE_BAYESIAN_LAYER {
            layers.push(Box::new(VisualBayesianLayer::new()));
        }
        if ENABLE_LLM_LAYER {
            layers.push(Box::new(VisualLlmLayer::new()));
        }

        tracing::debug!(
            layers = layers.iter().map(|l| l.name()).collect::<Vec<_>>().join(", "),
            "visual_router pipeline initialised"
        );

        Self { layers }
    }

    /// Run the pipeline and return whether visual context is needed.
    ///
    /// Falls back to `needs_visual = false` if all layers abstain (should never
    /// happen in practice since the LLM layer always returns `Some`).
    pub async fn classify(
        &self,
        goal: &str,
        steps_log: &[String],
        todo_steps: &[TodoStep],
        ctx: &NodeContext,
    ) -> VisualDecisionResult {
        for layer in &self.layers {
            tracing::debug!(layer = layer.name(), "trying visual_router layer");
            if let Some(result) = layer.classify(goal, steps_log, todo_steps, ctx).await {
                tracing::info!(
                    layer = layer.name(),
                    needs_visual = result.needs_visual,
                    confidence = result.confidence,
                    "visual decision made"
                );
                return result;
            }
        }

        // All layers abstained — default to false (safe: just means no screenshot)
        tracing::info!("all visual_router layers abstained — defaulting to needs_visual=false");
        VisualDecisionResult { needs_visual: false, confidence: 0.0 }
    }
}
