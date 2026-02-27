// stdio transport for MCP — full implementation in Phase 8.
use async_trait::async_trait;
use crate::errors::{SeeClawError, SeeClawResult};
use crate::mcp::transport::McpTransport;

pub struct StdioTransport {
    pub command: String,
    pub args: Vec<String>,
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send(&self, _request: serde_json::Value) -> SeeClawResult<serde_json::Value> {
        Err(SeeClawError::Mcp("stdio transport not implemented yet (Phase 8)".to_string()))
    }
}
