//! VlmObserveNode — captures screenshot, runs perception pipeline (YOLO/UIA/SoM),
//! sends to VLM for element location, then patches the action template.

use async_trait::async_trait;
use base64::Engine as _;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{poll_stop, Node, NodeOutput};
use crate::agent_engine::state::{SharedState, StepStatus};
use crate::agent_engine::tool_parser::{action_supports_element_id, extract_cell_label_from_text, patch_element_id};
use crate::llm::types::{ChatMessage, ContentPart, ImageUrl, MessageContent};
use crate::perception::annotator;
use crate::perception::screenshot::capture_primary;
use crate::perception::som_grid::{col_label, draw_som_grid};

const VLM_PROMPT_TEMPLATE: &str = include_str!("../../../prompts/system/vlm_grid.md");
const VLM_ANNOTATED_TEMPLATE: &str = include_str!("../../../prompts/system/vlm_annotated.md");

pub struct VlmObserveNode;

impl VlmObserveNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for VlmObserveNode {
    fn name(&self) -> &str {
        "vlm_observe"
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
            .ok_or_else(|| format!("VlmObserveNode: no step at index {idx}"))?
            .clone();

        let target = step
            .target
            .as_deref()
            .unwrap_or(&step.description);

        tracing::info!(step = idx, target = %target, "VlmObserveNode: locating element");
        let _ = ctx.app.emit("agent_activity", serde_json::json!({ "text": "正在截取屏幕…" }));

        // Capture screenshot
        let shot = capture_primary().await.map_err(|e| e.to_string())?;
        state.last_meta = Some(shot.meta.clone());

        let _ = ctx.app.emit("agent_activity", serde_json::json!({ "text": "正在分析屏幕元素…" }));

        // Try YOLO + UIA annotation pipeline
        let use_annotated = {
            let detector = ctx.yolo_detector.lock().await;
            detector.is_some()
        } || ctx.perception_cfg.enable_ui_automation;

        let cell_result = if use_annotated {
            self.try_annotated_pipeline(state, ctx, &shot.image_bytes, &shot.meta, target).await
        } else {
            Ok(None) // skip to SoM grid
        };

        // If annotated pipeline yielded nothing, try SoM grid fallback
        let cell = match cell_result {
            Ok(Some(cell)) => Some(cell),
            Ok(None) => {
                self.try_som_grid_fallback(state, ctx, &shot.image_bytes, &shot.meta, target).await?
            }
            Err(e) => return Err(e),
        };

        // Patch the action template with the located element
        if let Some(cell_label) = cell {
            let action = if let Some(template) = &step.action_template {
                if action_supports_element_id(template) {
                    patch_element_id(template.clone(), &cell_label)
                } else {
                    template.clone()
                }
            } else {
                // Default to mouse_click on the found element
                crate::agent_engine::state::AgentAction::MouseClick {
                    element_id: cell_label,
                }
            };
            state.current_action = Some(action);
        } else {
            // VLM couldn't find the target — mark step Failed and skip
            let msg = format!(
                "Step {}: VLM could not locate '{}' on screen",
                idx, target
            );
            tracing::warn!("{msg}");
            state.steps_log.push(format!("FAIL: {msg}"));
            if let Some(step) = state.todo_steps.get_mut(idx) {
                step.status = StepStatus::Failed;
            }
            let mut ctrl = ctx.loop_ctrl.lock().await;
            ctrl.record_failure();
            return Ok(NodeOutput::GoTo("step_advance".to_string()));
        }

        Ok(NodeOutput::Continue)
    }
}

impl VlmObserveNode {
    /// Try YOLO/UIA annotated pipeline, returning element_id if found.
    async fn try_annotated_pipeline(
        &self,
        state: &mut SharedState,
        ctx: &NodeContext,
        image_bytes: &[u8],
        meta: &crate::perception::types::ScreenshotMeta,
        target: &str,
    ) -> Result<Option<String>, String> {
        // YOLO detection
        let mut elements = {
            let mut detector = ctx.yolo_detector.lock().await;
            if let Some(ref mut det) = *detector {
                match det.detect(image_bytes) {
                    Ok(elems) => {
                        tracing::info!(count = elems.len(), "YOLO detections");
                        elems
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "YOLO inference failed");
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            }
        };

        // UIA merge
        if ctx.perception_cfg.enable_ui_automation {
            match crate::perception::ui_automation::collect_ui_elements(meta).await {
                Ok(uia) => {
                    tracing::debug!(count = uia.len(), "UIA elements");
                    crate::perception::ui_automation::merge_detections(&mut elements, uia, 0.3);
                }
                Err(e) => tracing::warn!(error = %e, "UIA failed"),
            }
        }

        if elements.is_empty() {
            return Ok(None);
        }

        // Annotate image
        let annotated_bytes =
            annotator::annotate_image(image_bytes, &elements).map_err(|e| e.to_string())?;
        let annotated_b64 = base64::engine::general_purpose::STANDARD.encode(&annotated_bytes);

        state.detected_elements = elements.clone();

        // Emit to frontend
        let _ = ctx.app.emit("viewport_captured", serde_json::json!({
            "image_base64": &annotated_b64,
            "source": "yolo_annotated",
            "element_count": elements.len(),
        }));

        let element_list = annotator::build_element_list(&elements);
        let vlm_prompt = VLM_ANNOTATED_TEMPLATE
            .replace("{element_list}", &element_list)
            .replace("{target}", target);

        let data_url = format!("data:image/png;base64,{annotated_b64}");
        call_vlm(state, ctx, &data_url, &vlm_prompt, true).await
    }

    /// Fallback: SoM grid overlay + VLM.
    async fn try_som_grid_fallback(
        &self,
        state: &mut SharedState,
        ctx: &NodeContext,
        image_bytes: &[u8],
        meta: &crate::perception::types::ScreenshotMeta,
        target: &str,
    ) -> Result<Option<String>, String> {
        tracing::info!("Using SoM grid fallback");
        state.detected_elements.clear();

        let grid_bytes =
            draw_som_grid(image_bytes, ctx.grid_n).unwrap_or_else(|_| image_bytes.to_vec());
        let grid_b64 = base64::engine::general_purpose::STANDARD.encode(&grid_bytes);

        let _ = ctx.app.emit("viewport_captured", serde_json::json!({
            "image_base64": &grid_b64,
            "grid_n": ctx.grid_n,
            "physical_width": meta.physical_width,
            "physical_height": meta.physical_height,
            "source": "som_grid",
        }));

        let last_col = col_label(ctx.grid_n - 1);
        let vlm_prompt = VLM_PROMPT_TEMPLATE
            .replace("{grid_n}", &ctx.grid_n.to_string())
            .replace("{last_col}", &last_col)
            .replace("{target}", target);

        let data_url = format!("data:image/png;base64,{grid_b64}");
        call_vlm(state, ctx, &data_url, &vlm_prompt, false).await
    }
}

/// Shared VLM call logic used by both annotated and grid modes.
async fn call_vlm(
    state: &SharedState,
    ctx: &NodeContext,
    data_url: &str,
    vlm_prompt: &str,
    is_annotated: bool,
) -> Result<Option<String>, String> {
    let vlm_messages = vec![ChatMessage {
        role: "user".into(),
        content: MessageContent::Parts(vec![
            ContentPart::ImageUrl {
                image_url: ImageUrl {
                    url: data_url.to_string(),
                },
            },
            ContentPart::Text {
                text: vlm_prompt.to_string(),
            },
        ]),
        tool_call_id: None,
        tool_calls: None,
    }];

    let (provider, mut cfg) = {
        let reg = ctx.registry.lock().await;
        reg.call_config_for_role("vision").map_err(|e| e.to_string())?
    };
    cfg.silent = true;

    let flag = state.stop_flag.clone();
    let response = tokio::select! {
        result = provider.chat(vlm_messages, vec![], &cfg, &ctx.app) => {
            result.map_err(|e| e.to_string())?
        }
        _ = poll_stop(flag) => {
            return Err("Stopped by user".into());
        }
        _ = tokio::time::sleep(std::time::Duration::from_secs(45)) => {
            return Err("VLM call timed out after 45s".into());
        }
    };

    if state.is_stopped() {
        return Err("Stopped by user".into());
    }

    tracing::debug!(vlm_raw = %response.content, "VLM response");

    let raw = response.content.trim();
    let json_str = raw
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    match serde_json::from_str::<serde_json::Value>(json_str) {
        Ok(v) => {
            let found = v["found"].as_bool().unwrap_or(false);
            if !found {
                return Ok(None);
            }
            if is_annotated {
                Ok(v["element_id"].as_str().map(|s| s.to_string()))
            } else {
                Ok(v["cell"].as_str().map(|s| s.to_string()))
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, raw = %json_str, "VLM JSON parse failed");
            Ok(extract_cell_label_from_text(raw))
        }
    }
}


