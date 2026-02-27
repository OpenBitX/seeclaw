// OpenAI-compatible provider — full implementation in Phase 2.
// Covers: GLM/Zhipu, OpenAI, DeepSeek, Qwen, OpenRouter.
use async_trait::async_trait;
use tauri::AppHandle;

use crate::errors::SeeClawResult;
use crate::llm::provider::LlmProvider;
use crate::llm::types::{ChatMessage, ProviderConfig, ToolDef};

pub struct OpenAiCompatibleProvider {
    id: String,
    config: ProviderConfig,
    api_key: String,
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(id: String, config: ProviderConfig, api_key: String) -> Self {
        Self {
            id,
            config,
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    fn name(&self) -> &str {
        &self.id
    }

    async fn stream_chat(
        &self,
        _messages: Vec<ChatMessage>,
        _tools: Vec<ToolDef>,
        _app: &AppHandle,
    ) -> SeeClawResult<()> {
        // Full SSE streaming implementation in Phase 2
        tracing::info!(provider = %self.id, model = %self.config.model, "stream_chat stub called");
        Ok(())
    }
}
