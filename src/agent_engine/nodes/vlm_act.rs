//! VlmActNode — VLM autonomous mode with conversation memory.
//!
//! Unlike the previous stateless design, this node now maintains a conversation
//! history in `state.step_messages` across iterations within a step. This gives
//! the VLM continuity: it can see its own prior reasoning, what actions it took,
//! and how the screen changed — enabling natural positive/negative feedback.
//!
//! Image sliding window: only the most recent N screenshots are kept as images;
//! older screenshots are stripped to text placeholders to prevent context explosion.
//!
//! Supports mode switching: if VLM returns `switch_to_chat`, routes to StepRouter
//! for mode change. Routes to `step_evaluate` for loop control.

use async_trait::async_trait;
use base64::Engine as _;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{poll_stop, Node, NodeOutput};
use crate::agent_engine::state::{SharedState, StepMode, StepStatus};
use crate::agent_engine::tool_parser::parse_action_by_name;
use crate::llm::tools::load_builtin_tools;
use crate::llm::types::{ChatMessage, ContentPart, ImageUrl, MessageContent};
use crate::perception::annotator;
use crate::perception::screenshot::capture_primary;
use crate::perception::som_grid::draw_som_grid;

/// Maximum VLM iterations per step (must match step_evaluate::MAX_VLM_ITERATIONS).
const MAX_VLM_ITERATIONS: u32 = 4;

/// Maximum number of screenshots to keep as images in conversation history.
/// Older screenshots are stripped to text placeholders.
/// CUA-style: `only_n_most_recent_images`.
const MAX_RECENT_IMAGES: usize = 2;

/// VLM system prompt with behavioral rules inspired by Open-AutoGLM / CUA Loop.
const VLM_SYSTEM_PROMPT: &str = "\
You are a GUI automation agent that interacts with a computer screen.
You observe screenshots, reason about what you see, and execute ONE action per turn.

## Available tools
mouse_click, mouse_double_click, mouse_right_click, scroll, type_text, hotkey, key_press, wait, finish_step, switch_to_chat.

## Core rules
1. ONE action per turn. Observe the screenshot, decide, act. You will see the result in the next turn.
2. After executing an action, you will receive a new screenshot showing the result. Compare it with the previous state to judge success or failure — this is your feedback signal.
3. Call `finish_step` when the sub-goal is achieved OR when your previous action already accomplished it.
4. Call `switch_to_chat` if the task needs terminal/keyboard operations without vision.

## Element targeting
For mouse_click, use the `element_id` parameter:
- PREFERRED: Use element IDs from the detected elements list (e.g. \"UI_7\"). Match the element by its content/label text, NOT just by visual position.
- FALLBACK: If the target is NOT in the detected elements list, use grid coordinates (e.g. \"C4\", \"E7\") based on the grid overlay on the screenshot.
- Read the element list carefully. Match by content text (e.g. if looking for '英雄联盟', find the element whose content contains that text).

## Anti-loop rules (CRITICAL)
5. If your previous action succeeded (screen changed as expected), call `finish_step` with a summary. Do NOT repeat the action.
6. If you already performed a click/type and the screen shows the expected result, call `finish_step` immediately.
7. If the same action failed twice, try a different approach (different coordinates, different element, scroll first). Do NOT retry the exact same action.
8. If you cannot find the target element after scrolling, call `finish_step` with a failure message rather than looping.
9. Before acting, verify the previous action's effect by comparing the current screenshot with your memory of what you did.
10. Never click the same coordinates more than once if the first click succeeded.
";
pub struct VlmActNode;

impl VlmActNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for VlmActNode {
    fn name(&self) -> &str {
        "vlm_act"
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
            .ok_or_else(|| format!("VlmActNode: no step at index {idx}"))?
            .clone();

        // ── Increment unified iteration counter ─────────────────────────
        state.step_iterations += 1;
        let iter = state.step_iterations;

        let vlm_goal = &step.description;
        let guidance = step.guidance.as_deref().unwrap_or("");

        tracing::info!(
            step = idx, iter, goal = %vlm_goal,
            "[VlmAct] iter={} goal='{}'", iter, truncate(vlm_goal, 80)
        );
        let _ = ctx.app.emit("agent_activity", serde_json::json!({
            "text": format!("VLM 观察屏幕 (第{}次)…", iter)
        }));

        // ── Capture screenshot & run perception pipeline ─────────────────
        let shot = capture_primary().await.map_err(|e| e.to_string())?;
        state.last_meta = Some(shot.meta.clone());

        let (image_b64, elements) = run_perception(ctx, &shot).await?;
        state.detected_elements = elements.clone();

        // Build text listing of detected elements so VLM has both visual AND textual info
        let element_list_text = annotator::build_element_list(&elements);

        let _ = ctx.app.emit("viewport_captured", serde_json::json!({
            "image_base64": &image_b64,
            "grid_n": ctx.grid_n,
            "physical_width": shot.meta.physical_width,
            "physical_height": shot.meta.physical_height,
        }));

        // ── Build / extend conversation in step_messages ─────────────────
        let max_iters = MAX_VLM_ITERATIONS;
        let data_url = format!("data:image/png;base64,{image_b64}");

        if state.step_messages.is_empty() {
            // First iteration: system prompt + initial user message with screenshot
            let mut user_text = format!(
                "Sub-goal: {vlm_goal}\nIteration: {iter}/{max_iters}\n"
            );
            if !guidance.is_empty() {
                user_text.push_str(&format!("Guidance: {guidance}\n"));
            }
            if !state.final_goal.is_empty() {
                user_text.push_str(&format!("Overall goal: {}\n", state.final_goal));
            }
            user_text.push_str(
                "\nAnalyze the screenshot and decide what action to take. Perform ONE action.\n"
            );
            // Inject detected element list so VLM can match IDs to visual labels
            user_text.push_str(&format!("\n{element_list_text}\n"));
            user_text.push_str(
                "\nUse element IDs (e.g. UI_7) from the list above for mouse_click. \
                 If the target element is NOT in the list, you can use grid coordinates (e.g. \"C4\") instead.\n"
            );

            state.step_messages = vec![
                ChatMessage {
                    role: "system".into(),
                    content: MessageContent::Text(VLM_SYSTEM_PROMPT.to_string()),
                    tool_call_id: None,
                    tool_calls: None,
                },
                ChatMessage {
                    role: "user".into(),
                    content: MessageContent::Parts(vec![
                        ContentPart::ImageUrl {
                            image_url: ImageUrl { url: data_url.clone() },
                        },
                        ContentPart::Text { text: user_text },
                    ]),
                    tool_call_id: None,
                    tool_calls: None,
                },
            ];
        } else {
            // Subsequent iteration: inject action result as tool/user message, then new screenshot
            // This creates the implicit visual feedback loop:
            // VLM sees: what it did → what happened → new screenshot → decides next action
            if !state.last_exec_result.is_empty() {
                // If last response had a tool call, inject tool result
                if let Some(ref tid) = state.step_messages.last()
                    .and_then(|m| m.tool_calls.as_ref())
                    .and_then(|tcs| tcs.first())
                    .map(|tc| tc.id.clone())
                {
                    state.step_messages.push(ChatMessage {
                        role: "tool".into(),
                        content: MessageContent::Text(state.last_exec_result.clone()),
                        tool_call_id: Some(tid.clone()),
                        tool_calls: None,
                    });
                }
            }

            // New user message with fresh screenshot — this IS the feedback signal
            let mut feedback_text = format!(
                "Iteration: {iter}/{max_iters}\n"
            );
            if state.last_action_succeeded {
                feedback_text.push_str(&format!(
                    "Previous action `{}` succeeded. Result: {}\n\
                     Compare this screenshot with the previous state. If the sub-goal is achieved, call `finish_step`.\n",
                    state.last_action_kind, state.last_exec_result
                ));
            } else if !state.last_action_kind.is_empty() {
                feedback_text.push_str(&format!(
                    "Previous action `{}` FAILED. Result: {}\n\
                     Try a different approach. Do NOT repeat the same action.\n",
                    state.last_action_kind, state.last_exec_result
                ));
            }
            if iter >= max_iters - 1 {
                feedback_text.push_str(
                    "WARNING: This is your last iteration. You MUST call `finish_step` now.\n"
                );
            }
            // Inject updated element list for this new screenshot
            feedback_text.push_str(&format!("\n{element_list_text}\n"));

            state.step_messages.push(ChatMessage {
                role: "user".into(),
                content: MessageContent::Parts(vec![
                    ContentPart::ImageUrl {
                        image_url: ImageUrl { url: data_url.clone() },
                    },
                    ContentPart::Text { text: feedback_text },
                ]),
                tool_call_id: None,
                tool_calls: None,
            });
        }

        // ── Strip old images (sliding window) ────────────────────────────
        strip_old_images(&mut state.step_messages, MAX_RECENT_IMAGES);

        // ── Filter tools to VLM-relevant set ─────────────────────────────
        let tools = load_builtin_tools()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|t| {
                matches!(
                    t.function.name.as_str(),
                    "mouse_click" | "mouse_double_click" | "mouse_right_click"
                        | "scroll" | "type_text" | "hotkey" | "key_press"
                        | "wait" | "finish_step" | "switch_to_chat"
                )
            })
            .collect::<Vec<_>>();

        let (provider, mut cfg) = {
            let reg = ctx.registry.lock().await;
            reg.call_config_for_role("vision").map_err(|e| e.to_string())?
        };
        cfg.silent = true;

        // ── Call VLM with full conversation ──────────────────────────────
        let messages = state.step_messages.clone();
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

        // ── Log & append assistant response to conversation ──────────────
        let tool_name = response.tool_calls.first()
            .map(|tc| tc.function.name.as_str()).unwrap_or("(none)");
        let content_preview = truncate(response.content.trim(), 100);
        tracing::info!(
            step = idx, iter, tool = tool_name, content = %content_preview,
            "[VlmAct] response: tool={} content='{}'", tool_name, content_preview
        );

        // Append assistant message to conversation for next iteration
        state.step_messages.push(ChatMessage {
            role: "assistant".into(),
            content: MessageContent::Text(response.content.clone()),
            tool_call_id: None,
            tool_calls: if response.tool_calls.is_empty() {
                None
            } else {
                Some(response.tool_calls.clone())
            },
        });

        // ── Parse VLM response ───────────────────────────────────────────
        if let Some(tc) = response.tool_calls.into_iter().next() {
            let args: serde_json::Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));

            match tc.function.name.as_str() {
                "switch_to_chat" => {
                    tracing::info!(step = idx, iter, "[VlmAct] 🔄 switch_to_chat after {} iters", iter);
                    state.mode_switch_requested = Some(StepMode::Chat);
                    return Ok(NodeOutput::GoTo("step_router".to_string()));
                }
                "finish_step" => {
                    let summary = args["summary"].as_str().unwrap_or("Step completed by VLM");
                    // Detect if VLM is reporting failure via finish_step
                    let is_failure = summary_indicates_failure(summary);
                    if is_failure {
                        tracing::warn!(step = idx, iter, summary = %summary,
                            "[VlmAct] ⚠ finish_step with FAILURE after {} iters: '{}'", iter, summary);
                        if let Some(step) = state.todo_steps.get_mut(idx) {
                            step.status = StepStatus::Failed;
                        }
                    } else {
                        tracing::info!(step = idx, iter, summary = %summary,
                            "[VlmAct] ✅ finish_step after {} iters: '{}'", iter, summary);
                    }
                    state.step_complete = true;
                    state.last_exec_result = summary.to_string();
                    state.steps_log.push(format!(
                        "Step {}: {} - {}", idx + 1, step.description, summary
                    ));
                    return Ok(NodeOutput::GoTo("step_evaluate".to_string()));
                }
                name => {
                    state.pending_tool_id = tc.id.clone();
                    match parse_action_by_name(name, &args) {
                        Ok(action) => {
                            state.current_action = Some(action);
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "VlmActNode: failed to parse tool call");
                            // Inject error as tool result for self-correction
                            state.step_messages.push(ChatMessage {
                                role: "tool".into(),
                                content: MessageContent::Text(format!(
                                    "Error: failed to parse action '{}': {}. Try a different action.",
                                    name, e
                                )),
                                tool_call_id: Some(tc.id.clone()),
                                tool_calls: None,
                            });
                            state.steps_log.push(format!("FAIL: VLM act parse error: {e}"));
                            // Let VLM self-correct on next iteration instead of failing
                            return Ok(NodeOutput::GoTo("step_evaluate".to_string()));
                        }
                    }
                }
            }
        } else {
            // No tool call — try parsing JSON from content (fallback)
            let raw = response.content.trim();
            let json_str = raw
                .trim_start_matches("```json")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim();

            match serde_json::from_str::<serde_json::Value>(json_str) {
                Ok(v) => {
                    let name = v["name"].as_str()
                        .or_else(|| v["tool_call"]["name"].as_str());
                    let args = v.get("arguments")
                        .or_else(|| v.get("tool_call").and_then(|tc| tc.get("arguments")));
                    if let (Some(name), Some(args)) = (name, args) {
                        match name {
                            "finish_step" => {
                                let summary = args["summary"].as_str()
                                    .unwrap_or("Step completed by VLM");
                                state.step_complete = true;
                                state.last_exec_result = summary.to_string();
                                return Ok(NodeOutput::GoTo("step_evaluate".to_string()));
                            }
                            "switch_to_chat" => {
                                state.mode_switch_requested = Some(StepMode::Chat);
                                return Ok(NodeOutput::GoTo("step_router".to_string()));
                            }
                            _ => match parse_action_by_name(name, args) {
                                Ok(action) => {
                                    state.current_action = Some(action);
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "VlmActNode: content parse failed");
                                    state.steps_log.push(format!("FAIL: VLM act: {e}"));
                                    if let Some(step) = state.todo_steps.get_mut(idx) {
                                        step.status = StepStatus::Failed;
                                    }
                                    return Ok(NodeOutput::GoTo("step_evaluate".to_string()));
                                }
                            },
                        }
                    } else {
                        tracing::warn!("VlmActNode: no tool call in VLM response");
                        state.steps_log.push("FAIL: VLM act returned no action".to_string());
                        if let Some(step) = state.todo_steps.get_mut(idx) {
                            step.status = StepStatus::Failed;
                        }
                        return Ok(NodeOutput::GoTo("step_evaluate".to_string()));
                    }
                }
                Err(_) => {
                    tracing::warn!("VlmActNode: couldn't parse VLM response");
                    state.steps_log.push("FAIL: VLM act response unparseable".to_string());
                    if let Some(step) = state.todo_steps.get_mut(idx) {
                        step.status = StepStatus::Failed;
                    }
                    return Ok(NodeOutput::GoTo("step_evaluate".to_string()));
                }
            }
        }

        Ok(NodeOutput::Continue)
    }
}

/// Strip images from older messages, keeping only the most recent `keep` images.
/// Older images are replaced with a text placeholder: "[Previous screenshot]".
/// This is the CUA-style `only_n_most_recent_images` strategy.
fn strip_old_images(messages: &mut [ChatMessage], keep: usize) {
    // Count total images (from newest to oldest)
    let mut image_positions: Vec<usize> = Vec::new();
    for (i, msg) in messages.iter().enumerate() {
        if let MessageContent::Parts(parts) = &msg.content {
            if parts.iter().any(|p| matches!(p, ContentPart::ImageUrl { .. })) {
                image_positions.push(i);
            }
        }
    }

    // Strip all but the last `keep` images
    if image_positions.len() <= keep {
        return;
    }
    let strip_count = image_positions.len() - keep;
    for &msg_idx in image_positions.iter().take(strip_count) {
        if let MessageContent::Parts(ref mut parts) = messages[msg_idx].content {
            // Replace ImageUrl parts with text placeholder
            let mut new_parts = Vec::new();
            let mut replaced = false;
            for part in parts.drain(..) {
                match part {
                    ContentPart::ImageUrl { .. } => {
                        if !replaced {
                            new_parts.push(ContentPart::Text {
                                text: "[Previous screenshot — image stripped to save context]".to_string(),
                            });
                            replaced = true;
                        }
                    }
                    other => new_parts.push(other),
                }
            }
            *parts = new_parts;
        }
    }
}

/// Run the perception pipeline (YOLO / UIA / SoM grid) on a screenshot.
async fn run_perception(
    ctx: &NodeContext,
    shot: &crate::perception::screenshot::ScreenshotResult,
) -> Result<(String, Vec<crate::perception::types::UIElement>), String> {
    let mut detector = ctx.yolo_detector.lock().await;
    let mut elements = if let Some(ref mut det) = *detector {
        det.detect(&shot.image_bytes).unwrap_or_default()
    } else {
        Vec::new()
    };

    if ctx.perception_cfg.enable_ui_automation {
        if let Ok(uia) = crate::perception::ui_automation::collect_ui_elements(&shot.meta).await {
            crate::perception::ui_automation::merge_detections(&mut elements, uia, 0.3);
        }
    }

    if !elements.is_empty() {
        let annotated = annotator::annotate_image(&shot.image_bytes, &elements)
            .map_err(|e| e.to_string())?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&annotated);
        Ok((b64, elements))
    } else {
        let grid = draw_som_grid(&shot.image_bytes, ctx.grid_n)
            .unwrap_or_else(|_| shot.image_bytes.clone());
        let b64 = base64::engine::general_purpose::STANDARD.encode(&grid);
        Ok((b64, Vec::new()))
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

/// Detect if a finish_step summary indicates failure rather than success.
/// VLMs sometimes call finish_step with a failure message instead of retrying.
fn summary_indicates_failure(summary: &str) -> bool {
    let lower = summary.to_lowercase();
    let failure_keywords = [
        "fail", "unable", "cannot", "could not", "couldn't",
        "not found", "not able", "impossible", "error",
        "失败", "无法", "找不到", "未能", "不能",
    ];
    failure_keywords.iter().any(|kw| lower.contains(kw))
}
