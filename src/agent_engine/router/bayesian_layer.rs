//! L2: Bayesian probability classifier layer (skeleton).
//!
//! Loads a pre-trained Bayesian model from `models/router_bayesian.bin`.
//! The model file is expected to be trained offline — this layer provides
//! the scaffolding for loading and inference.
//!
//! **Current status**: skeleton only. Returns `None` (pass-through) if the
//! model file is not found. Inference is `todo!()` until the model is trained.

use async_trait::async_trait;
use std::path::Path;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::router::layer::{RouteResult, RouterLayer};
use crate::agent_engine::state::RouteType;

/// Confidence threshold — below this, the Bayesian layer defers to L3.
const BAYESIAN_THRESHOLD: f32 = 0.75;

/// Bayesian classifier for query routing.
pub struct BayesianLayer {
    /// Whether the model was successfully loaded.
    model_loaded: bool,
    // TODO: Add actual model fields once training pipeline is ready.
    // e.g. class_priors, feature_log_probs, vocabulary, etc.
}

impl BayesianLayer {
    /// Attempt to load the model from disk.
    pub fn new() -> Self {
        let model_path = Path::new("models/router_bayesian.bin");
        if model_path.exists() {
            tracing::info!(path = %model_path.display(), "Bayesian model file found");
            // TODO: Deserialize model parameters from the binary file.
            Self { model_loaded: true }
        } else {
            tracing::warn!(
                path = %model_path.display(),
                "Bayesian model file not found — layer will pass through"
            );
            Self { model_loaded: false }
        }
    }

    /// Run inference on the query text.
    /// Returns (route_type, confidence) or None if model not loaded.
    fn predict(&self, _query: &str) -> Option<(RouteType, f32)> {
        if !self.model_loaded {
            return None;
        }
        // TODO: Implement actual Naive Bayes inference here.
        // 1. Tokenize query
        // 2. Compute log-probabilities for each class
        // 3. Return argmax class with calibrated confidence
        //
        // For now, return None to pass through to L3.
        None
    }
}

#[async_trait]
impl RouterLayer for BayesianLayer {
    fn name(&self) -> &str {
        "bayesian"
    }

    async fn classify(&self, query: &str, _ctx: &NodeContext) -> Option<RouteResult> {
        let (route_type, confidence) = self.predict(query)?;

        if confidence < BAYESIAN_THRESHOLD {
            tracing::debug!(
                layer = "bayesian",
                confidence,
                threshold = BAYESIAN_THRESHOLD,
                "confidence below threshold — deferring to next layer"
            );
            return None;
        }

        tracing::debug!(
            layer = "bayesian",
            route = ?route_type,
            confidence,
            "classification accepted"
        );
        Some(RouteResult {
            route_type,
            confidence,
        })
    }
}
