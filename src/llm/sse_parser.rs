use crate::errors::{SeeClawError, SeeClawResult};
use crate::llm::types::{StreamChunk, StreamChunkKind};

/// Parses a raw SSE line (OpenAI-compatible format) into a StreamChunk.
/// Returns None if the line is a keep-alive or non-data line.
pub fn parse_sse_line(line: &str) -> SeeClawResult<Option<StreamChunk>> {
    if line.is_empty() || line.starts_with(':') {
        return Ok(None);
    }

    let data = if let Some(d) = line.strip_prefix("data: ") {
        d.trim()
    } else {
        return Ok(None);
    };

    if data == "[DONE]" {
        return Ok(Some(StreamChunk {
            kind: StreamChunkKind::Done,
            content: String::new(),
        }));
    }

    let json: serde_json::Value =
        serde_json::from_str(data).map_err(|e| SeeClawError::SseParsing(e.to_string()))?;

    // Extract delta content (OpenAI-compatible format)
    if let Some(choices) = json["choices"].as_array() {
        if let Some(first) = choices.first() {
            let delta = &first["delta"];

            // Reasoning content (some models like DeepSeek expose this)
            if let Some(reasoning) = delta["reasoning_content"].as_str() {
                if !reasoning.is_empty() {
                    return Ok(Some(StreamChunk {
                        kind: StreamChunkKind::Reasoning,
                        content: reasoning.to_string(),
                    }));
                }
            }

            // Tool calls
            if let Some(tool_calls) = delta["tool_calls"].as_array() {
                if !tool_calls.is_empty() {
                    return Ok(Some(StreamChunk {
                        kind: StreamChunkKind::ToolCall,
                        content: serde_json::to_string(tool_calls)
                            .map_err(|e| SeeClawError::SseParsing(e.to_string()))?,
                    }));
                }
            }

            // Regular content
            if let Some(content) = delta["content"].as_str() {
                if !content.is_empty() {
                    return Ok(Some(StreamChunk {
                        kind: StreamChunkKind::Content,
                        content: content.to_string(),
                    }));
                }
            }

            // Finish reason signals done
            if first["finish_reason"].as_str().is_some() {
                return Ok(Some(StreamChunk {
                    kind: StreamChunkKind::Done,
                    content: String::new(),
                }));
            }
        }
    }

    Ok(None)
}
