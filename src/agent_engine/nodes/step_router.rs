//! StepRouterNode — decides the execution mode for the current TodoStep.
//!
//! The Planner only provides a `recommended_mode` hint. StepRouter makes the
//! final decision using a lightweight multi-signal approach:
//!
//! 1. **Skill trigger matching** — if description matches a skill's triggers,
//!    force Combo mode (zero LLM, fastest path).
//! 2. **Keyword heuristics** — regex patterns for chat-like vs vlm-like tasks.
//! 3. **Planner hint** — fall back to `recommended_mode` if no strong signal.
//!
//! This node also handles mode_switch_requested from loop agents.

use async_trait::async_trait;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{poll_stop, Node, NodeOutput};
use crate::agent_engine::state::{SharedState, StepMode, StepStatus};

pub struct StepRouterNode;

impl StepRouterNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for StepRouterNode {
    fn name(&self) -> &str {
        "step_router"
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
        if idx >= state.todo_steps.len() {
            // All steps done → go to verifier
            return Ok(NodeOutput::GoTo("verifier".to_string()));
        }

        // Check if a loop agent requested a mode switch
        if let Some(requested_mode) = state.mode_switch_requested.take() {
            tracing::info!(
                step = idx,
                mode = ?requested_mode,
                "StepRouterNode: mode switch requested by loop agent"
            );
            let step = &mut state.todo_steps[idx];
            step.mode = requested_mode.clone();
            state.current_loop_mode = requested_mode;
            // Don't reset step_complete — the loop agent signalled switch, not completion
            return Ok(NodeOutput::GoTo(mode_to_node(&state.current_loop_mode)));
        }

        // Fresh step entry — decide mode
        let step = &mut state.todo_steps[idx];
        step.status = StepStatus::InProgress;

        // Reset per-step state
        state.step_complete = false;
        state.last_exec_result.clear();
        state.step_messages.clear();
        state.step_iterations = 0;
        state.step_action_history.clear();

        tracing::info!(
            step = idx,
            recommended = ?step.recommended_mode,
            desc = %step.description,
            "[StepRouter] → routing step: '{}'",
            truncate(&step.description, 80)
        );

        // Emit step_started to frontend
        let _ = ctx.app.emit("step_started", serde_json::json!({
            "index": idx,
            "description": &step.description,
            "mode": &step.recommended_mode,
            "recommended_mode": &step.recommended_mode,
        }));

        // Inter-step delay (give OS time to process previous UI action)
        if idx > 0 {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {}
                _ = poll_stop(state.stop_flag.clone()) => return Ok(NodeOutput::End),
            }
        }

        // ── Decision logic ─────────────────────────────────────────────

        // Signal 1: If step has a combo skill, check if it exists in registry
        if step.recommended_mode == StepMode::Combo {
            if let Some(skill_name) = &step.skill {
                if ctx.skill_registry.has_combo(skill_name) {
                    let mode = StepMode::Combo;
                    step.mode = mode.clone();
                    state.current_loop_mode = mode;
                    tracing::info!(step = idx, skill = %skill_name, "[StepRouter] ✅ Combo skill confirmed → combo_exec");
                    return Ok(NodeOutput::GoTo("combo_exec".to_string()));
                } else {
                    tracing::warn!(
                        step = idx,
                        skill = %skill_name,
                        "[StepRouter] ⚠ combo skill '{}' not found, falling back to heuristics",
                        skill_name
                    );
                }
            }
        }

        // Signal 2: Skill trigger matching — ask registry if any skill matches
        let trigger_matches = ctx.skill_registry.match_triggers(&step.description);
        if let Some((matched_skill, _score)) = trigger_matches.first() {
            // Attempt to extract parameters from the step description
            let extracted_params = ctx.skill_registry.extract_params_from_description(
                matched_skill,
                &step.description,
            );
            // Only use combo if we actually got parameter values;
            // otherwise the placeholders will be sent literally.
            if !extracted_params.is_null()
                && extracted_params.as_object().map_or(false, |m| !m.is_empty())
            {
                tracing::info!(
                    step = idx,
                    skill = %matched_skill,
                    params = %extracted_params,
                    "[StepRouter] ✅ trigger matched '{}' with params → Combo mode",
                    matched_skill
                );
                step.skill = Some(matched_skill.clone());
                step.params = Some(extracted_params);
                step.mode = StepMode::Combo;
                state.current_loop_mode = StepMode::Combo;
                return Ok(NodeOutput::GoTo("combo_exec".to_string()));
            } else {
                tracing::warn!(
                    step = idx,
                    skill = %matched_skill,
                    "[StepRouter] ⚠ trigger matched '{}' but could not extract params — skipping combo",
                    matched_skill
                );
            }
        }

        // Signal 3: Keyword heuristics
        let desc_lower = step.description.to_lowercase();
        let mode = if is_chat_like(&desc_lower) {
            StepMode::Chat
        } else if is_vlm_like(&desc_lower) {
            StepMode::Vlm
        } else {
            // Signal 4: Fall back to Planner's recommendation
            step.recommended_mode.clone()
        };

        step.mode = mode.clone();
        state.current_loop_mode = mode.clone();

        tracing::info!(
            step = idx,
            mode = ?mode,
            signal = if is_chat_like(&desc_lower) { "chat_heuristic" } else if is_vlm_like(&desc_lower) { "vlm_heuristic" } else { "planner_hint" },
            "[StepRouter] ✅ decided → {:?} ({})",
            mode,
            mode_to_node(&mode)
        );

        Ok(NodeOutput::GoTo(mode_to_node(&mode)))
    }
}

/// Map StepMode to the corresponding graph node name.
fn mode_to_node(mode: &StepMode) -> String {
    match mode {
        StepMode::Combo => "combo_exec".to_string(),
        StepMode::Chat => "chat_agent".to_string(),
        StepMode::Vlm => "vlm_act".to_string(),
    }
}

/// Heuristic: does the description look like a chat/terminal task?
fn is_chat_like(desc: &str) -> bool {
    let patterns = [
        "terminal", "powershell", "cmd", "命令", "终端",
        "文件", "file", "创建", "删除", "create", "delete",
        "写入", "write", "保存", "save",
        "快捷键", "hotkey", "shortcut",
        "输入", "type", "键入",
        "脚本", "script", "执行", "execute", "run",
    ];
    patterns.iter().any(|p| desc.contains(p))
}

/// Heuristic: does the description look like a vision task?
fn is_vlm_like(desc: &str) -> bool {
    let patterns = [
        "点击", "click", "按钮", "button",
        "界面", "ui", "窗口", "window",
        "截图", "screenshot", "屏幕", "screen",
        "查看", "观察", "look", "find", "locate",
        "菜单", "menu", "图标", "icon",
        "确认", "verify", "检查", "check",
        "拖拽", "drag", "滚动", "scroll",
    ];
    patterns.iter().any(|p| desc.contains(p))
}

/// Truncate a string to `max` chars with "…" if longer (for log display).
fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > max {
        format!("{}…", chars[..max].iter().collect::<String>())
    } else {
        s.to_string()
    }
}
