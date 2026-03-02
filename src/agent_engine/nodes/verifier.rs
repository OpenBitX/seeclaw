//! VerifierNode — takes a final screenshot and compares against the original
//! goal to determine if the task was successfully completed.
//!
//! - Pass → GoTo("summarizer") to generate human-readable response
//! - Fail → GoTo("planner") with failure context injected

use async_trait::async_trait;
use base64::Engine as _;
use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{poll_stop, Node, NodeOutput};
use crate::agent_engine::state::SharedState;
use crate::llm::types::{ChatMessage, ContentPart, ImageUrl, MessageContent};
use crate::perception::screenshot::capture_primary;

const VERIFIER_PROMPT: &str = include_str!("../../../prompts/system/verifier.md");

/// Maximum number of replan cycles before giving up.
const MAX_REPLAN_CYCLES: u32 = 3;

pub struct VerifierNode;

impl VerifierNode {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Node for VerifierNode {
    fn name(&self) -> &str {
        "verifier"
    }

    async fn execute(
        &self,
        state: &mut SharedState,
        ctx: &NodeContext,
    ) -> Result<NodeOutput, String> {
        if state.is_stopped() {
            return Ok(NodeOutput::End);
        }

        tracing::info!(
            goal = %state.goal,
            cycle = state.cycle_count,
            "VerifierNode: verifying task completion"
        );

        let _ = ctx.app.emit("agent_activity", serde_json::json!({ "text": "正在验证任务完成情况…" }));

        // Check cycle limit — delegate to summarizer even on exhaustion
        if state.cycle_count >= MAX_REPLAN_CYCLES {
            tracing::warn!("VerifierNode: max replan cycles reached → summarizer");
            state.steps_log.push(format!(
                "[验证] 已达到最大重试次数 ({})，任务可能未完全完成。",
                state.cycle_count
            ));
            return Ok(NodeOutput::GoTo("summarizer".to_string()));
        }

        // Capture final screenshot
        let shot = capture_primary().await.map_err(|e| e.to_string())?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&shot.image_bytes);
        let data_url = format!("data:image/png;base64,{b64}");

        // Build verification prompt
        let steps_summary = state.steps_log.join("\n");
        let verify_prompt = VERIFIER_PROMPT
            .replace("{goal}", &state.goal)
            .replace("{steps_summary}", &steps_summary);

        let messages = vec![ChatMessage {
            role: "user".into(),
            content: MessageContent::Parts(vec![
                ContentPart::ImageUrl {
                    image_url: ImageUrl { url: data_url },
                },
                ContentPart::Text {
                    text: verify_prompt,
                },
            ]),
            tool_call_id: None,
            tool_calls: None,
        }];

        let (provider, mut cfg) = {
            let reg = ctx.registry.lock().await;
            reg.call_config_for_role("vision").map_err(|e| e.to_string())?
        };
        cfg.silent = true;

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

        // Parse verification result
        let raw = response.content.trim();
        let json_str = raw
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        let (pass, reason) = match serde_json::from_str::<serde_json::Value>(json_str) {
            Ok(v) => {
                let pass = v["pass"].as_bool().unwrap_or(false)
                    || v["result"].as_str() == Some("pass")
                    || v["verified"].as_bool().unwrap_or(false);
                let reason = v["reason"]
                    .as_str()
                    .or(v["description"].as_str())
                    .unwrap_or("")
                    .to_string();
                (pass, reason)
            }
            Err(_) => {
                // If we can't parse JSON, check for keywords
                let lower = raw.to_lowercase();
                let pass = lower.contains("pass") || lower.contains("success") || lower.contains("completed");
                (pass, raw.to_string())
            }
        };

        if pass {
            tracing::info!(reason = %reason, "VerifierNode: PASS → summarizer");
            if !reason.is_empty() {
                state.steps_log.push(format!("[验证通过] {reason}"));
            }
            // Delegate human-readable response generation to SummarizerNode
            Ok(NodeOutput::GoTo("summarizer".to_string()))
        } else {
            tracing::warn!(reason = %reason, cycle = state.cycle_count, "VerifierNode: FAIL → replan");

            // Inject failure context into conversation
            state.conv_messages.push(ChatMessage {
                role: "user".into(),
                content: MessageContent::Text(format!(
                    "Verification failed. Reason: {reason}\n\
                     Please re-plan to complete the goal: {}\n\
                     This is retry cycle {}.",
                    state.goal, state.cycle_count
                )),
                tool_call_id: None,
                tool_calls: None,
            });

            // Reset for replan
            state.reset_for_replan();

            Ok(NodeOutput::GoTo("planner".to_string()))
        }
    }
}


