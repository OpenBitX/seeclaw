use crate::errors::{SeeClawError, SeeClawResult};
use crate::llm::types::ToolDef;

/// Loads built-in tool definitions from the prompts/tools/builtin.json file.
/// The JSON is embedded at compile time via include_str!.
pub fn load_builtin_tools() -> SeeClawResult<Vec<ToolDef>> {
    let json = include_str!("../../prompts/tools/builtin.json");
    serde_json::from_str(json).map_err(|e| SeeClawError::Config(format!("Failed to parse builtin tools: {e}")))
}
