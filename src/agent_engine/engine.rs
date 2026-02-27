// Agent engine main loop — placeholder until Phase 3 full implementation.
use crate::agent_engine::state::{AgentState, LoopConfig};
use crate::errors::SeeClawResult;

pub struct AgentEngine {
    pub state: AgentState,
    pub loop_config: LoopConfig,
    pub failure_count: u32,
    pub loop_count: u32,
}

impl AgentEngine {
    pub fn new(loop_config: LoopConfig) -> Self {
        Self {
            state: AgentState::Idle,
            loop_config,
            failure_count: 0,
            loop_count: 0,
        }
    }

    pub async fn start(&mut self, _goal: String) -> SeeClawResult<()> {
        // Full implementation in Phase 3
        Ok(())
    }

    pub fn stop(&mut self) {
        self.state = AgentState::Idle;
    }
}
