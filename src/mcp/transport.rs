// MCP transport trait — full implementation in Phase 8.
use async_trait::async_trait;
use crate::errors::SeeClawResult;

#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn send(&self, request: serde_json::Value) -> SeeClawResult<serde_json::Value>;
}
