//! SummarizerNode — takes the full execution context (goal + steps_log) and
//! calls an LLM to generate a concise, human-readable final response.
//!
//! Whether to capture a screenshot and use the vision model is decided by the
//! 3-layer `VisualDecisionPipeline` (regex → Bayesian → LLM), mirroring the
//! main router design. Only tasks that genuinely need on-screen content will
//! trigger a screenshot; pure action tasks summarize from the execution log.

use async_trait::async_trait;
use base64::Engine as _;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{poll_stop, Node, NodeOutput};
use crate::agent_engine::nodes::visual_router::VisualDecisionPipeline;
use crate::agent_engine::state::{GraphResult, SharedState};
use crate::llm::types::{ChatMessage, ContentPart, ImageUrl, MessageContent, StreamChunk, StreamChunkKind};
use crate::perception::screenshot::capture_primary;

const SUMMARIZER_PROMPT: &str = include_str!("../../../prompts/system/summarizer.md");

pub struct SummarizerNode {
    /// Decides whether a screenshot is needed for this summarization.
    visual_pipeline: VisualDecisionPipeline,
}

impl SummarizerNode {
    pub fn new() -> Self {
        Self {
            visual_pipeline: VisualDecisionPipeline::new(),
        }
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

        // Build execution log context
        let steps_summary = if state.steps_log.is_empty() {
            "(no execution log)".to_string()
        } else {
            state.steps_log.join("\n")
        };

        let system_prompt = SUMMARIZER_PROMPT
            .replace("{goal}", &state.goal)
            .replace("{steps_summary}", &steps_summary);

        // Ask the 3-layer visual decision pipeline: regex → Bayesian → LLM.
        // Only acquires a screenshot when genuinely needed for the answer.
        let decision = self.visual_pipeline
            .classify(&state.goal, &state.steps_log, &state.todo_steps, ctx)
            .await;
        let needs_visual = decision.needs_visual;
        tracing::debug!(
            needs_visual,
            confidence = decision.confidence,
            "SummarizerNode: visual decision"
        );

        let (messages, role) = if needs_visual {
            let _ = ctx.app.emit("agent_activity", serde_json::json!({ "text": "正在截取屏幕用于总结…" }));
            match capture_primary().await {
                Ok(shot) => {
                    let screenshot_b64 =
                        base64::engine::general_purpose::STANDARD.encode(&shot.image_bytes);

                    // Show the screenshot in the frontend so the user can see what was captured
                    let _ = ctx.app.emit("viewport_captured", serde_json::json!({
                        "image_base64": &screenshot_b64,
                        "source": "summarizer",
                    }));

                    let data_url = format!("data:image/png;base64,{screenshot_b64}");
                    let msgs = vec![
                        ChatMessage {
                            role: "system".into(),
                            content: MessageContent::Text(system_prompt),
                            tool_call_id: None,
                            tool_calls: None,
                        },
                        ChatMessage {
                            role: "user".into(),
                            content: MessageContent::Parts(vec![
                                ContentPart::ImageUrl {
                                    image_url: ImageUrl { url: data_url },
                                },
                                ContentPart::Text {
                                    text: String::new(),
                                },
                            ]),
                            tool_call_id: None,
                            tool_calls: None,
                        },
                    ];
                    (msgs, "vision")
                }
                Err(e) => {
                    tracing::warn!(error = %e, "SummarizerNode: screenshot capture failed, falling back to text-only");
                    let msgs = vec![ChatMessage {
                        role: "system".into(),
                        content: MessageContent::Text(system_prompt),
                        tool_call_id: None,
                        tool_calls: None,
                    }];
                    (msgs, "chat")
                }
            }
        } else {
            // Text-only summary — no screenshot needed
            let msgs = vec![ChatMessage {
                role: "system".into(),
                content: MessageContent::Text(system_prompt),
                tool_call_id: None,
                tool_calls: None,
            }];
            (msgs, "chat")
        };

        let (provider, mut cfg) = {
            let reg = ctx.registry.lock().await;
            reg.call_config_for_role(role).map_err(|e| e.to_string())?
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

        // ── Log LLM/VLM response (truncated) ────────────────────────────────
        {
            let content_preview = truncate(&summary, 100);
            tracing::info!(
                needs_visual,
                content_len = summary.len(),
                content = %content_preview,
                "[Summarizer] response (visual={}): '{}'",
                needs_visual, content_preview
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

        state.result = Some(GraphResult::Done { summary });
        Ok(NodeOutput::End)
    }
}

/// Truncate to `max` chars with "…" if longer (for log display).
fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > max {
        format!("{}…", chars[..max].iter().collect::<String>())
    } else {
        s.to_string()
    }
}
