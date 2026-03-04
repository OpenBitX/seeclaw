//! ComboExecNode — executes a pre-defined skill combo sequence in one graph step.
//!
//! When the Planner assigns `mode: "combo"` to a step, this node:
//! 1. Looks up the combo definition from the SkillRegistry.
//! 2. Expands parameter placeholders with actual values.
//! 3. Converts each combo step into an `AgentAction`.
//! 4. Executes ALL actions sequentially in a single node invocation.
//!
//! **Zero LLM calls** — this is the fastest execution path.
//! If the combo is not found, the node falls back to `vlm_act`.

use async_trait::async_trait;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{poll_stop, Node, NodeOutput};
use crate::agent_engine::state::{AgentAction, SharedState};
use crate::agent_engine::tool_parser::parse_action_by_name;
use crate::executor::input;

pub struct ComboExecNode;

impl ComboExecNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for ComboExecNode {
    fn name(&self) -> &str {
        "combo_exec"
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
            .ok_or_else(|| format!("ComboExecNode: no step at index {idx}"))?
            .clone();

        let skill_name = match &step.skill {
            Some(name) => name.clone(),
            None => {
                tracing::warn!(step = idx, "ComboExecNode: no skill specified — fallback to vlm_act");
                // Inject description as vlm_goal for the fallback
                if let Some(s) = state.todo_steps.get_mut(idx) {
                    if s.vlm_goal.is_none() {
                        s.vlm_goal = Some(s.description.clone());
                    }
                }
                return Ok(NodeOutput::GoTo("vlm_act".to_string()));
            }
        };

        let params = step.params.clone().unwrap_or(serde_json::json!({}));

        tracing::info!(
            step = idx,
            skill = %skill_name,
            "ComboExecNode: expanding combo"
        );

        // Look up and expand the combo
        let combo_steps = match ctx.skill_registry.expand_combo(&skill_name, &params) {
            Some(steps) => steps,
            None => {
                tracing::warn!(
                    skill = %skill_name,
                    "ComboExecNode: no combo found — fallback to vlm_act"
                );
                if let Some(s) = state.todo_steps.get_mut(idx) {
                    if s.vlm_goal.is_none() {
                        s.vlm_goal = Some(s.description.clone());
                    }
                }
                return Ok(NodeOutput::GoTo("vlm_act".to_string()));
            }
        };

        let _ = ctx.app.emit(
            "agent_activity",
            serde_json::json!({
                "text": format!("执行技能组合: {} ({} 步)", skill_name, combo_steps.len())
            }),
        );

        // Execute each action in the combo sequence
        for (i, combo_step) in combo_steps.iter().enumerate() {
            if state.is_stopped() {
                return Ok(NodeOutput::End);
            }

            let action = match parse_action_by_name(&combo_step.action, &combo_step.args) {
                Ok(a) => a,
                Err(e) => {
                    tracing::warn!(
                        step = idx,
                        combo_step = i,
                        error = %e,
                        "ComboExecNode: failed to parse combo action — skipping"
                    );
                    continue;
                }
            };

            tracing::debug!(
                step = idx,
                combo_step = i,
                action = ?action,
                "ComboExecNode: executing combo action"
            );

            // Execute the action
            match &action {
                AgentAction::Wait { milliseconds } => {
                    let flag = state.stop_flag.clone();
                    let ms = *milliseconds;
                    tokio::select! {
                        _ = tokio::time::sleep(std::time::Duration::from_millis(ms as u64)) => {}
                        _ = poll_stop(flag) => return Ok(NodeOutput::End),
                    }
                }
                AgentAction::Hotkey { keys } => {
                    if let Err(e) = input::press_hotkey(keys.clone()).await {
                        tracing::warn!(error = %e, "ComboExecNode: hotkey failed");
                    }
                }
                AgentAction::KeyPress { key } => {
                    if let Err(e) = input::press_hotkey(key.clone()).await {
                        tracing::warn!(error = %e, "ComboExecNode: key_press failed");
                    }
                }
                AgentAction::TypeText { text, clear_first } => {
                    if *clear_first {
                        let _ = input::press_hotkey("ctrl+a".to_string()).await;
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                    if let Err(e) = input::type_text(text.clone(), *clear_first).await {
                        tracing::warn!(error = %e, "ComboExecNode: type_text failed");
                    }
                }
                AgentAction::MouseClick { element_id } => {
                    // For combo, element_id would need to be resolved — but combos
                    // shouldn't normally contain mouse clicks with element_ids.
                    tracing::debug!(element_id = %element_id, "ComboExecNode: mouse_click in combo");
                }
                other => {
                    tracing::warn!(action = ?other, "ComboExecNode: unsupported action in combo — skipping");
                }
            }
        }

        tracing::info!(
            step = idx,
            skill = %skill_name,
            "ComboExecNode: combo completed"
        );

        // Mark step log
        state.steps_log.push(format!(
            "Step {}: combo '{}' executed ({} actions)",
            idx,
            skill_name,
            combo_steps.len()
        ));

        // Move to step_advance (combo replaces the action_exec path)
        Ok(NodeOutput::GoTo("step_advance".to_string()))
    }
}
