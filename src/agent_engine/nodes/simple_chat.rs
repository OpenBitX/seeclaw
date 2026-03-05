//! SimpleChatNode — handles greetings, simple knowledge Q&A, and casual
//! conversation that require NO tools or GUI operations.
//!
//! This is the fastest path through the agent graph. It sends the user's
//! message to a lightweight chat model with a conversational prompt and
//! streams the response directly back. No screenshots, no tool calls.
//!
//! Flow: `router` → (Chat) → `simple_chat` → (end)

use async_trait::async_trait;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{poll_stop, Node, NodeOutput};
use crate::agent_engine::state::{GraphResult, SharedState};
use crate::llm::types::{ChatMessage, MessageContent, StreamChunk, StreamChunkKind};

const SIMPLE_CHAT_SYSTEM: &str = include_str!("../../../prompts/system/simple_chat.md");

pub struct SimpleChatNode;

impl SimpleChatNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for SimpleChatNode {
    fn name(&self) -> &str {
        "simple_chat"
    }

    async fn execute(
        &self,
        state: &mut SharedState,
        ctx: &NodeContext,
    ) -> Result<NodeOutput, String> {
        if state.is_stopped() {
            return Ok(NodeOutput::End);
        }

        tracing::info!(goal = %state.goal, "SimpleChatNode: answering conversational query");
        let _ = ctx.app.emit(
            "agent_activity",
            serde_json::json!({ "text": "正在回复…" }),
        );

        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: MessageContent::Text(SIMPLE_CHAT_SYSTEM.to_string()),
                tool_call_id: None,
                tool_calls: None,
            },
            ChatMessage {
                role: "user".into(),
                content: MessageContent::Text(state.goal.clone()),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        // Use the lightweight "chat" role — no tools needed
        let (provider, mut cfg) = {
            let reg = ctx.registry.lock().await;
            reg.call_config_for_role("chat").map_err(|e| e.to_string())?
        };
        // Stream to frontend so the user sees the response in real-time
        cfg.silent = false;
        cfg.stream = true;

        let flag = state.stop_flag.clone();
        let response = tokio::select! {
            result = provider.chat(messages, vec![], &cfg, &ctx.app) => {
                result.map_err(|e| e.to_string())?
            }
            _ = poll_stop(flag) => {
                return Ok(NodeOutput::End);
            }
        };

        if state.is_stopped() {
            return Ok(NodeOutput::End);
        }

        let answer = response.content.trim().to_string();

        // ── Log LLM response (truncated) ────────────────────────────────
        {
            let content_preview = if answer.len() > 100 {
                format!("{}…", &answer[..100])
            } else {
                answer.clone()
            };
            tracing::info!(
                content_len = answer.len(),
                content = %content_preview,
                "[SimpleChat] response: '{}'",
                content_preview
            );
        }

        // Emit Done to close the stream on the frontend
        let _ = ctx.app.emit(
            "llm_stream_chunk",
            &StreamChunk {
                kind: StreamChunkKind::Done,
                content: String::new(),
            },
        );

        state.result = Some(GraphResult::Done { summary: answer });
        Ok(NodeOutput::End)
    }
}
