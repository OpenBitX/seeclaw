//! Router pipeline — 3-layer classification system.
//!
//! Layers are executed top-to-bottom. The first layer to return `Some(RouteResult)`
//! wins. Each layer can be independently enabled/disabled via const flags.

pub mod bayesian_layer;
pub mod layer;
pub mod llm_layer;
pub mod regex_layer;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::router::bayesian_layer::BayesianLayer;
use crate::agent_engine::router::layer::{RouteResult, RouterLayer};
use crate::agent_engine::router::llm_layer::LlmLayer;
use crate::agent_engine::router::regex_layer::RegexLayer;
use crate::agent_engine::state::RouteType;

// ── Layer enable/disable flags ─────────────────────────────────────────────
// These are compile-time constants. Users cannot toggle them from the UI.
// Flip these to enable/disable individual router layers.

/// L1: Regex keyword matching.
const ENABLE_REGEX_LAYER: bool = true;

/// L2: Bayesian probability classifier (disabled until model is trained).
const ENABLE_BAYESIAN_LAYER: bool = false;

/// L3: LLM fallback classification.
const ENABLE_LLM_LAYER: bool = true;

// ── RouterPipeline ─────────────────────────────────────────────────────────

/// Orchestrates the 3-layer routing pipeline.
pub struct RouterPipeline {
    layers: Vec<Box<dyn RouterLayer>>,
}

impl RouterPipeline {
    /// Build the pipeline from the enabled layers.
    pub fn new() -> Self {
        let mut layers: Vec<Box<dyn RouterLayer>> = Vec::new();

        if ENABLE_REGEX_LAYER {
            layers.push(Box::new(RegexLayer::new()));
        }
        if ENABLE_BAYESIAN_LAYER {
            layers.push(Box::new(BayesianLayer::new()));
        }
        if ENABLE_LLM_LAYER {
            layers.push(Box::new(LlmLayer::new()));
        }

        tracing::info!(
            layers = layers.iter().map(|l| l.name()).collect::<Vec<_>>().join(", "),
            "router pipeline initialised"
        );

        Self { layers }
    }

    /// Classify a query by running it through all enabled layers.
    ///
    /// Returns the first successful classification, or a default Complex route
    /// if no layer could classify the query.
    pub async fn classify(&self, query: &str, ctx: &NodeContext) -> RouteResult {
        for layer in &self.layers {
            tracing::debug!(layer = layer.name(), "trying router layer");
            if let Some(result) = layer.classify(query, ctx).await {
                tracing::info!(
                    layer = layer.name(),
                    route = ?result.route_type,
                    confidence = result.confidence,
                    "route classified"
                );
                return result;
            }
        }

        // No layer could classify — default to Complex
        tracing::info!("all router layers returned None — defaulting to Complex");
        RouteResult {
            route_type: RouteType::Complex,
            confidence: 0.0,
            tool_calls: None,
        }
    }
}
