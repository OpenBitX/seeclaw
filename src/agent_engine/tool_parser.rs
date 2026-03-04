//! Tool call parser — converts LLM tool calls into `AgentAction` / `TodoStep`.
//!
//! Extracted from the old `engine.rs` to keep parsing logic isolated and
//! reusable across multiple nodes (PlannerNode, DirectExecNode, VlmActNode).

use crate::agent_engine::state::{
    AgentAction, StepMode, StepStatus, TodoStep, ToolCallData,
};
use crate::llm::types::ToolCall;

// ── Public API ─────────────────────────────────────────────────────────────

/// Parse an LLM `ToolCall` into an `AgentAction`.
///
/// Special handling for `plan_task` which produces a `PlanTask` containing
/// a list of `TodoStep`s.
pub fn parse_tool_call_to_action(tc: &ToolCall) -> Result<AgentAction, String> {
    let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, raw = %tc.function.arguments, "tool args JSON parse failed, using {{}}");
            serde_json::json!({})
        });

    match tc.function.name.as_str() {
        "plan_task" => parse_plan_task(&args),
        other => parse_action_by_name(other, &args),
    }
}

/// Convert a tool name + arguments JSON into an `AgentAction`.
pub fn parse_action_by_name(name: &str, args: &serde_json::Value) -> Result<AgentAction, String> {
    match name {
        "mouse_click" => Ok(AgentAction::MouseClick {
            element_id: str_field(args, "element_id"),
        }),
        "mouse_double_click" => Ok(AgentAction::MouseDoubleClick {
            element_id: str_field(args, "element_id"),
        }),
        "mouse_right_click" => Ok(AgentAction::MouseRightClick {
            element_id: str_field(args, "element_id"),
        }),
        "scroll" => Ok(AgentAction::Scroll {
            direction: args["direction"].as_str().unwrap_or("down").to_string(),
            distance: args["distance"].as_str().unwrap_or("short").to_string(),
            element_id: args["element_id"].as_str().map(|s| s.to_string()),
        }),
        "type_text" => Ok(AgentAction::TypeText {
            text: str_field(args, "text"),
            clear_first: args["clear_first"].as_bool().unwrap_or(false),
        }),
        "hotkey" => Ok(AgentAction::Hotkey {
            keys: str_field(args, "keys"),
        }),
        "key_press" => Ok(AgentAction::KeyPress {
            key: str_field(args, "key"),
        }),
        "get_viewport" => Ok(AgentAction::GetViewport {
            annotate: args["annotate"].as_bool().unwrap_or(true),
        }),
        "execute_terminal" => Ok(AgentAction::ExecuteTerminal {
            command: str_field(args, "command"),
            reason: str_field(args, "reason"),
        }),
        "mcp_call" => Ok(AgentAction::McpCall {
            server_name: str_field(args, "server_name"),
            tool_name: str_field(args, "tool_name"),
            arguments: args["arguments"].clone(),
        }),
        "invoke_skill" => Ok(AgentAction::InvokeSkill {
            skill_name: str_field(args, "skill_name"),
            inputs: args["inputs"].clone(),
        }),
        "wait" => Ok(AgentAction::Wait {
            milliseconds: args["milliseconds"].as_u64().unwrap_or(1000) as u32,
        }),
        "finish_task" => Ok(AgentAction::FinishTask {
            summary: str_field(args, "summary"),
        }),
        "report_failure" => Ok(AgentAction::ReportFailure {
            reason: str_field(args, "reason"),
            last_attempted_action: args["last_attempted_action"]
                .as_str()
                .map(|s| s.to_string()),
        }),
        other => Err(format!("unknown tool: {other}")),
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Check if the action type supports an `element_id` field (for VLM patching).
pub fn action_supports_element_id(action: &AgentAction) -> bool {
    matches!(
        action,
        AgentAction::MouseClick { .. }
            | AgentAction::MouseDoubleClick { .. }
            | AgentAction::MouseRightClick { .. }
            | AgentAction::Scroll { .. }
    )
}

/// Patch the `element_id` field in an action with a new value (from VLM).
pub fn patch_element_id(action: AgentAction, cell: &str) -> AgentAction {
    match action {
        AgentAction::MouseClick { .. } => AgentAction::MouseClick {
            element_id: cell.to_string(),
        },
        AgentAction::MouseDoubleClick { .. } => AgentAction::MouseDoubleClick {
            element_id: cell.to_string(),
        },
        AgentAction::MouseRightClick { .. } => AgentAction::MouseRightClick {
            element_id: cell.to_string(),
        },
        AgentAction::Scroll {
            direction, distance, ..
        } => AgentAction::Scroll {
            direction,
            distance,
            element_id: Some(cell.to_string()),
        },
        other => other,
    }
}

/// Safety check: actions that don't need user approval.
pub fn is_auto_approved(action: &AgentAction) -> bool {
    matches!(
        action,
        AgentAction::GetViewport { .. }
            | AgentAction::Wait { .. }
            | AgentAction::FinishTask { .. }
            | AgentAction::ReportFailure { .. }
            | AgentAction::MouseClick { .. }
            | AgentAction::MouseDoubleClick { .. }
            | AgentAction::MouseRightClick { .. }
            | AgentAction::TypeText { .. }
            | AgentAction::Hotkey { .. }
            | AgentAction::KeyPress { .. }
            | AgentAction::Scroll { .. }
            | AgentAction::InvokeSkill { .. }
    )
}

/// Check if an action typically triggers UI changes that need stability wait.
pub fn needs_stability_wait(action: &AgentAction) -> bool {
    matches!(
        action,
        AgentAction::MouseClick { .. }
            | AgentAction::MouseDoubleClick { .. }
            | AgentAction::MouseRightClick { .. }
            | AgentAction::TypeText { .. }
            | AgentAction::Hotkey { .. }
            | AgentAction::KeyPress { .. }
            | AgentAction::Scroll { .. }
    )
}

/// Try to extract a grid cell label (e.g. "B3") from free-text VLM output.
pub fn extract_cell_label_from_text(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"\b([A-L]{1,2})(\d{1,2})\b").ok()?;
    re.captures(text).map(|c| c[0].to_string())
}

// ── Internal ───────────────────────────────────────────────────────────────

/// Parse `plan_task` arguments into `AgentAction::PlanTask`.
fn parse_plan_task(args: &serde_json::Value) -> Result<AgentAction, String> {
    // Tolerate steps being a JSON string instead of an array
    let steps_val = &args["steps"];
    let raw_steps: Vec<serde_json::Value> = if let Some(arr) = steps_val.as_array() {
        arr.clone()
    } else if let Some(s) = steps_val.as_str() {
        serde_json::from_str(s).unwrap_or_default()
    } else {
        tracing::warn!("plan_task: steps field missing or wrong type, using empty list");
        vec![]
    };

    let mut steps = Vec::new();
    for (i, s) in raw_steps.iter().enumerate() {
        // Determine step mode: new field "mode" or legacy "needs_viewport" fallback
        let mode = match s["mode"].as_str() {
            Some("combo") => StepMode::Combo,
            Some("visual_locate") => StepMode::VisualLocate,
            Some("visual_act") => StepMode::VisualAct,
            Some("direct") => StepMode::Direct,
            _ => {
                // Legacy fallback: needs_viewport maps to VisualLocate
                if s["needs_viewport"].as_bool().unwrap_or(false) {
                    StepMode::VisualLocate
                } else {
                    StepMode::Direct
                }
            }
        };

        // Parse tool_calls for Direct mode
        let tool_calls = if mode == StepMode::Direct {
            parse_step_tool_calls(s, i)
        } else {
            Vec::new()
        };

        // Parse skill + params for Combo mode
        let skill = s["skill"].as_str().map(|t| t.to_string());
        let params = if mode == StepMode::Combo {
            s.get("params").cloned()
        } else {
            None
        };

        // Parse action_template for VisualLocate mode
        let action_template = if mode == StepMode::VisualLocate {
            let action_type = s["action_type"]
                .as_str()
                .or_else(|| s["action"]["type"].as_str())
                .unwrap_or("mouse_click");
            let mut step_args = s.clone();
            if step_args.is_object() {
                step_args["element_id"] = serde_json::json!("");
            }
            parse_action_by_name(action_type, &step_args).ok()
        } else {
            None
        };

        steps.push(TodoStep {
            index: i,
            description: s["description"].as_str().unwrap_or("").to_string(),
            mode,
            skill,
            params,
            tool_calls,
            target: s["target"].as_str().map(|t| t.to_string()),
            action_template,
            vlm_goal: s["vlm_goal"].as_str().map(|g| g.to_string()),
            status: StepStatus::Pending,
        });
    }

    Ok(AgentAction::PlanTask { steps })
}

/// Parse the tool_calls array from a step, or build one from legacy action_type field.
fn parse_step_tool_calls(step: &serde_json::Value, idx: usize) -> Vec<ToolCallData> {
    // New format: explicit tool_calls array
    if let Some(tcs) = step["tool_calls"].as_array() {
        return tcs
            .iter()
            .filter_map(|tc| {
                let name = tc["name"].as_str()?.to_string();
                let arguments = tc.get("arguments").cloned().unwrap_or(serde_json::json!({}));
                Some(ToolCallData { name, arguments })
            })
            .collect();
    }

    // Legacy format: single action_type field
    let action_type = step["action_type"]
        .as_str()
        .or_else(|| step["action"]["type"].as_str());

    if let Some(name) = action_type {
        vec![ToolCallData {
            name: name.to_string(),
            arguments: step.clone(),
        }]
    } else {
        tracing::warn!(step = idx, "no tool_calls or action_type in step, defaulting to wait");
        vec![ToolCallData {
            name: "wait".to_string(),
            arguments: serde_json::json!({ "milliseconds": 500 }),
        }]
    }
}

/// Helper to extract a string field with empty-string default.
fn str_field(args: &serde_json::Value, key: &str) -> String {
    args[key].as_str().unwrap_or("").to_string()
}
