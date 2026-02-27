use std::collections::BTreeMap;

use async_trait::async_trait;
use futures_util::StreamExt;
use tauri::{AppHandle, Emitter};

use crate::errors::{SeeClawError, SeeClawResult};
use crate::llm::provider::LlmProvider;
use crate::llm::sse_parser;
use crate::llm::types::{
    CallConfig, ChatMessage, FunctionCall, LlmResponse, StreamChunk, StreamChunkKind, ToolCall,
    ToolDef,
};

pub struct OpenAiCompatibleProvider {
    id: String,
    api_base: String,
    api_key: String,
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new(id: String, api_base: String, api_key: String) -> Self {
        Self {
            id,
            api_base,
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

    async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDef>,
        cfg: &CallConfig,
        app: &AppHandle,
    ) -> SeeClawResult<LlmResponse> {
        let mut body = serde_json::json!({
            "model": cfg.model,
            "messages": &messages,
            "stream": cfg.stream,
            "temperature": cfg.temperature,
        });

        if !tools.is_empty() {
            body["tools"] = serde_json::to_value(&tools)?;
            body["tool_choice"] = serde_json::json!("auto");
        }

        tracing::debug!(
            provider = %self.id,
            model = %cfg.model,
            stream = cfg.stream,
            "sending LLM request"
        );
        tracing::debug!(
            body = %{
                // Clone body and sanitize only for logging so the actual request
                // still contains the real image payloads.
                let mut log_body = body.clone();
                // Sanitize large payloads (e.g. base64 images) before logging.
                if let Some(msgs) = log_body.get_mut("messages").and_then(|m| m.as_array_mut()) {
                    for msg in msgs {
                        if let Some(content) = msg.get_mut("content") {
                            // content can be string or array of parts; we only touch the array case.
                            if let Some(parts) = content.as_array_mut() {
                                for part in parts {
                                    if part.get("type").and_then(|t| t.as_str()) == Some("image_url") {
                                        if let Some(image_url) = part.get_mut("image_url") {
                                            if let Some(url) = image_url.get_mut("url") {
                                                *url = serde_json::Value::String("<omitted_base64_image>".to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                serde_json::to_string(&log_body).unwrap_or_default()
            },
            "request body (sanitized, base64 omitted)"
        );

        let response = self
            .client
            .post(&self.api_base)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let err_body = response.text().await.unwrap_or_default();
            return Err(SeeClawError::LlmProvider(format!("{}: {}", status, err_body)));
        }

        if cfg.stream {
            self.handle_stream(response, app).await
        } else {
            self.handle_json(response, app).await
        }
    }
}

impl OpenAiCompatibleProvider {
    /// Handle SSE streaming response.
    /// Streams chunks to the frontend and accumulates the full response to return.
    async fn handle_stream(
        &self,
        response: reqwest::Response,
        app: &AppHandle,
    ) -> SeeClawResult<LlmResponse> {
        let mut byte_stream = response.bytes_stream();
        let mut line_buf = String::new();

        let mut resp_content = String::new();
        let mut resp_reasoning = String::new();
        // Tool call accumulator: delta index → (id, type, name, accumulated_arguments)
        let mut tc_builders: BTreeMap<usize, (String, String, String, String)> = BTreeMap::new();
        let mut done_emitted = false;

        'stream: while let Some(result) = byte_stream.next().await {
            let bytes = result?;
            let text = String::from_utf8_lossy(&bytes);

            for ch in text.chars() {
                if ch == '\n' {
                    let line = line_buf.trim().to_string();
                    line_buf.clear();

                    if line.is_empty() {
                        continue;
                    }

                    match sse_parser::parse_sse_line(&line) {
                        Ok(Some(chunk)) => {
                            let is_done = matches!(chunk.kind, StreamChunkKind::Done);

                            // Accumulate before forwarding to frontend
                            match &chunk.kind {
                                StreamChunkKind::Reasoning => {
                                    resp_reasoning.push_str(&chunk.content);
                                }
                                StreamChunkKind::Content => {
                                    resp_content.push_str(&chunk.content);
                                }
                                StreamChunkKind::ToolCall => {
                                    merge_tool_call_deltas(&chunk.content, &mut tc_builders);
                                }
                                _ => {}
                            }

                            let _ = app.emit("llm_stream_chunk", &chunk);

                            if is_done {
                                done_emitted = true;
                                break 'stream;
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            tracing::debug!("SSE parse skipped: {e}");
                        }
                    }
                } else {
                    line_buf.push(ch);
                }
            }
        }

        // Fallback Done in case stream ended without [DONE] marker
        if !done_emitted {
            let _ = app.emit(
                "llm_stream_chunk",
                &StreamChunk {
                    kind: StreamChunkKind::Done,
                    content: String::new(),
                },
            );
        }

        let tool_calls = build_tool_calls(tc_builders);

        tracing::info!(
            content_len = resp_content.len(),
            reasoning_len = resp_reasoning.len(),
            tool_calls = tool_calls.len(),
            tools = ?tool_calls.iter().map(|tc| tc.function.name.as_str()).collect::<Vec<_>>(),
            "LLM stream complete"
        );

        Ok(LlmResponse {
            content: resp_content,
            reasoning: resp_reasoning,
            tool_calls,
        })
    }

    /// Handle a non-streaming JSON response.
    async fn handle_json(
        &self,
        response: reqwest::Response,
        app: &AppHandle,
    ) -> SeeClawResult<LlmResponse> {
        let json: serde_json::Value = response.json().await?;

        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let tool_calls: Vec<ToolCall> = json["choices"][0]["message"]["tool_calls"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|tc| ToolCall {
                        id: tc["id"].as_str().unwrap_or("").to_string(),
                        call_type: tc["type"].as_str().unwrap_or("function").to_string(),
                        function: FunctionCall {
                            name: tc["function"]["name"].as_str().unwrap_or("").to_string(),
                            arguments: tc["function"]["arguments"]
                                .as_str()
                                .unwrap_or("{}")
                                .to_string(),
                        },
                    })
                    .collect()
            })
            .unwrap_or_default();

        tracing::info!(
            content_len = content.len(),
            tool_calls = tool_calls.len(),
            "LLM JSON response received"
        );

        if !content.is_empty() {
            let _ = app.emit(
                "llm_stream_chunk",
                &StreamChunk {
                    kind: StreamChunkKind::Content,
                    content: content.clone(),
                },
            );
        }
        if !tool_calls.is_empty() {
            if let Ok(tc_json) = serde_json::to_string(&tool_calls) {
                let _ = app.emit(
                    "llm_stream_chunk",
                    &StreamChunk {
                        kind: StreamChunkKind::ToolCall,
                        content: tc_json,
                    },
                );
            }
        }
        let _ = app.emit(
            "llm_stream_chunk",
            &StreamChunk {
                kind: StreamChunkKind::Done,
                content: String::new(),
            },
        );

        Ok(LlmResponse {
            content,
            reasoning: String::new(),
            tool_calls,
        })
    }
}

/// Merge streaming tool-call delta fragments into the accumulator map (keyed by delta index).
fn merge_tool_call_deltas(
    chunk_content: &str,
    builders: &mut BTreeMap<usize, (String, String, String, String)>,
) {
    let Ok(deltas) = serde_json::from_str::<Vec<serde_json::Value>>(chunk_content) else {
        return;
    };
    for delta in deltas {
        let idx = delta["index"].as_u64().unwrap_or(0) as usize;
        let entry = builders.entry(idx).or_default();

        if let Some(id) = delta["id"].as_str() {
            if !id.is_empty() {
                entry.0 = id.to_string();
            }
        }
        if let Some(t) = delta["type"].as_str() {
            if !t.is_empty() {
                entry.1 = t.to_string();
            }
        }
        if let Some(name) = delta["function"]["name"].as_str() {
            if !name.is_empty() {
                entry.2.push_str(name);
            }
        }
        if let Some(args) = delta["function"]["arguments"].as_str() {
            entry.3.push_str(args);
        }
    }
}

/// Convert accumulated tool-call builders into typed `ToolCall` structs.
fn build_tool_calls(
    builders: BTreeMap<usize, (String, String, String, String)>,
) -> Vec<ToolCall> {
    builders
        .into_values()
        .filter(|(_, _, name, _)| !name.is_empty())
        .map(|(id, call_type, name, arguments)| ToolCall {
            id,
            call_type: if call_type.is_empty() {
                "function".to_string()
            } else {
                call_type
            },
            function: FunctionCall { name, arguments },
        })
        .collect()
}
