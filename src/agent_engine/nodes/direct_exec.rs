//! DirectExecNode — takes pre-generated tool calls from the current TodoStep
//! and converts them into an AgentAction for ActionExecNode.

use async_trait::async_trait;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{Node, NodeOutput};
use crate::agent_engine::state::{AgentAction, SharedState};
use crate::agent_engine::tool_parser::parse_action_by_name;

pub struct DirectExecNode;

impl DirectExecNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for DirectExecNode {
    fn name(&self) -> &str {
        "direct_exec"
    }

    async fn execute(
        &self,
        state: &mut SharedState,
        _ctx: &NodeContext,
    ) -> Result<NodeOutput, String> {
        if state.is_stopped() {
            return Ok(NodeOutput::End);
        }

        // ── Simple route: no todo_steps, use simple_tool_calls directly ──────
        if state.todo_steps.is_empty() {
            if let Some(tool_calls) = state.simple_tool_calls.take() {
                if let Some(tc) = tool_calls.first() {
                    let args: serde_json::Value =
                        serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(serde_json::json!({}));
                    match parse_action_by_name(&tc.function.name, &args) {
                        Ok(action) => state.current_action = Some(action),
                        Err(e) => {
                            tracing::warn!(error = %e, "DirectExecNode: simple route parse failed");
                            state.current_action = Some(AgentAction::Wait { milliseconds: 500 });
                        }
                    }
                }
                return Ok(NodeOutput::Continue);
            } else {
                // No tool calls available — the router classified this as Simple but provided
                // no pre-parsed actions. Fall back to the planner so the task is not lost.
                tracing::warn!("DirectExecNode: Simple route but no simple_tool_calls — falling back to planner");
                use crate::agent_engine::state::RouteType;
                state.route_type = RouteType::Complex;
                return Ok(NodeOutput::GoTo("planner".to_string()));
            }
        }

        let idx = state.current_step_idx;
        let step = state
            .todo_steps
            .get(idx)
            .ok_or_else(|| format!("DirectExecNode: no step at index {idx}"))?
            .clone();

        tracing::info!(step = idx, desc = %step.description, "DirectExecNode: processing");

        // Get the first tool call from the step (execute sequentially)
        if let Some(tc) = step.tool_calls.first() {
            match parse_action_by_name(&tc.name, &tc.arguments) {
                Ok(action) => {
                    state.current_action = Some(action);
                }
                Err(e) => {
                    tracing::warn!(error = %e, step = idx, "DirectExecNode: parse failed, defaulting to wait");
                    state.current_action = Some(crate::agent_engine::state::AgentAction::Wait {
                        milliseconds: 500,
                    });
                }
            }
        } else if let Some(template) = &step.action_template {
            // Fallback: use action_template if no tool_calls
            state.current_action = Some(template.clone());
        } else {
            tracing::warn!(step = idx, "DirectExecNode: no tool_calls and no action_template");
            state.current_action = Some(crate::agent_engine::state::AgentAction::Wait {
                milliseconds: 500,
            });
        }

        Ok(NodeOutput::Continue)
    }
}
