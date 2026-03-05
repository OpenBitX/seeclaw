//! VlmActNode — VLM autonomous mode: captures screenshot, sends sub-goal to VLM,
//! VLM generates tool_calls which are then set as the current action.
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

#[allow(dead_code)]
const VLM_ACT_SYSTEM: &str = include_str!("../../../prompts/system/vlm_annotated.md");

/// Maximum VLM iterations per step (must match step_evaluate::MAX_VLM_ITERATIONS).
const MAX_VLM_ITERATIONS: u32 = 4;

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

        // Use step description as the sub-goal, with guidance as additional context
        let vlm_goal = &step.description;
        let guidance = step.guidance.as_deref().unwrap_or("");

        tracing::info!(
            step = idx,
            iter,
            goal = %vlm_goal,
            "[VlmAct] iter={} goal='{}'",
            iter,
            truncate(vlm_goal, 80)
        );
        let _ = ctx.app.emit("agent_activity", serde_json::json!({ "text": format!("VLM 观察屏幕 (第{}次)…", iter) }));

        // Capture screenshot
        let shot = capture_primary().await.map_err(|e| e.to_string())?;
        state.last_meta = Some(shot.meta.clone());

        // Run perception pipeline (YOLO / UIA / SoM grid)
        let (image_b64, elements) = {
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
                (b64, elements)
            } else {
                let grid = draw_som_grid(&shot.image_bytes, ctx.grid_n)
                    .unwrap_or_else(|_| shot.image_bytes.clone());
                let b64 = base64::engine::general_purpose::STANDARD.encode(&grid);
                (b64, Vec::new())
            }
        };

        state.detected_elements = elements;

        // ── Emit screenshot to frontend (UI perception feedback) ────────
        let _ = ctx.app.emit("viewport_captured", serde_json::json!({
            "image_base64": &image_b64,
            "grid_n": ctx.grid_n,
            "physical_width": shot.meta.physical_width,
            "physical_height": shot.meta.physical_height,
        }));

        // Build VLM prompt with sub-goal, iteration awareness, and action history
        let max_iters = MAX_VLM_ITERATIONS;
        let mut vlm_prompt = format!(
            "You are a GUI automation agent. Your sub-goal is: {vlm_goal}\n\
             Iteration: {iter}/{max_iters}\n"
        );
        if !guidance.is_empty() {
            vlm_prompt.push_str(&format!("Guidance: {guidance}\n"));
        }
        if !state.final_goal.is_empty() {
            vlm_prompt.push_str(&format!("Overall goal: {}\n", state.final_goal));
        }

        // ── Inject action history so VLM knows what already happened ────
        if !state.step_action_history.is_empty() {
            vlm_prompt.push_str("\n## Actions already taken in this step:\n");
            for entry in &state.step_action_history {
                vlm_prompt.push_str(&format!("- {entry}\n"));
            }
            vlm_prompt.push_str("\n");
        }

        if !state.last_exec_result.is_empty() {
            vlm_prompt.push_str(&format!("Last action result: {}\n", state.last_exec_result));
        }

        vlm_prompt.push_str(
            "Analyze the screenshot and decide what action to take.\n\
             Return a tool call. Available tools: mouse_click, mouse_double_click, type_text, hotkey, key_press, scroll, wait, finish_step, switch_to_chat.\n\n\
             CRITICAL RULES:\n\n\
             1. ONE ACTION PER STEP. Each step is a SINGLE atomic action. Perform ONE action (click, type, scroll), then call `finish_step` on the NEXT iteration. Do NOT try to complete the entire goal in one step.\n\n\
             2. WHEN TO CALL `finish_step`:\n\
                - If the 'Actions already taken' list shows ANY successful action, call `finish_step` immediately with a brief summary. Do NOT perform another action.\n\
                - If the screenshot visually confirms the sub-goal is achieved, call `finish_step`.\n\
                - The sub-goal is about PERFORMING the action, not waiting for the result. Once a click/type/scroll on the correct target is done, the step is complete.\n\
                - If unsure whether the action worked, call `finish_step` rather than retrying blindly.\n\n"
        );

        // Iteration-aware urgency
        if iter >= 2 {
            vlm_prompt.push_str(&format!(
                "3. URGENT: You are on iteration {iter} of {max_iters}. You have already performed action(s). \
                 You MUST call `finish_step` NOW with a summary of what was done. Do NOT perform another action.\n\n"
            ));
        } else {
            vlm_prompt.push_str(&format!(
                "3. You are on iteration {iter} of {max_iters}. After iteration {max_iters}, this step will be force-failed. \
                 Perform ONE action now, and call `finish_step` on the next iteration.\n\n"
            ));
        }

        vlm_prompt.push_str(
            "4. Call `switch_to_chat` if this task needs terminal/keyboard operations without vision.\n\
             5. Do NOT click the same element again if the action history shows it already succeeded.\n"
        );

        let data_url = format!("data:image/png;base64,{image_b64}");
        let messages = vec![ChatMessage {
            role: "user".into(),
            content: MessageContent::Parts(vec![
                ContentPart::ImageUrl {
                    image_url: ImageUrl { url: data_url },
                },
                ContentPart::Text {
                    text: vlm_prompt,
                },
            ]),
            tool_call_id: None,
            tool_calls: None,
        }];

        // Filter tools to only those relevant for VLM visual actions.
        // Internal tools (plan_task, evaluate_completion, execute_terminal, mcp_call, invoke_skill)
        // should NOT leak to the VLM — they cause confusion and wasted tokens.
        let tools = load_builtin_tools()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|t| {
                matches!(
                    t.function.name.as_str(),
                    "mouse_click"
                        | "mouse_double_click"
                        | "mouse_right_click"
                        | "scroll"
                        | "type_text"
                        | "hotkey"
                        | "key_press"
                        | "wait"
                        | "finish_step"
                        | "switch_to_chat"
                )
            })
            .collect::<Vec<_>>();
        let (provider, mut cfg) = {
            let reg = ctx.registry.lock().await;
            reg.call_config_for_role("vision").map_err(|e| e.to_string())?
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

        // ── Log VLM response (truncated) ────────────────────────────────
        {
            let tool_name = response.tool_calls.first().map(|tc| tc.function.name.as_str()).unwrap_or("(none)");
            let content_preview = truncate(response.content.trim(), 100);
            tracing::info!(
                step = idx,
                iter,
                tool = tool_name,
                content = %content_preview,
                "[VlmAct] response: tool={} content='{}'",
                tool_name, content_preview
            );
        }

        // Parse VLM response — try tool_calls first, then JSON in content
        if let Some(tc) = response.tool_calls.into_iter().next() {
            let args: serde_json::Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));

            match tc.function.name.as_str() {
                // Mode switch signal
                "switch_to_chat" => {
                    tracing::info!(step = idx, iter, "[VlmAct] 🔄 switch_to_chat requested after {} iters", iter);
                    state.mode_switch_requested = Some(StepMode::Chat);
                    return Ok(NodeOutput::GoTo("step_router".to_string()));
                }
                // Step completion signal
                "finish_step" => {
                    let summary = args["summary"].as_str().unwrap_or("Step completed by VLM");
                    tracing::info!(step = idx, iter, summary = %summary, "[VlmAct] ✅ finish_step after {} iters: '{}'", iter, summary);
                    state.step_complete = true;
                    state.last_exec_result = summary.to_string();
                    state.steps_log.push(format!(
                        "Step {}: {} - {}",
                        idx + 1, step.description, summary
                    ));
                    return Ok(NodeOutput::GoTo("step_evaluate".to_string()));
                }
                // Regular tool call
                name => {
                    match parse_action_by_name(name, &args) {
                        Ok(action) => {
                            state.current_action = Some(action);
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "VlmActNode: failed to parse tool call");
                            state.steps_log.push(format!("FAIL: VLM act parse error: {e}"));
                            if let Some(step) = state.todo_steps.get_mut(idx) {
                                step.status = StepStatus::Failed;
                            }
                            return Ok(NodeOutput::GoTo("step_evaluate".to_string()));
                        }
                    }
                }
            }
        } else {
            // Try parsing JSON from content
            let raw = response.content.trim();
            let json_str = raw
                .trim_start_matches("```json")
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim();

            match serde_json::from_str::<serde_json::Value>(json_str) {
                Ok(v) => {
                    let name = v["name"]
                        .as_str()
                        .or_else(|| v["tool_call"]["name"].as_str());
                    let args = v
                        .get("arguments")
                        .or_else(|| v.get("tool_call").and_then(|tc| tc.get("arguments")));
                    if let (Some(name), Some(args)) = (name, args) {
                        match name {
                            "finish_step" => {
                                let summary =
                                    args["summary"].as_str().unwrap_or("Step completed by VLM");
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
                        state
                            .steps_log
                            .push("FAIL: VLM act returned no action".to_string());
                        if let Some(step) = state.todo_steps.get_mut(idx) {
                            step.status = StepStatus::Failed;
                        }
                        return Ok(NodeOutput::GoTo("step_evaluate".to_string()));
                    }
                }
                Err(_) => {
                    tracing::warn!("VlmActNode: couldn't parse VLM response");
                    state
                        .steps_log
                        .push("FAIL: VLM act response unparseable".to_string());
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

/// Truncate to `max` chars with "…" if longer (for log display).
fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > max {
        format!("{}…", chars[..max].iter().collect::<String>())
    } else {
        s.to_string()
    }
}