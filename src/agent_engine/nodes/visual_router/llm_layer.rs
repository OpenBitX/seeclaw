//! L3: LLM fallback layer.
//!
//! Called only when L1 (regex) and L2 (Bayesian) cannot make a confident decision.
//! Uses the `routing` role (lightweight, silent, json_mode) so cost/latency is minimal.

use async_trait::async_trait;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::nodes::visual_router::layer::{VisualDecisionLayer, VisualDecisionResult};
use crate::agent_engine::state::TodoStep;
use crate::llm::types::{ChatMessage, MessageContent};

const VISUAL_ROUTER_PROMPT: &str = include_str!("../../../../prompts/system/visual_router.md");

pub struct VisualLlmLayer;

impl VisualLlmLayer {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl VisualDecisionLayer for VisualLlmLayer {
    fn name(&self) -> &str { "visual_llm" }

    async fn classify(
        &self,
        goal: &str,
        steps_log: &[String],
        _todo_steps: &[TodoStep],
        ctx: &NodeContext,
    ) -> Option<VisualDecisionResult> {
        // Prefer the lightweight `routing` model; fall back to `chat` if not configured.
        let (provider, mut cfg) = {
            let reg = ctx.registry.lock().await;
            match reg.call_config_for_role("routing") {
                Ok(pair) => pair,
                Err(_) => match reg.call_config_for_role("chat") {
                    Ok(pair) => pair,
                    Err(e) => {
                        tracing::warn!(error = %e, "visual_router: no provider available — defaulting to needs_visual=false");
                        return Some(VisualDecisionResult { needs_visual: false, confidence: 0.5 });
                    }
                },
            }
        };
        cfg.stream = false;
        cfg.silent = true;
        cfg.json_mode = true;

        let log_summary = if steps_log.is_empty() {
            "(no steps executed)".to_string()
        } else {
            steps_log.join("\n")
        };

        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: MessageContent::Text(VISUAL_ROUTER_PROMPT.to_string()),
                tool_call_id: None,
                tool_calls: None,
            },
            ChatMessage {
                role: "user".into(),
                content: MessageContent::Text(format!(
                    "User's goal: {goal}\n\nExecution log:\n{log_summary}"
                )),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            provider.chat(messages, vec![], &cfg, &ctx.app),
        )
        .await;

        match result {
            Ok(Ok(response)) => {
                let raw = response.content.trim();
                tracing::debug!(layer = "visual_llm", raw = %raw, "LLM response");

                let json_str = raw
                    .trim_start_matches("```json")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim();

                match serde_json::from_str::<serde_json::Value>(json_str) {
                    Ok(v) => {
                        let needs_visual = v["needs_visual"].as_bool().unwrap_or(false);
                        let confidence = v["confidence"].as_f64().unwrap_or(0.7) as f32;
                        tracing::info!(
                            layer = "visual_llm",
                            needs_visual,
                            confidence,
                            "[VisualRouter] decision: needs_visual={} confidence={}",
                            needs_visual, confidence
                        );
                        Some(VisualDecisionResult { needs_visual, confidence })
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, raw = %json_str, "visual_router LLM JSON parse failed — defaulting to false");
                        Some(VisualDecisionResult { needs_visual: false, confidence: 0.5 })
                    }
                }
            }
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "visual_router LLM call failed — defaulting to false");
                Some(VisualDecisionResult { needs_visual: false, confidence: 0.5 })
            }
            Err(_) => {
                tracing::warn!("visual_router LLM call timed out — defaulting to false");
                Some(VisualDecisionResult { needs_visual: false, confidence: 0.5 })
            }
        }
    }
}
