//! ChatAgentNode — LLM-driven loop for non-visual tasks.
//!
//! Handles: terminal commands, keyboard shortcuts, file operations, text input.
//! Each invocation executes ONE tool call, then returns to step_evaluate
//! which decides whether to loop back or advance.
//!
//! The agent can signal a mode switch to VLM via `switch_to_vlm` tool call.

use async_trait::async_trait;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{poll_stop, Node, NodeOutput};
use crate::agent_engine::state::{SharedState, StepMode};
use crate::agent_engine::tool_parser::parse_action_by_name;
use crate::llm::tools::load_builtin_tools;
use crate::llm::types::{ChatMessage, MessageContent};

const CHAT_AGENT_SYSTEM: &str = include_str!("../../../prompts/system/chat_agent.md");

pub struct ChatAgentNode;

impl ChatAgentNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for ChatAgentNode {
    fn name(&self) -> &str {
        "chat_agent"
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
        let step = state
            .todo_steps
            .get(idx)
            .ok_or_else(|| format!("ChatAgentNode: no step at index {idx}"))?
            .clone();

        tracing::info!(
            step = idx,
            desc = %step.description,
            "ChatAgentNode: processing"
        );
        let _ = ctx.app.emit(
            "agent_activity",
            serde_json::json!({ "text": format!("Chat Agent: {}", step.description) }),
        );

        // ── Increment unified iteration counter ─────────────────────────
        state.step_iterations += 1;
        let iter = state.step_iterations;

        // Build conversation for this chat turn
        if state.step_messages.is_empty() {
            // First turn for this step — build system + context message
            let mut context_parts = vec![
                format!("**Current step goal**: {}", step.description),
                format!("**Final goal**: {}", state.final_goal),
                format!("**Plan summary**: {}", state.plan_summary),
            ];

            if !step.required_skills.is_empty() {
                context_parts.push(format!(
                    "**Required skills**: {}",
                    step.required_skills.join(", ")
                ));
            }

            if let Some(ref guidance) = step.guidance {
                context_parts.push(format!("**Guidance**: {}", guidance));
            }

            state.step_messages = vec![
                ChatMessage {
                    role: "system".into(),
                    content: MessageContent::Text(CHAT_AGENT_SYSTEM.to_string()),
                    tool_call_id: None,
                    tool_calls: None,
                },
                ChatMessage {
                    role: "user".into(),
                    content: MessageContent::Text(context_parts.join("\n")),
                    tool_call_id: None,
                    tool_calls: None,
                },
            ];
        } else if !state.last_exec_result.is_empty() {
            // Subsequent turn — inject last execution result
            state.step_messages.push(ChatMessage {
                role: "tool".into(),
                content: MessageContent::Text(state.last_exec_result.clone()),
                tool_call_id: Some(state.pending_tool_id.clone()),
                tool_calls: None,
            });
        }

        // Load tools and call LLM
        let tools = load_builtin_tools().map_err(|e| e.to_string())?;
        let messages = state.step_messages.clone();

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

        // ── Log LLM response (truncated) ────────────────────────────────
        {
            let tool_name = response.tool_calls.first().map(|tc| tc.function.name.as_str()).unwrap_or("(text)");
            let content_preview = truncate(response.content.trim(), 100);
            tracing::info!(
                step = idx,
                iter,
                tool = tool_name,
                content = %content_preview,
                "[ChatAgent] iter={} response: tool={} content='{}'",
                iter, tool_name, content_preview
            );
        }

        // Process response
        if let Some(tc) = response.tool_calls.into_iter().next() {
            // Append assistant message to step conversation
            state.step_messages.push(ChatMessage {
                role: "assistant".into(),
                content: MessageContent::Text(response.content.clone()),
                tool_call_id: None,
                tool_calls: Some(vec![tc.clone()]),
            });
            state.pending_tool_id = tc.id.clone();

            let args: serde_json::Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));

            match tc.function.name.as_str() {
                // Mode switch signal
                "switch_to_vlm" => {
                    tracing::info!(step = idx, iter, "[ChatAgent] 🔄 switch_to_vlm after {} iters", iter);
                    state.mode_switch_requested = Some(StepMode::Vlm);
                    return Ok(NodeOutput::GoTo("step_router".to_string()));
                }
                // Step completion signal
                "finish_step" => {
                    let summary = args["summary"].as_str().unwrap_or("Step completed");
                    tracing::info!(step = idx, iter, summary = %summary, "[ChatAgent] ✅ finish_step after {} iters: '{}'", iter, summary);
                    state.step_complete = true;
                    state.last_exec_result = summary.to_string();
                    state.steps_log.push(format!(
                        "Step {}: {} - {}",
                        idx + 1,
                        step.description,
                        summary
                    ));
                    return Ok(NodeOutput::GoTo("step_evaluate".to_string()));
                }
                // Regular tool call — convert to action
                name => {
                    match parse_action_by_name(name, &args) {
                        Ok(action) => {
                            state.current_action = Some(action);
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, iter, "[ChatAgent] ⚠ unknown tool '{}' at iter {}", name, iter);
                            state.step_messages.push(ChatMessage {
                                role: "tool".into(),
                                content: MessageContent::Text(format!(
                                    "Error: unknown tool '{}'. Use one of: execute_terminal, hotkey, type_text, key_press, wait, finish_step, switch_to_vlm.",
                                    name
                                )),
                                tool_call_id: Some(tc.id.clone()),
                                tool_calls: None,
                            });
                            // Re-enter chat_agent for self-correction
                            return Ok(NodeOutput::GoTo("chat_agent".to_string()));
                        }
                    }
                }
            }

            // Regular action — go to action_exec, then step_evaluate
            Ok(NodeOutput::Continue)
        } else {
            // Content-only response — treat as step complete
            tracing::info!(step = idx, iter, content = %truncate(&response.content, 100), "[ChatAgent] content-only response → step complete: '{}'", truncate(&response.content, 100));
            state.step_complete = true;
            state.last_exec_result = response.content;
            Ok(NodeOutput::GoTo("step_evaluate".to_string()))
        }
    }
}

/// Truncate to `max` chars with "…" if longer (for log display).
fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > max {
        format!("{}…", chars[..max].iter().collect::<String>())
    } else {
        s.to_string()
    }
}
