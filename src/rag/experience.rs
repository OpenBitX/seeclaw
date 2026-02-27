// Experience document writer — full implementation in Phase 9.
use crate::errors::{SeeClawError, SeeClawResult};

pub async fn append_experience(_title: &str, _content: &str) -> SeeClawResult<()> {
    Err(SeeClawError::Rag("Experience writer not implemented yet (Phase 9)".to_string()))
}
