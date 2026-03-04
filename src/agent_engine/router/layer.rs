//! Router layer trait — interface for each classification layer.

use async_trait::async_trait;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::state::RouteType;

/// The result from a router layer classification attempt.
///
/// The Router is a **pure classifier** — it only determines `route_type`.
/// Tool-call generation is the responsibility of `SimpleExecNode` (Simple path)
/// or `PlannerNode` (Complex path).
#[derive(Debug, Clone)]
pub struct RouteResult {
    /// The classified route type.
    pub route_type: RouteType,
    /// Confidence score (0.0 – 1.0).
    pub confidence: f32,
}

/// A single classification layer in the Router pipeline.
///
/// Each layer either returns `Some(RouteResult)` (classification succeeded)
/// or `None` (pass to next layer).
#[async_trait]
pub trait RouterLayer: Send + Sync {
    /// Human-readable name of this layer.
    fn name(&self) -> &str;

    /// Attempt to classify the query.
    /// Return `None` to delegate to the next layer.
    async fn classify(
        &self,
        query: &str,
        ctx: &NodeContext,
    ) -> Option<RouteResult>;
}
