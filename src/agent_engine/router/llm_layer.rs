//! L3: LLM fallback classification layer.
//!
//! Uses a lightweight model (the `routing` role in the provider registry)
//! to classify the query. For simple routes, it also generates tool_calls
//! in a single LLM call (two birds with one stone).

use async_trait::async_trait;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::router::layer::{RouteResult, RouterLayer};
use crate::agent_engine::state::RouteType;
use crate::llm::types::{ChatMessage, FunctionCall, MessageContent, ToolCall};

const ROUTER_SYSTEM_PROMPT: &str = include_str!("../../../prompts/system/router.md");

/// LLM-based router layer (L3 fallback).
pub struct LlmLayer;

impl LlmLayer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RouterLayer for LlmLayer {
    fn name(&self) -> &str {
        "llm"
    }

    async fn classify(&self, query: &str, ctx: &NodeContext) -> Option<RouteResult> {
        // Try to get the routing provider; if not configured, fall back to complex
        let (provider, mut cfg) = {
            let reg = ctx.registry.lock().await;
            match reg.call_config_for_role("routing") {
                Ok(pair) => pair,
                Err(e) => {
                    tracing::warn!(error = %e, "routing provider not configured — defaulting to Complex");
                    return Some(RouteResult {
                        route_type: RouteType::Complex,
                        confidence: 0.5,
                        tool_calls: None,
                    });
                }
            }
        };

        // Use non-streaming, silent call for routing with JSON mode enabled
        cfg.stream = false;
        cfg.silent = true;
        cfg.json_mode = true;

        let messages = vec![
            ChatMessage {
                role: "system".into(),
                content: MessageContent::Text(ROUTER_SYSTEM_PROMPT.to_string()),
                tool_call_id: None,
                tool_calls: None,
            },
            ChatMessage {
                role: "user".into(),
                content: MessageContent::Text(query.to_string()),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        match provider.chat(messages, vec![], &cfg, &ctx.app).await {
            Ok(response) => {
                let raw = response.content.trim();
                tracing::debug!(layer = "llm", raw = %raw, "router LLM response");

                // Parse the response JSON: { "route_type": "simple"|"complex", "tool_calls": [...] }
                let json_str = raw
                    .trim_start_matches("```json")
                    .trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim();

                match serde_json::from_str::<serde_json::Value>(json_str) {
                    Ok(v) => {
                        let route_type = match v["route_type"].as_str() {
                            Some("simple") => RouteType::Simple,
                            _ => RouteType::Complex,
                        };

                        // Extract tool_calls from the JSON body (NOT response.tool_calls,
                        // because we used json_mode — tool_calls live inside the content).
                        let tool_calls = if route_type == RouteType::Simple {
                            v["tool_calls"]
                                .as_array()
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|tc| {
                                            let name = tc["name"].as_str()?;
                                            let args = &tc["arguments"];
                                            Some(ToolCall {
                                                id: format!("router_{name}"),
                                                call_type: "function".to_string(),
                                                function: FunctionCall {
                                                    name: name.to_string(),
                                                    arguments: serde_json::to_string(args)
                                                        .unwrap_or_else(|_| "{}".to_string()),
                                                },
                                            })
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .filter(|v| !v.is_empty())
                        } else {
                            None
                        };

                        Some(RouteResult {
                            route_type,
                            confidence: v["confidence"].as_f64().unwrap_or(0.8) as f32,
                            tool_calls,
                        })
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, raw = %json_str, "router LLM JSON parse failed — defaulting to Complex");
                        Some(RouteResult {
                            route_type: RouteType::Complex,
                            confidence: 0.5,
                            tool_calls: None,
                        })
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "router LLM call failed — defaulting to Complex");
                Some(RouteResult {
                    route_type: RouteType::Complex,
                    confidence: 0.5,
                    tool_calls: None,
                })
            }
        }
    }
}
