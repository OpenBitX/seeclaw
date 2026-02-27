// Vector index for RAG — full implementation in Phase 9.
use crate::errors::{SeeClawError, SeeClawResult};

pub struct RagIndex;

impl RagIndex {
    pub fn new() -> Self {
        Self
    }

    pub async fn search(&self, _query_vec: &[f32], _top_k: usize) -> SeeClawResult<Vec<String>> {
        Err(SeeClawError::Rag("RAG index not implemented yet (Phase 9)".to_string()))
    }

    pub async fn insert(&self, _id: &str, _vec: &[f32], _text: &str) -> SeeClawResult<()> {
        Err(SeeClawError::Rag("RAG index not implemented yet (Phase 9)".to_string()))
    }
}

impl Default for RagIndex {
    fn default() -> Self {
        Self::new()
    }
}
