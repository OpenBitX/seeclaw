// Tool call dispatcher — full implementation in Phase 5.
use crate::agent_engine::state::AgentAction;
use crate::errors::{SeeClawError, SeeClawResult};

pub async fn dispatch(_action: &AgentAction) -> SeeClawResult<()> {
    Err(SeeClawError::Executor("Dispatcher not implemented yet (Phase 5)".to_string()))
}
