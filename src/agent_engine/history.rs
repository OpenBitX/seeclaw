use serde::{Deserialize, Serialize};
use std::io::Write;

use crate::errors::SeeClawResult;

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
    file_path: std::path::PathBuf,
}

impl SessionHistory {
    pub fn new() -> Self {
        let session_id = uuid::Uuid::new_v4().to_string();
        let dir = data_dir_or_cwd();
        let file_path = dir.join(format!("session_{session_id}.jsonl"));
        Self {
            session_id,
            entries: Vec::new(),
            file_path,
        }
    }

    pub fn push(&mut self, entry: HistoryEntry) {
        self.entries.push(entry);
    }

    /// Append the latest entry to the JSONL file.
    pub fn flush(&self) -> SeeClawResult<()> {
        if let Some(last) = self.entries.last() {
            let line = serde_json::to_string(last)?;
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.file_path)?;
            writeln!(file, "{}", line)?;
            tracing::debug!(
                path = %self.file_path.display(),
                "history entry flushed"
            );
        }
        Ok(())
    }
}

impl Default for SessionHistory {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns `%LOCALAPPDATA%\SeeClaw\sessions` on Windows,
/// `~/.local/share/seeclaw/sessions` on Linux/macOS,
/// falling back to the current working directory.
fn data_dir_or_cwd() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    let base = std::env::var("LOCALAPPDATA").ok().map(std::path::PathBuf::from);

    #[cfg(not(target_os = "windows"))]
    let base = std::env::var("HOME")
        .ok()
        .map(|h| std::path::PathBuf::from(h).join(".local").join("share"));

    if let Some(data_dir) = base {
        let d = data_dir.join("SeeClaw").join("sessions");
        let _ = std::fs::create_dir_all(&d);
        return d;
    }
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}
