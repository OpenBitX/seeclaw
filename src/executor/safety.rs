// Safety interceptor — full implementation in Phase 5.
use crate::agent_engine::state::AgentAction;

/// Returns true if this action requires human approval before execution.
pub fn requires_approval(action: &AgentAction, require_list: &[String]) -> bool {
    let action_name = match action {
        AgentAction::ExecuteTerminal { .. } => "execute_terminal",
        AgentAction::McpCall { .. } => "mcp_call",
        _ => return false,
    };
    require_list.iter().any(|r| r == action_name)
}
