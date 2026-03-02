//! StepAdvanceNode — marks the current step complete and advances the index.

use async_trait::async_trait;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{Node, NodeOutput};
use crate::agent_engine::state::{SharedState, StepStatus};

pub struct StepAdvanceNode;

impl StepAdvanceNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for StepAdvanceNode {
    fn name(&self) -> &str {
        "step_advance"
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

        // Mark current step status
        if let Some(step) = state.todo_steps.get_mut(idx) {
            if step.status == StepStatus::InProgress {
                step.status = StepStatus::Completed;
            }
            // If it was already set to Failed/Skipped by another node, keep that
        }

        tracing::info!(
            step = idx,
            status = ?state.todo_steps.get(idx).map(|s| &s.status),
            "StepAdvanceNode: step done"
        );

        // Emit step_completed to frontend
        let _ = ctx.app.emit("step_completed", serde_json::json!({
            "index": idx,
            "status": state.todo_steps.get(idx).map(|s| &s.status),
        }));

        // Emit updated todolist
        let _ = ctx.app.emit("todolist_updated", serde_json::json!({
            "steps": &state.todo_steps,
            "total": state.todo_steps.len(),
            "completed": state.todo_steps.iter().filter(|s| s.status == StepStatus::Completed).count(),
        }));

        // Advance
        state.current_step_idx += 1;
        state.current_action = None;
        state.needs_stability = false;
        state.needs_approval = false;

        Ok(NodeOutput::Continue)
    }
}
