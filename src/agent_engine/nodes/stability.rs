//! StabilityNode — waits for UI visual stability after an action.

use async_trait::async_trait;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{Node, NodeOutput};
use crate::agent_engine::state::SharedState;
use crate::perception::screenshot::capture_primary;
use crate::perception::stability::{wait_for_visual_stability, StabilityConfig};

pub struct StabilityNode;

impl StabilityNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for StabilityNode {
    fn name(&self) -> &str {
        "stability"
    }

    async fn execute(
        &self,
        state: &mut SharedState,
        ctx: &NodeContext,
    ) -> Result<NodeOutput, String> {
        if state.is_stopped() {
            return Ok(NodeOutput::End);
        }

        tracing::info!("StabilityNode: waiting for visual stability");
        let _ = ctx.app.emit("agent_activity", serde_json::json!({ "text": "等待页面稳定…" }));

        let config = StabilityConfig {
            max_wait_ms: 3000,
            check_interval_ms: 200,
            stability_threshold: 0.02,
            min_stable_frames: 2,
        };

        let stop_flag = state.stop_flag.clone();
        let capture_fn = || async {
            let result = capture_primary().await?;
            Ok(result.image_bytes)
        };

        match wait_for_visual_stability(capture_fn, config, stop_flag).await {
            Ok(true) => {
                tracing::info!("StabilityNode: visual stability achieved");
            }
            Ok(false) => {
                tracing::warn!("StabilityNode: stability timeout or stopped");
                if state.is_stopped() {
                    return Ok(NodeOutput::End);
                }
                // Timeout — proceed anyway
            }
            Err(e) => {
                tracing::error!(error = %e, "StabilityNode: check failed, proceeding anyway");
            }
        }

        state.needs_stability = false;
        Ok(NodeOutput::Continue)
    }
}
