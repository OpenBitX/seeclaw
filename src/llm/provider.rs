use async_trait::async_trait;
use tauri::AppHandle;

use crate::errors::SeeClawResult;
use crate::llm::types::{ChatMessage, ToolDef};

/// Unified LLM provider trait. All providers implement this trait.
/// New providers only need to implement this trait and register in config.toml.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Returns the provider's identifier (matches config.toml key).
    fn name(&self) -> &str;

    /// Streams chat completion chunks back to the frontend via Tauri events.
    /// Emits "llm_stream_chunk" events with StreamChunk payload.
    async fn stream_chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDef>,
        app: &AppHandle,
    ) -> SeeClawResult<()>;
}
