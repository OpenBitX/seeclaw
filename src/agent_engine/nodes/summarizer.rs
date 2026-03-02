//! SummarizerNode — takes the full execution context (goal + steps_log) and
//! calls an LLM to generate a concise, human-readable final response.
//!
//! This node is the **sole gateway** for user-facing output. Neither the
//! verifier nor action_exec should emit `llm_stream_chunk` content any more;
//! they route here instead.
//!
//! The summarizer streams its output so the user sees tokens arriving in real
//! time, then emits a `Done` chunk to close the response.

use async_trait::async_trait;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{poll_stop, Node, NodeOutput};
use crate::agent_engine::state::{GraphResult, SharedState};
use crate::llm::types::{ChatMessage, MessageContent, StreamChunk, StreamChunkKind};

const SUMMARIZER_PROMPT: &str = include_str!("../../../prompts/system/summarizer.md");

pub struct SummarizerNode;

impl SummarizerNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for SummarizerNode {
    fn name(&self) -> &str {
        "summarizer"
    }

    async fn execute(
        &self,
        state: &mut SharedState,
        ctx: &NodeContext,
    ) -> Result<NodeOutput, String> {
        if state.is_stopped() {
            return Ok(NodeOutput::End);
        }

        tracing::info!(goal = %state.goal, "SummarizerNode: generating final response");
        let _ = ctx.app.emit(
            "agent_activity",
            serde_json::json!({ "text": "正在总结回复…" }),
        );

        // Build context for the summarizer LLM
        let steps_summary = if state.steps_log.is_empty() {
            "(no execution log)".to_string()
        } else {
            state.steps_log.join("\n")
        };

        let system_prompt = SUMMARIZER_PROMPT
            .replace("{goal}", &state.goal)
            .replace("{steps_summary}", &steps_summary);

        let messages = vec![ChatMessage {
            role: "system".into(),
            content: MessageContent::Text(system_prompt),
            tool_call_id: None,
            tool_calls: None,
        }];

        // Use the "chat" role — lightweight, streaming-enabled
        let (provider, mut cfg) = {
            let reg = ctx.registry.lock().await;
            reg.call_config_for_role("chat").map_err(|e| e.to_string())?
        };
        // Stream to the user (silent = false means provider emits llm_stream_chunk)
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

        let summary = response.content.trim().to_string();

        // Emit Done to close the stream on the frontend
        let _ = ctx.app.emit(
            "llm_stream_chunk",
            &StreamChunk {
                kind: StreamChunkKind::Done,
                content: String::new(),
            },
        );

        state.result = Some(GraphResult::Done { summary });
        Ok(NodeOutput::End)
    }
}
