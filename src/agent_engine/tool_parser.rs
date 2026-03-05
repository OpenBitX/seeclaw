//! Tool call parser — converts LLM tool calls into `AgentAction` / `TodoStep`.
//!
//! Extracted from the old `engine.rs` to keep parsing logic isolated and
//! reusable across multiple nodes (PlannerNode, DirectExecNode, VlmActNode).

use crate::agent_engine::state::{
    AgentAction, StepMode, StepStatus, TodoStep,
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
///
/// New format from Planner:
/// ```json
/// {
///   "final_goal": "...",
///   "plan_summary": "...",
///   "steps": [
///     {
///       "description": "...",
///       "recommended_mode": "combo|chat|vlm",
///       "required_skills": ["skill_name"],
///       "guidance": "optional hint for the loop agent",
///       "skill": "skill_name (for combo mode)",
///       "params": { ... }
///     }
///   ]
/// }
/// ```
fn parse_plan_task(args: &serde_json::Value) -> Result<AgentAction, String> {
    let final_goal = args["final_goal"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let plan_summary = args["plan_summary"]
        .as_str()
        .unwrap_or("")
        .to_string();

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
        // Parse recommended_mode
        let recommended_mode = match s["recommended_mode"].as_str() {
            Some("combo") => StepMode::Combo,
            Some("vlm") => StepMode::Vlm,
            _ => StepMode::Chat, // default to Chat
        };

        // Parse required_skills
        let required_skills = s["required_skills"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Parse skill + params for combo mode
        let skill = s["skill"].as_str().map(|t| t.to_string());
        let params = s.get("params").cloned();

        // Parse guidance
        let guidance = s["guidance"].as_str().map(|g| g.to_string());

        steps.push(TodoStep {
            index: i,
            description: s["description"].as_str().unwrap_or("").to_string(),
            recommended_mode: recommended_mode.clone(),
            mode: recommended_mode, // StepRouter may override at runtime
            required_skills,
            guidance,
            skill,
            params,
            status: StepStatus::Pending,
        });
    }

    Ok(AgentAction::PlanTask {
        final_goal,
        plan_summary,
        steps,
    })
}

/// Helper to extract a string field with empty-string default.
fn str_field(args: &serde_json::Value, key: &str) -> String {
    args[key].as_str().unwrap_or("").to_string()
}
