//! PlannerNode — calls the planner LLM to generate a TodoList for complex tasks.
//!
//! This node:
//! 1. Loads builtin tools + relevant skills.
//! 2. Sends the conversation to the LLM (tools role).
//! 3. Parses the `plan_task` tool call response.
//! 4. Writes the resulting TodoStep list into SharedState.

use async_trait::async_trait;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{poll_stop, Node, NodeOutput};
use crate::agent_engine::state::{AgentAction, GraphResult, RouteType, SharedState};
use crate::agent_engine::tool_parser::parse_tool_call_to_action;
use crate::llm::tools::load_builtin_tools;
use crate::llm::types::{ChatMessage, ContentPart, ImageUrl, MessageContent, StreamChunk, StreamChunkKind};
use crate::perception::screenshot::capture_primary;

const PLANNER_SYSTEM: &str = include_str!("../../../prompts/system/planner.md");

pub struct PlannerNode;

impl PlannerNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for PlannerNode {
    fn name(&self) -> &str {
        "planner"
    }

    async fn execute(
        &self,
        state: &mut SharedState,
        ctx: &NodeContext,
    ) -> Result<NodeOutput, String> {
        if state.is_stopped() {
            return Ok(NodeOutput::End);
        }

        tracing::info!(goal = %state.goal, cycle = state.cycle_count, "PlannerNode: calling planner LLM");
        let _ = ctx.app.emit("agent_activity", serde_json::json!({ "text": "正在规划任务步骤…" }));
        state.cycle_count += 1;

        // Initialise conversation if empty (first call)
        if state.conv_messages.is_empty() {
            // Build system prompt: base prompt + skills context (if any)
            let system_prompt = if ctx.skills_context.is_empty() {
                PLANNER_SYSTEM.to_string()
            } else {
                format!("{}\n\n{}", PLANNER_SYSTEM, ctx.skills_context)
            };

            // Only capture an initial screenshot when the route is ComplexVisual.
            // For plain Complex tasks (e.g. terminal commands, file operations)
            // the screenshot is unnecessary and can even confuse the planner by
            // showing the SeeClaw UI itself.
            let needs_visual = state.route_type == RouteType::ComplexVisual;

            let user_content = if needs_visual {
                match capture_primary().await {
                    Ok(shot) => {
                        tracing::info!("PlannerNode: initial screenshot captured for planning context (ComplexVisual)");
                        let _ = ctx.app.emit("viewport_captured", serde_json::json!({
                            "image_base64": &shot.image_base64,
                            "source": "planner_initial",
                        }));
                        let _ = ctx.app.emit("agent_activity", serde_json::json!({
                            "text": "已截取当前屏幕，正在结合画面制定计划…"
                        }));
                        let data_url = format!("data:image/jpeg;base64,{}", shot.image_base64);
                        MessageContent::Parts(vec![
                            ContentPart::ImageUrl {
                                image_url: ImageUrl { url: data_url },
                            },
                            ContentPart::Text {
                                text: state.goal.clone(),
                            },
                        ])
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "PlannerNode: screenshot failed, falling back to text-only planning");
                        MessageContent::Text(state.goal.clone())
                    }
                }
            } else {
                tracing::info!("PlannerNode: Complex route — skipping initial screenshot");
                let _ = ctx.app.emit("agent_activity", serde_json::json!({
                    "text": "正在制定任务计划…"
                }));
                MessageContent::Text(state.goal.clone())
            };

            state.conv_messages = vec![
                ChatMessage {
                    role: "system".into(),
                    content: MessageContent::Text(system_prompt),
                    tool_call_id: None,
                    tool_calls: None,
                },
                ChatMessage {
                    role: "user".into(),
                    content: user_content,
                    tool_call_id: None,
                    tool_calls: None,
                },
            ];
        }

        // Load tools
        let tools = load_builtin_tools().map_err(|e| e.to_string())?;
        let messages = state.conv_messages.clone();

        // Get provider — planner reasoning is internal, don't stream to frontend
        let (provider, mut cfg) = {
            let reg = ctx.registry.lock().await;
            reg.call_config_for_role("tools").map_err(|e| e.to_string())?
        };
        cfg.silent = true;

        // Race LLM call against stop flag
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
            let tool_name = response.tool_calls.first().map(|tc| tc.function.name.as_str()).unwrap_or("(none)");
            let content_preview = truncate(response.content.trim(), 100);
            tracing::info!(
                tool = tool_name,
                content = %content_preview,
                "[Planner] response: tool={} content='{}'",
                tool_name, content_preview
            );
        }

        // Process tool call
        if let Some(tc) = response.tool_calls.into_iter().next() {
            // Append assistant message
            state.conv_messages.push(ChatMessage {
                role: "assistant".into(),
                content: MessageContent::Text(response.content.clone()),
                tool_call_id: None,
                tool_calls: Some(vec![tc.clone()]),
            });
            state.pending_tool_id = tc.id.clone();

            match parse_tool_call_to_action(&tc) {
                Ok(AgentAction::PlanTask {
                    ref final_goal,
                    ref plan_summary,
                    ref steps,
                }) => {
                    state.final_goal = final_goal.clone();
                    state.plan_summary = plan_summary.clone();
                    state.todo_steps = steps.clone();
                    state.current_step_idx = 0;
                    state.steps_log.clear();
                    tracing::info!(
                        steps = steps.len(),
                        final_goal = %final_goal,
                        "PlannerNode: plan created"
                    );

                    // Ack the plan_task tool call
                    state.conv_messages.push(ChatMessage {
                        role: "tool".into(),
                        content: MessageContent::Text(format!(
                            "Plan accepted: {} steps.",
                            steps.len()
                        )),
                        tool_call_id: Some(state.pending_tool_id.clone()),
                        tool_calls: None,
                    });

                    // Emit todolist to frontend
                    let _ = ctx.app.emit("todolist_updated", serde_json::json!({
                        "steps": &state.todo_steps,
                        "total": state.todo_steps.len(),
                    }));

                    Ok(NodeOutput::Continue)
                }
                Ok(AgentAction::FinishTask { ref summary }) => {
                    tracing::info!(summary = %summary, "PlannerNode: task finished");
                    let _ = ctx.app.emit("llm_stream_chunk", &StreamChunk {
                        kind: StreamChunkKind::Content,
                        content: summary.clone(),
                    });
                    let _ = ctx.app.emit("llm_stream_chunk", &StreamChunk {
                        kind: StreamChunkKind::Done,
                        content: String::new(),
                    });
                    state.result = Some(GraphResult::Done {
                        summary: summary.clone(),
                    });
                    Ok(NodeOutput::End)
                }
                Ok(AgentAction::ReportFailure { ref reason, .. }) => {
                    tracing::warn!(reason = %reason, "PlannerNode: task failure reported");
                    let _ = ctx.app.emit("llm_stream_chunk", &StreamChunk {
                        kind: StreamChunkKind::Content,
                        content: format!("Task failed: {reason}"),
                    });
                    let _ = ctx.app.emit("llm_stream_chunk", &StreamChunk {
                        kind: StreamChunkKind::Done,
                        content: String::new(),
                    });
                    state.result = Some(GraphResult::Error {
                        message: reason.clone(),
                    });
                    Ok(NodeOutput::End)
                }
                Ok(action) => {
                    // Direct action from planner (rare but possible)
                    state.current_action = Some(action);
                    Ok(NodeOutput::GoTo("action_exec".to_string()))
                }
                Err(e) => {
                    // Unknown tool — inject error feedback for self-correction
                    tracing::warn!(error = %e, tool = %tc.function.name, "[Planner] unrecognised tool");
                    state.conv_messages.push(ChatMessage {
                        role: "tool".into(),
                        content: MessageContent::Text(format!(
                            "Error: unknown tool '{}'. Please call plan_task or one of the registered tools.",
                            tc.function.name
                        )),
                        tool_call_id: Some(tc.id.clone()),
                        tool_calls: None,
                    });
                    // Re-enter planner for self-correction
                    Ok(NodeOutput::GoTo("planner".to_string()))
                }
            }
        } else {
            // Content-only response — treat as done
            tracing::info!("[Planner] content-only response → done");
            state.result = Some(GraphResult::Done {
                summary: response.content,
            });
            Ok(NodeOutput::End)
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
