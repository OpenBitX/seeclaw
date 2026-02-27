// Session history JSONL persistence — placeholder until Phase 3 full implementation.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub ts: i64,
    pub role: String,
    pub content: Option<String>,
    pub action: Option<serde_json::Value>,
}

pub struct SessionHistory {
    pub session_id: String,
    entries: Vec<HistoryEntry>,
}

impl SessionHistory {
    pub fn new() -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, entry: HistoryEntry) {
        self.entries.push(entry);
    }
}

impl Default for SessionHistory {
    fn default() -> Self {
        Self::new()
    }
}
