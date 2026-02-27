use thiserror::Error;

#[derive(Debug, Error)]
pub enum SeeClawError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("LLM provider error: {0}")]
    LlmProvider(String),

    #[error("SSE parsing error: {0}")]
    SseParsing(String),

    #[error("Perception error: {0}")]
    Perception(String),

    #[error("Executor error: {0}")]
    Executor(String),

    #[error("Safety violation: {0}")]
    SafetyViolation(String),

    #[error("MCP error: {0}")]
    Mcp(String),

    #[error("RAG error: {0}")]
    Rag(String),

    #[error("Skills error: {0}")]
    Skills(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("TOML deserialize error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("Agent error: {0}")]
    Agent(String),

    #[error("Task cancelled")]
    Cancelled,
}

impl serde::Serialize for SeeClawError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}

pub type SeeClawResult<T> = Result<T, SeeClawError>;
