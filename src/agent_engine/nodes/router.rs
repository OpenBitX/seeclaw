//! RouterNode — classifies the user query via the 3-layer Router pipeline
//! and writes the result into SharedState for downstream edge routing.

use async_trait::async_trait;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{Node, NodeOutput};
use crate::agent_engine::router::RouterPipeline;
use crate::agent_engine::state::SharedState;

pub struct RouterNode {
    pipeline: RouterPipeline,
}

impl RouterNode {
    pub fn new() -> Self {
        Self {
            pipeline: RouterPipeline::new(),
        }
    }
}

#[async_trait]
impl Node for RouterNode {
    fn name(&self) -> &str {
        "router"
    }

    async fn execute(
        &self,
        state: &mut SharedState,
        ctx: &NodeContext,
    ) -> Result<NodeOutput, String> {
        if state.is_stopped() {
            return Ok(NodeOutput::End);
        }

        tracing::info!(goal = %state.goal, "RouterNode: classifying query");

        let result = self.pipeline.classify(&state.goal, ctx).await;

        state.route_type = result.route_type.clone();

        tracing::info!(
            route = ?state.route_type,
            "RouterNode: classification complete"
        );

        Ok(NodeOutput::Continue)
    }
}
