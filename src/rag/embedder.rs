// Text embedder for RAG — full implementation in Phase 9.
use crate::errors::{SeeClawError, SeeClawResult};

pub async fn embed(_text: &str) -> SeeClawResult<Vec<f32>> {
    Err(SeeClawError::Rag("Embedder not implemented yet (Phase 9)".to_string()))
}
