use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub def_type: String,
    pub function: FunctionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub kind: StreamChunkKind,
    pub content: String,
}

/// The fully-accumulated response returned by `LlmProvider::chat`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub reasoning: String,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamChunkKind {
    Reasoning,
    Content,
    ToolCall,
    Done,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub display_name: String,
    pub api_base: String,
    pub model: String,
    pub temperature: f64,
}

/// Per-call configuration that controls which model to use and how to call it.
#[derive(Debug, Clone)]
pub struct CallConfig {
    /// Model identifier sent to the API (e.g. "GLM-4.7-Flash").
    pub model: String,
    /// Whether to use SSE streaming; non-streaming returns a single JSON response.
    pub stream: bool,
    pub temperature: f64,
    /// When true, do NOT emit `llm_stream_chunk` events to the frontend.
    /// Use for internal calls (e.g. VLM element-location queries) that should
    /// be invisible to the user.
    pub silent: bool,
}
