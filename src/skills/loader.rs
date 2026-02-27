// Skills loader — full implementation in Phase 9.
use crate::errors::{SeeClawError, SeeClawResult};
use crate::skills::types::Skill;

pub async fn load_all() -> SeeClawResult<Vec<Skill>> {
    Err(SeeClawError::Skills("Skills loader not implemented yet (Phase 9)".to_string()))
}
