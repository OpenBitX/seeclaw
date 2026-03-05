//! StepEvaluateNode — inner loop control point.
//!
//! After each action execution (or after a loop agent signals completion),
//! this node decides:
//! 1. **Step complete** → mark done, go to step_advance.
//! 2. **Mode switch requested** → go back to step_router.
//! 3. **Max iterations exceeded** → force advance with failure.
//! 4. **Continue** → loop back to the current agent for another iteration.

use async_trait::async_trait;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{Node, NodeOutput};
use crate::agent_engine::state::{SharedState, StepMode, StepStatus};

/// Maximum iterations per step before forced advancement.
/// VLM is expensive (screenshot + LLM), so it gets a lower cap.
const MAX_VLM_ITERATIONS: u32 = 4;
const MAX_CHAT_ITERATIONS: u32 = 15;

pub struct StepEvaluateNode;

impl StepEvaluateNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for StepEvaluateNode {
    fn name(&self) -> &str {
        "step_evaluate"
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

        // Use unified step_iterations counter (incremented by both chat_agent and vlm_act)
        let step_iterations = state.step_iterations;
        let max_iters = match state.current_loop_mode {
            StepMode::Vlm => MAX_VLM_ITERATIONS,
            _ => MAX_CHAT_ITERATIONS,
        };

        tracing::info!(
            step = idx,
            step_complete = state.step_complete,
            mode_switch = ?state.mode_switch_requested,
            iterations = step_iterations,
            max = max_iters,
            mode = ?state.current_loop_mode,
            "[StepEvaluate] evaluating: complete={}, iters={}/{}, mode={:?}",
            state.step_complete, step_iterations, max_iters, state.current_loop_mode
        );

        // Case 1: Step marked complete by the loop agent
        if state.step_complete {
            tracing::info!(step = idx, iterations = step_iterations, "[StepEvaluate] ✅ step complete after {} iters → step_advance", step_iterations);
            let _ = ctx.app.emit("agent_activity", serde_json::json!({
                "text": format!("步骤 {} 完成", idx + 1)
            }));
            return Ok(NodeOutput::GoTo("step_advance".to_string()));
        }

        // Case 1b: Auto-completion heuristic for VLM mode.
        //
        // VLM steps should be atomic (one action per step). The VLM often
        // fails to call finish_step, causing infinite loops. Two-tier heuristic:
        //
        // Tier 1: Definitive GUI action (click) succeeded → auto-complete
        //         after 1 successful action (relaxed from previous threshold of 2).
        // Tier 2: Iteration fallback — if iter >= 2 and any action succeeded,
        //         the VLM already had a chance to call finish_step and didn't.
        //         Force auto-complete regardless of action type.
        if !state.step_complete && state.current_loop_mode == StepMode::Vlm {
            let successful_action_count = state.step_action_history.iter()
                .filter(|h| !h.contains("FAILED"))
                .count();

            // Tier 1: Definitive GUI action auto-complete
            let tier1 = if state.last_action_succeeded
                && is_definitive_gui_action(&state.last_action_kind)
            {
                let is_single_step_first_iter = state.todo_steps.len() <= 1 && step_iterations == 1;
                let step_desc = state.todo_steps.get(idx)
                    .map(|s| s.description.to_lowercase())
                    .unwrap_or_default();
                let is_simple_click_goal = is_simple_click_description(&step_desc);
                // VLM steps are atomic — 1 successful definitive action = done
                let vlm_atomic_complete = successful_action_count >= 1;
                is_single_step_first_iter || is_simple_click_goal || vlm_atomic_complete
            } else {
                false
            };

            // Tier 2: Iteration fallback — VLM had its chance to call finish_step
            let tier2 = step_iterations >= 2 && successful_action_count >= 1;

            if tier1 || tier2 {
                let tier_label = if tier1 { "definitive_action" } else { "iteration_fallback" };
                tracing::info!(
                    step = idx,
                    action = %state.last_action_kind,
                    iterations = step_iterations,
                    successful_actions = successful_action_count,
                    tier = tier_label,
                    "[StepEvaluate] ✅ auto-complete ({}) → step_advance", tier_label
                );
                state.step_complete = true;
                if let Some(step) = state.todo_steps.get_mut(idx) {
                    step.status = StepStatus::Completed;
                }
                state.steps_log.push(format!(
                    "Step {}: auto-completed after {} successful action(s) ({})",
                    idx + 1, successful_action_count, tier_label
                ));
                let _ = ctx.app.emit("agent_activity", serde_json::json!({
                    "text": format!("步骤 {} 完成（自动确认）", idx + 1)
                }));
                return Ok(NodeOutput::GoTo("step_advance".to_string()));
            }
        }

        // Case 2: Mode switch requested by loop agent
        if state.mode_switch_requested.is_some() {
            tracing::info!(step = idx, new_mode = ?state.mode_switch_requested, "[StepEvaluate] 🔄 mode switch requested → step_router");
            return Ok(NodeOutput::GoTo("step_router".to_string()));
        }

        // Case 3: Max iterations exceeded — force fail and advance
        if step_iterations >= max_iters {
            tracing::warn!(
                step = idx,
                iterations = step_iterations,
                max = max_iters,
                mode = ?state.current_loop_mode,
                "[StepEvaluate] ⚠ max iterations ({}/{}) exceeded for {:?} → force advance",
                step_iterations, max_iters, state.current_loop_mode
            );
            if let Some(step) = state.todo_steps.get_mut(idx) {
                step.status = StepStatus::Failed;
            }
            state.steps_log.push(format!(
                "Step {}: TIMEOUT — exceeded max iterations ({}/{})",
                idx + 1,
                step_iterations,
                max_iters
            ));
            let mut ctrl = ctx.loop_ctrl.lock().await;
            ctrl.record_failure();
            return Ok(NodeOutput::GoTo("step_advance".to_string()));
        }

        // Case 4: Continue the loop — route back to current agent
        let target = match state.current_loop_mode {
            StepMode::Combo => {
                // Combo should never loop — it executes atomically.
                // If we get here, the combo finished; advance.
                return Ok(NodeOutput::GoTo("step_advance".to_string()));
            }
            StepMode::Chat => "chat_agent",
            StepMode::Vlm => "vlm_act",
        };

        tracing::debug!(
            step = idx,
            target = target,
            iterations = step_iterations,
            "[StepEvaluate] continuing loop → {} (iter {}/{})",
            target, step_iterations, max_iters
        );

        Ok(NodeOutput::GoTo(target.to_string()))
    }
}

/// Returns true for GUI actions that are "definitive" — once executed
/// successfully, the step is likely complete for single-step plans.
fn is_definitive_gui_action(kind: &str) -> bool {
    matches!(
        kind,
        "mouse_click" | "mouse_double_click" | "mouse_right_click"
    )
}

/// Returns true if the step description looks like a simple click/open action
/// that should auto-complete after a single successful GUI action.
fn is_simple_click_description(desc: &str) -> bool {
    // Patterns: "找到X并点击", "点击X图标", "单击X", "左键单击X", "定位X"
    let click_words = ["点击", "单击", "click", "左键", "双击", "右键"];
    let simple_words = [
        "找到", "打开", "启动", "open", "launch", "find",
        "定位", "locate", "选择", "选中", "select",
        "查找", "寻找", "识别", "获取", "切换到",
    ];
    let has_click = click_words.iter().any(|w| desc.contains(w));
    let has_simple = simple_words.iter().any(|w| desc.contains(w));
    // Must mention clicking or a simple action AND not be a complex multi-part description
    (has_click || has_simple) && !desc.contains("然后") && !desc.contains("接着") && !desc.contains("并且")
}
