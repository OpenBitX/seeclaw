//! UserConfirmNode — waits for human approval on high-risk actions.

use async_trait::async_trait;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{Node, NodeOutput};
use crate::agent_engine::state::{AgentEvent, SharedState};

pub struct UserConfirmNode;

impl UserConfirmNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for UserConfirmNode {
    fn name(&self) -> &str {
        "user_confirm"
    }

    async fn execute(
        &self,
        state: &mut SharedState,
        ctx: &NodeContext,
    ) -> Result<NodeOutput, String> {
        if state.is_stopped() {
            return Ok(NodeOutput::End);
        }

        let action = state
            .current_action
            .as_ref()
            .ok_or_else(|| "UserConfirmNode: no pending action")?;

        tracing::info!(?action, "UserConfirmNode: waiting for user approval");

        // Emit approval request to frontend
        let req = serde_json::json!({
            "id": format!("step-{}", state.current_step_idx),
            "action": serde_json::to_value(action).unwrap_or_default(),
            "reason": format!("步骤 {}", state.current_step_idx + 1),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        let _ = ctx.app.emit("action_required", &req);

        // Wait for user response via event channel
        match state.event_rx.recv().await {
            Some(AgentEvent::UserApproved) => {
                tracing::info!("UserConfirmNode: approved");
                state.needs_approval = false;
                // Signal to action_exec that this action was explicitly approved,
                // so it must not re-route to user_confirm for the same action.
                state.action_user_approved = true;
                // Action is still in current_action — go to action_exec
                Ok(NodeOutput::GoTo("action_exec".to_string()))
            }
            Some(AgentEvent::UserRejected) | Some(AgentEvent::Stop) | None => {
                tracing::info!("UserConfirmNode: rejected/stop");
                state.current_action = None;
                state.needs_approval = false;
                // Skip this step
                Ok(NodeOutput::GoTo("step_advance".to_string()))
            }
            _ => {
                // Unexpected event — re-wait by going to self
                Ok(NodeOutput::GoTo("user_confirm".to_string()))
            }
        }
    }
}
