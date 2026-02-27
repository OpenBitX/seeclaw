// MCP client — full implementation in Phase 8.
use crate::errors::{SeeClawError, SeeClawResult};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

pub struct McpClient {
    pub server_name: String,
}

impl McpClient {
    pub fn new(server_name: String) -> Self {
        Self { server_name }
    }

    pub async fn list_tools(&self) -> SeeClawResult<Vec<McpTool>> {
        Err(SeeClawError::Mcp("MCP not implemented yet (Phase 8)".to_string()))
    }

    pub async fn call_tool(
        &self,
        _name: &str,
        _args: serde_json::Value,
    ) -> SeeClawResult<serde_json::Value> {
        Err(SeeClawError::Mcp("MCP not implemented yet (Phase 8)".to_string()))
    }
}
