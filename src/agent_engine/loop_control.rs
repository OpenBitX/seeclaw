// Loop control engine — placeholder until Phase 3 full implementation.
use crate::agent_engine::state::LoopConfig;

pub struct LoopController {
    config: LoopConfig,
    start_time: std::time::Instant,
    failure_count: u32,
}

impl LoopController {
    pub fn new(config: LoopConfig) -> Self {
        Self {
            config,
            start_time: std::time::Instant::now(),
            failure_count: 0,
        }
    }

    pub fn record_failure(&mut self) {
        self.failure_count += 1;
    }

    pub fn should_stop(&self) -> bool {
        use crate::agent_engine::state::LoopMode;
        match &self.config.mode {
            LoopMode::UntilDone => false,
            LoopMode::Timed => {
                if let Some(max_min) = self.config.max_duration_minutes {
                    self.start_time.elapsed().as_secs() / 60 >= max_min as u64
                } else {
                    false
                }
            }
            LoopMode::FailureLimit => {
                if let Some(max_fail) = self.config.max_failures {
                    self.failure_count >= max_fail
                } else {
                    false
                }
            }
        }
    }
}
