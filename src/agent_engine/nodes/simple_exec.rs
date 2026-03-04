//! SimpleExecNode — generates exactly one tool call for a Simple-route task.
//!
//! **Design**: The Router is a pure classifier. When it decides a task is
//! "Simple", it no longer generates tool calls itself (that would violate SRP
//! and bloat the Router's prompt with tool awareness). Instead, routing hands
//! off to this dedicated node, which uses a minimal, tool-focused prompt to
//! produce the single action needed.
//!
//! Flow: `router` → (Simple) → `simple_exec` → `action_exec` → `summarizer`

use async_trait::async_trait;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{poll_stop, Node, NodeOutput};
use crate::agent_engine::state::{RouteType, SharedState};
use crate::agent_engine::tool_parser::parse_tool_call_to_action;
use crate::llm::tools::load_builtin_tools;
use crate::llm::types::{ChatMessage, MessageContent};

const SIMPLE_EXECUTOR_SYSTEM: &str =
    include_str!("../../../prompts/system/simple_exec.md");

pub struct SimpleExecNode;

impl SimpleExecNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for SimpleExecNode {
    fn name(&self) -> &str {
        "simple_exec"
    }

    async fn execute(
        &self,
        state: &mut SharedState,
        ctx: &NodeContext,
    ) -> Result<NodeOutput, String> {
        if state.is_stopped() {
            return Ok(NodeOutput::End);
        }

        tracing::info!(goal = %state.goal, "SimpleExecNode: generating tool call");
        let _ = ctx
            .app
            .emit("agent_activity", serde_json::json!({ "text": "正在执行简单任务…" }));

        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: MessageContent::Text(SIMPLE_EXECUTOR_SYSTEM.to_string()),
                tool_call_id: None,
                tool_calls: None,
            },
            ChatMessage {
                role: "user".into(),
                content: MessageContent::Text(state.goal.clone()),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        // Load the full builtin tool set — SimpleExec picks the right one from it.
        // The focused system prompt ensures only a single action is generated.
        let tools = load_builtin_tools().map_err(|e| e.to_string())?;

        let (provider, mut cfg) = {
            let reg = ctx.registry.lock().await;
            reg.call_config_for_role("tools").map_err(|e| e.to_string())?
        };
        cfg.silent = true;

        let flag = state.stop_flag.clone();
        let response = tokio::select! {
            result = provider.chat(messages, tools, &cfg, &ctx.app) => {
                result.map_err(|e| e.to_string())?
            }
            _ = poll_stop(flag) => {
                return Ok(NodeOutput::End);
            }
        };

        if state.is_stopped() {
            return Ok(NodeOutput::End);
        }

        if let Some(tc) = response.tool_calls.into_iter().next() {
            match parse_tool_call_to_action(&tc) {
                Ok(action) => {
                    tracing::info!(tool = %tc.function.name, "SimpleExecNode: action ready");
                    state.current_action = Some(action);
                    return Ok(NodeOutput::Continue);
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "SimpleExecNode: parse failed — escalating to planner"
                    );
                }
            }
        } else {
            tracing::warn!("SimpleExecNode: LLM returned no tool call — escalating to planner");
        }

        // Fallback: promote to the full Complex planning path rather than silently dropping the task.
        state.route_type = RouteType::Complex;
        Ok(NodeOutput::GoTo("planner".to_string()))
    }
}
