//! VlmActNode — VLM autonomous mode: captures screenshot, sends sub-goal to VLM,
//! VLM generates tool_calls which are then set as the current action.

use async_trait::async_trait;
use base64::Engine as _;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{poll_stop, Node, NodeOutput};
use crate::agent_engine::state::{SharedState, StepStatus};
use crate::agent_engine::tool_parser::parse_action_by_name;
use crate::llm::tools::load_builtin_tools;
use crate::llm::types::{ChatMessage, ContentPart, ImageUrl, MessageContent};
use crate::perception::annotator;
use crate::perception::screenshot::capture_primary;
use crate::perception::som_grid::draw_som_grid;

#[allow(dead_code)]
const VLM_ACT_SYSTEM: &str = include_str!("../../../prompts/system/vlm_annotated.md");

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

        let vlm_goal = step
            .vlm_goal
            .as_deref()
            .unwrap_or(&step.description);

        tracing::info!(step = idx, goal = %vlm_goal, "VlmActNode: autonomous VLM mode");
        let _ = ctx.app.emit("agent_activity", serde_json::json!({ "text": "正在截取屏幕…" }));

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

        // Build VLM prompt with sub-goal
        let vlm_prompt = format!(
            "You are a GUI automation agent. Your sub-goal is: {vlm_goal}\n\
             Analyze the screenshot and decide what action to take.\n\
             Return a JSON with tool_call: {{\"name\": \"<tool>\", \"arguments\": {{...}}}}\n\
             Available tools: mouse_click, type_text, hotkey, key_press, scroll, wait, finish_task."
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

        let tools = load_builtin_tools().map_err(|e| e.to_string())?;
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

        // Parse VLM response — try tool_calls first, then JSON in content
        if let Some(tc) = response.tool_calls.into_iter().next() {
            let args: serde_json::Value =
                serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));
            match parse_action_by_name(&tc.function.name, &args) {
                Ok(action) => {
                    state.current_action = Some(action);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "VlmActNode: failed to parse tool call");
                    state.steps_log.push(format!("FAIL: VLM act parse error: {e}"));
                    if let Some(step) = state.todo_steps.get_mut(idx) { step.status = StepStatus::Failed; }
                    return Ok(NodeOutput::GoTo("step_advance".to_string()));
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
                    let name = v["name"].as_str().or_else(|| v["tool_call"]["name"].as_str());
                    let args = v.get("arguments").or_else(|| v.get("tool_call").and_then(|tc| tc.get("arguments")));
                    if let (Some(name), Some(args)) = (name, args) {
                        match parse_action_by_name(name, args) {
                            Ok(action) => {
                                state.current_action = Some(action);
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "VlmActNode: content parse failed");
                                state.steps_log.push(format!("FAIL: VLM act: {e}"));
                                if let Some(step) = state.todo_steps.get_mut(idx) { step.status = StepStatus::Failed; }
                                return Ok(NodeOutput::GoTo("step_advance".to_string()));
                            }
                        }
                    } else {
                        tracing::warn!("VlmActNode: no tool call in VLM response");
                        state.steps_log.push("FAIL: VLM act returned no action".to_string());
                        if let Some(step) = state.todo_steps.get_mut(idx) { step.status = StepStatus::Failed; }
                        return Ok(NodeOutput::GoTo("step_advance".to_string()));
                    }
                }
                Err(_) => {
                    tracing::warn!("VlmActNode: couldn't parse VLM response");
                    state.steps_log.push("FAIL: VLM act response unparseable".to_string());
                    if let Some(step) = state.todo_steps.get_mut(idx) { step.status = StepStatus::Failed; }
                    return Ok(NodeOutput::GoTo("step_advance".to_string()));
                }
            }
        }

        Ok(NodeOutput::Continue)
    }
}


