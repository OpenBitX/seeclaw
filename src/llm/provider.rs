use async_trait::async_trait;
use tauri::AppHandle;

use crate::errors::SeeClawResult;
use crate::llm::types::{CallConfig, ChatMessage, LlmResponse, ToolDef};

/// Unified LLM provider trait. All providers implement this trait.
/// New providers only need to implement this trait and register in config.toml.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Returns the provider's identifier (matches config.toml key).
    fn name(&self) -> &str;

    /// Execute a chat call with per-call configuration.
    ///
    /// Streams "llm_stream_chunk" events to the frontend in real time, and returns
    /// the fully-accumulated `LlmResponse` (content, reasoning, tool_calls) so the
    /// engine can act on any tool calls the model requested.
    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDef>,
        cfg: &CallConfig,
        app: &AppHandle,
    ) -> SeeClawResult<LlmResponse>;
}
