//! ActionExecNode — executes the current AgentAction (mouse, keyboard, terminal, etc.).
//!
//! This is the central executor node. It delegates to `executor::input` for
//! physical I/O and handles FinishTask / ReportFailure as terminal states.

use async_trait::async_trait;
use base64::Engine as _;
use tauri::Emitter;
use tokio::process::Command;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::history::HistoryEntry;
use crate::agent_engine::node::{poll_stop, Node, NodeOutput};
use crate::agent_engine::state::{AgentAction, GraphResult, SharedState};
use crate::agent_engine::tool_parser::{is_auto_approved, needs_stability_wait, parse_action_by_name};
use crate::executor::input;
use crate::llm::types::{ChatMessage, MessageContent, StreamChunk, StreamChunkKind};
use crate::perception::screenshot::capture_primary;
use crate::perception::som_grid::{col_label, draw_som_grid, grid_cell_to_physical, parse_grid_label};

pub struct ActionExecNode;

impl ActionExecNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for ActionExecNode {
    fn name(&self) -> &str {
        "action_exec"
    }

    async fn execute(
        &self,
        state: &mut SharedState,
        ctx: &NodeContext,
    ) -> Result<NodeOutput, String> {
        if state.is_stopped() {
            return Ok(NodeOutput::End);
        }

        let action = match state.current_action.take() {
            Some(a) => a,
            None => {
                // This should not happen in normal flow (direct_exec now falls back to planner).
                // Guard here so a logic bug silently skips rather than crashing the whole graph.
                tracing::warn!("ActionExecNode: no current_action set — skipping to step_advance");
                return Ok(NodeOutput::GoTo("step_advance".to_string()));
            }
        };

        // Safety check: route to user_confirm only if the action is not
        // auto-approved AND the user hasn't already approved it this round.
        // `action_user_approved` is set by UserConfirmNode after approval and
        // cleared here, preventing an infinite user_confirm ↔ action_exec loop.
        if !is_auto_approved(&action) && !state.action_user_approved {
            state.needs_approval = true;
            state.current_action = Some(action);
            return Ok(NodeOutput::GoTo("user_confirm".to_string()));
        }
        // Consume the approval flag so the next action goes through approval again.
        state.action_user_approved = false;

        // Emit activity
        let activity_label = action_activity_label(&action);
        let _ = ctx.app.emit("agent_activity", serde_json::json!({ "text": activity_label }));

        tracing::info!(?action, step = state.current_step_idx, "ActionExecNode: executing");

        let (ok, msg) = execute_action_impl(&action, state, ctx).await;

        // Handle terminal actions
        match &action {
            AgentAction::FinishTask { summary } => {
                let _ = ctx.app.emit("llm_stream_chunk", &StreamChunk {
                    kind: StreamChunkKind::Content,
                    content: summary.clone(),
                });
                let _ = ctx.app.emit("llm_stream_chunk", &StreamChunk {
                    kind: StreamChunkKind::Done,
                    content: String::new(),
                });
                state.result = Some(GraphResult::Done { summary: summary.clone() });
                return Ok(NodeOutput::End);
            }
            AgentAction::ReportFailure { reason, .. } => {
                let _ = ctx.app.emit("llm_stream_chunk", &StreamChunk {
                    kind: StreamChunkKind::Content,
                    content: format!("Task failed: {reason}"),
                });
                let _ = ctx.app.emit("llm_stream_chunk", &StreamChunk {
                    kind: StreamChunkKind::Done,
                    content: String::new(),
                });
                state.result = Some(GraphResult::Error { message: reason.clone() });
                return Ok(NodeOutput::End);
            }
            AgentAction::GetViewport { .. } => {
                // GetViewport: capture screenshot and inject into conversation, then re-plan
                return self.handle_get_viewport(state, ctx).await;
            }
            _ => {}
        }

        // Push tool result to conversation
        state.conv_messages.push(ChatMessage {
            role: "tool".into(),
            content: MessageContent::Text(msg.clone()),
            tool_call_id: Some(state.pending_tool_id.clone()),
            tool_calls: None,
        });

        // Record in history
        {
            let mut history = ctx.history.lock().await;
            history.push(HistoryEntry {
                ts: chrono::Utc::now().timestamp_millis(),
                role: "tool".into(),
                content: None,
                action: Some(serde_json::to_value(&action).unwrap_or_default()),
            });
            let _ = history.flush();
        }

        if !ok {
            let mut ctrl = ctx.loop_ctrl.lock().await;
            ctrl.record_failure();
        }

        // Log step result
        let step_desc = state
            .todo_steps
            .get(state.current_step_idx)
            .map(|s| s.description.clone())
            .unwrap_or_else(|| format!("step {}", state.current_step_idx));
        state.steps_log.push(format!(
            "Step {}: {} - {}",
            state.current_step_idx + 1,
            step_desc,
            if ok { msg } else { format!("FAILED: {msg}") }
        ));

        // Determine if stability wait is needed
        state.needs_stability = needs_stability_wait(&action) && ok;

        Ok(NodeOutput::Continue)
    }
}

impl ActionExecNode {
    /// Handle GetViewport: capture screenshot, inject into conversation, go to planner.
    async fn handle_get_viewport(
        &self,
        state: &mut SharedState,
        ctx: &NodeContext,
    ) -> Result<NodeOutput, String> {
        tracing::warn!("get_viewport called directly — capturing and injecting into conversation");
        let shot = capture_primary().await.map_err(|e| e.to_string())?;
        state.last_meta = Some(shot.meta.clone());

        let (b64, source_desc) = {
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
                state.detected_elements = elements.clone();
                let annotated = crate::perception::annotator::annotate_image(&shot.image_bytes, &elements)
                    .unwrap_or(shot.image_bytes.clone());
                let b64 = base64::engine::general_purpose::STANDARD.encode(&annotated);
                let desc = format!(
                    "Screenshot captured with {} annotated UI elements.",
                    elements.len()
                );
                (b64, desc)
            } else {
                state.detected_elements.clear();
                let grid = draw_som_grid(&shot.image_bytes, ctx.grid_n)
                    .unwrap_or(shot.image_bytes.clone());
                let b64 = base64::engine::general_purpose::STANDARD.encode(&grid);
                let last_col = col_label(ctx.grid_n - 1);
                let desc = format!(
                    "Screenshot captured. Grid: {n}x{n}, columns A-{last}.",
                    n = ctx.grid_n, last = last_col,
                );
                (b64, desc)
            }
        };

        let data_url = format!("data:image/png;base64,{b64}");
        state.conv_messages.push(ChatMessage {
            role: "tool".into(),
            content: MessageContent::Text(source_desc),
            tool_call_id: Some(state.pending_tool_id.clone()),
            tool_calls: None,
        });
        state.conv_messages.push(ChatMessage {
            role: "user".into(),
            content: MessageContent::Parts(vec![
                crate::llm::types::ContentPart::ImageUrl {
                    image_url: crate::llm::types::ImageUrl { url: data_url },
                },
                crate::llm::types::ContentPart::Text {
                    text: format!(
                        "This is the current screen. Now call plan_task to accomplish: {}",
                        state.goal
                    ),
                },
            ]),
            tool_call_id: None,
            tool_calls: None,
        });

        Ok(NodeOutput::GoTo("planner".to_string()))
    }
}

/// Execute the actual I/O for an action.
async fn execute_action_impl(
    action: &AgentAction,
    state: &SharedState,
    ctx: &NodeContext,
) -> (bool, String) {
    match action {
        AgentAction::MouseClick { element_id }
        | AgentAction::MouseDoubleClick { element_id }
        | AgentAction::MouseRightClick { element_id } => {
            let is_double = matches!(action, AgentAction::MouseDoubleClick { .. });
            let is_right = matches!(action, AgentAction::MouseRightClick { .. });
            if let Some(meta) = &state.last_meta {
                let coords = state
                    .detected_elements
                    .iter()
                    .find(|e| e.id == *element_id)
                    .map(|elem| elem.center_physical(meta));
                let coords = coords.or_else(|| {
                    parse_grid_label(element_id).map(|(col, row)| {
                        grid_cell_to_physical(
                            col,
                            row,
                            meta.physical_width,
                            meta.physical_height,
                            ctx.grid_n,
                        )
                    })
                });

                if let Some((px, py)) = coords {
                    let result = if is_right {
                        input::mouse_right_click(px, py).await
                    } else if is_double {
                        input::mouse_double_click(px, py).await
                    } else {
                        input::mouse_click(px, py).await
                    };
                    match result {
                        Ok(()) => (true, format!("Clicked {element_id} at ({px},{py})")),
                        Err(e) => (false, format!("Click failed: {e}")),
                    }
                } else {
                    (false, format!("Cannot resolve element: {element_id}"))
                }
            } else {
                (false, "No viewport — call get_viewport first".into())
            }
        }
        AgentAction::TypeText { text, clear_first } => {
            match input::type_text(text.clone(), *clear_first).await {
                Ok(()) => (true, format!("Typed: {text}")),
                Err(e) => (false, format!("TypeText failed: {e}")),
            }
        }
        AgentAction::Hotkey { keys } => match input::press_hotkey(keys.clone()).await {
            Ok(()) => (true, format!("Hotkey: {keys}")),
            Err(e) => (false, format!("Hotkey failed: {e}")),
        },
        AgentAction::KeyPress { key } => match input::press_hotkey(key.clone()).await {
            Ok(()) => (true, format!("KeyPress: {key}")),
            Err(e) => (false, format!("KeyPress failed: {e}")),
        },
        AgentAction::Wait { milliseconds } => {
            let flag = state.stop_flag.clone();
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_millis(*milliseconds as u64)) => {}
                _ = poll_stop(flag) => {
                    return (false, "Stopped by user".into());
                }
            }
            (true, format!("Waited {milliseconds}ms"))
        }
        AgentAction::ExecuteTerminal { command, reason } => {
            tracing::info!(%command, %reason, "executing terminal command");
            match Command::new("powershell")
                .arg("-NoProfile")
                .arg("-Command")
                .arg(command)
                .kill_on_drop(true)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
            {
                Ok(child) => {
                    let flag = state.stop_flag.clone();
                    let output = tokio::select! {
                        result = child.wait_with_output() => result,
                        _ = poll_stop(flag) => {
                            return (false, "Stopped by user".into());
                        }
                    };
                    match output {
                        Ok(out) => {
                            let mut buf = String::new();
                            if !out.stdout.is_empty() {
                                buf.push_str(&String::from_utf8_lossy(&out.stdout));
                            }
                            if !out.stderr.is_empty() {
                                if !buf.is_empty() {
                                    buf.push_str("\n--- STDERR ---\n");
                                }
                                buf.push_str(&String::from_utf8_lossy(&out.stderr));
                            }
                            let truncated = if buf.len() > 4000 {
                                format!("{}\n[truncated]", &buf[..4000])
                            } else {
                                buf
                            };
                            let ok = out.status.success();
                            (ok, format!("command: {command}\noutput:\n{truncated}"))
                        }
                        Err(e) => (false, format!("wait failed: {e}")),
                    }
                }
                Err(e) => (false, format!("spawn failed: {e}")),
            }
        }
        AgentAction::Scroll { direction, distance, element_id: _ } => {
            // Scroll is auto-approved; here we just handle the basic case
            (true, format!("Scrolled {direction} ({distance})"))
        }
        AgentAction::InvokeSkill { skill_name, inputs } => {
            // Fallback: if invoke_skill reaches action_exec (LLM used invoke_skill
            // instead of combo mode), expand the combo here and execute inline.
            tracing::info!(
                skill = %skill_name,
                "ActionExecNode: expanding invoke_skill as inline combo"
            );
            match ctx.skill_registry.expand_combo(skill_name, inputs) {
                Some(combo_steps) => {
                    let total = combo_steps.len();
                    for (i, combo_step) in combo_steps.iter().enumerate() {
                        if state.is_stopped() {
                            return (false, "Stopped by user".into());
                        }
                        let sub_action = match parse_action_by_name(&combo_step.action, &combo_step.args) {
                            Ok(a) => a,
                            Err(e) => {
                                tracing::warn!(combo_step = i, error = %e, "invoke_skill: failed to parse combo step — skipping");
                                continue;
                            }
                        };
                        match &sub_action {
                            AgentAction::Wait { milliseconds } => {
                                let flag = state.stop_flag.clone();
                                let ms = *milliseconds;
                                tokio::select! {
                                    _ = tokio::time::sleep(std::time::Duration::from_millis(ms as u64)) => {}
                                    _ = poll_stop(flag) => return (false, "Stopped by user".into()),
                                }
                            }
                            AgentAction::Hotkey { keys } => {
                                if let Err(e) = input::press_hotkey(keys.clone()).await {
                                    tracing::warn!(error = %e, "invoke_skill: hotkey failed");
                                }
                            }
                            AgentAction::KeyPress { key } => {
                                if let Err(e) = input::press_hotkey(key.clone()).await {
                                    tracing::warn!(error = %e, "invoke_skill: key_press failed");
                                }
                            }
                            AgentAction::TypeText { text, clear_first } => {
                                if *clear_first {
                                    let _ = input::press_hotkey("ctrl+a".to_string()).await;
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                }
                                if let Err(e) = input::type_text(text.clone(), *clear_first).await {
                                    tracing::warn!(error = %e, "invoke_skill: type_text failed");
                                }
                            }
                            other => {
                                tracing::warn!(action = ?other, "invoke_skill: unsupported action in combo — skipping");
                            }
                        }
                    }
                    (true, format!("Skill '{}' executed ({} combo steps)", skill_name, total))
                }
                None => {
                    tracing::warn!(skill = %skill_name, "invoke_skill: no combo found in registry");
                    (false, format!("Skill '{}' not found in registry", skill_name))
                }
            }
        }
        AgentAction::FinishTask { .. } | AgentAction::ReportFailure { .. } => {
            // Handled above in the node logic
            (true, String::new())
        }
        AgentAction::GetViewport { .. } => {
            // Handled above
            (true, String::new())
        }
        other => {
            tracing::warn!(?other, "action not yet implemented");
            (false, "Not implemented".into())
        }
    }
}

fn action_activity_label(action: &AgentAction) -> String {
    match action {
        AgentAction::MouseClick { element_id } => format!("正在点击 {element_id}…"),
        AgentAction::MouseDoubleClick { element_id } => format!("正在双击 {element_id}…"),
        AgentAction::MouseRightClick { element_id } => format!("正在右键点击 {element_id}…"),
        AgentAction::TypeText { text, .. } => {
            let preview: String = text.chars().take(20).collect();
            format!("正在输入: {preview}…")
        }
        AgentAction::Hotkey { keys } => format!("正在按下快捷键: {keys}"),
        AgentAction::KeyPress { key } => format!("正在按键: {key}"),
        AgentAction::Wait { milliseconds } => format!("等待 {milliseconds}ms…"),
        AgentAction::ExecuteTerminal { command, .. } => {
            let preview: String = command.chars().take(30).collect();
            format!("正在执行命令: {preview}…")
        }
        AgentAction::Scroll { direction, .. } => format!("正在滚动({direction})…"),
        AgentAction::InvokeSkill { skill_name, .. } => format!("正在执行技能: {skill_name}…"),
        AgentAction::FinishTask { .. } => "正在完成任务…".to_string(),
        AgentAction::ReportFailure { .. } => "正在报告结果…".to_string(),
        _ => "正在执行操作…".to_string(),
    }
}


