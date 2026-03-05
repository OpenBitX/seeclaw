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

        // ── Pre-flight check: tasks involving visual GUI actions (click, drag,
        // etc.) CANNOT be handled by SimpleExec because this node has no
        // vision. Escalate immediately to ComplexVisual → Planner → VLM
        // instead of wasting 10-30s on an LLM call that will inevitably fail
        // or produce a terminal-command workaround.
        if needs_vision(&state.goal) {
            tracing::info!(
                goal = %state.goal,
                "SimpleExecNode: task requires vision (click/GUI element) — escalating to ComplexVisual"
            );
            let _ = ctx.app.emit(
                "agent_activity",
                serde_json::json!({ "text": "该任务需要视觉，切换到视觉模式…" }),
            );
            state.route_type = RouteType::ComplexVisual;
            return Ok(NodeOutput::GoTo("planner".to_string()));
        }

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

        // Load builtin tools, but FILTER OUT internal loop-control tools that
        // only make sense inside the step loop (chat_agent / vlm_act). If they
        // leak here, the LLM will try to call switch_to_vlm instead of doing the
        // actual single-step action.
        let tools = load_builtin_tools()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|t| {
                let name = &t.function.name;
                !matches!(
                    name.as_str(),
                    "plan_task"
                        | "evaluate_completion"
                        | "finish_step"
                        | "switch_to_vlm"
                        | "switch_to_chat"
                )
            })
            .collect::<Vec<_>>();

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
            let tool_name = response.tool_calls.first().map(|tc| tc.function.name.as_str()).unwrap_or("(none)");
            let content_preview = truncate(response.content.trim(), 100);
            tracing::info!(
                tool = tool_name,
                content = %content_preview,
                "[SimpleExec] response: tool={} content='{}'",
                tool_name, content_preview
            );
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

        // Fallback: promote to the full planning path.
        // Use ComplexVisual (not Complex) because a Simple-route that failed
        // typically needs vision context (e.g. "click desktop icon" requires
        // a screenshot to know WHERE to click).
        state.route_type = RouteType::ComplexVisual;
        Ok(NodeOutput::GoTo("planner".to_string()))
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

/// Pre-flight heuristic: does this task require visual perception?
/// If it mentions clicking, dragging, or interacting with GUI elements
/// by visual reference, SimpleExec (text-only) cannot handle it.
fn needs_vision(goal: &str) -> bool {
    let goal_lower = goal.to_lowercase();
    let click_patterns = [
        "点击", "双击", "右键", "单击",
        "click", "double click", "right click",
        "图标", "icon", "按钮", "button",
        "拖拽", "drag", "拖动",
    ];
    click_patterns.iter().any(|p| goal_lower.contains(p))
}
