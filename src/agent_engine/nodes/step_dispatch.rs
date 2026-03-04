//! StepDispatchNode — reads the current TodoStep and routes to the correct
//! execution node based on StepMode (Direct / VisualLocate / VisualAct).

use async_trait::async_trait;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{poll_stop, Node, NodeOutput};
use crate::agent_engine::state::{SharedState, StepMode, StepStatus};

pub struct StepDispatchNode;

impl StepDispatchNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for StepDispatchNode {
    fn name(&self) -> &str {
        "step_dispatch"
    }

    async fn execute(
        &self,
        state: &mut SharedState,
        ctx: &NodeContext,
    ) -> Result<NodeOutput, String> {
        if state.is_stopped() {
            return Ok(NodeOutput::End);
        }

        let idx = state.current_step_idx;
        if idx >= state.todo_steps.len() {
            // All steps done → go to verifier
            return Ok(NodeOutput::GoTo("verifier".to_string()));
        }

        let step = &mut state.todo_steps[idx];
        step.status = StepStatus::InProgress;

        tracing::info!(
            step = idx,
            mode = ?step.mode,
            desc = %step.description,
            "StepDispatchNode: dispatching step"
        );

        // Emit step_started to frontend
        let _ = ctx.app.emit("step_started", serde_json::json!({
            "index": idx,
            "description": &step.description,
            "mode": &step.mode,
        }));

        // Inter-step delay: give the OS time to process previous UI action.
        // Wrapped in select! so a Stop cancels it immediately.
        if idx > 0 {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {}
                _ = poll_stop(state.stop_flag.clone()) => return Ok(NodeOutput::End),
            }
        }

        let target = match step.mode {
            StepMode::Combo => "combo_exec",
            StepMode::Direct => "direct_exec",
            StepMode::VisualLocate => "vlm_observe",
            StepMode::VisualAct => "vlm_act",
        };

        Ok(NodeOutput::GoTo(target.to_string()))
    }
}
