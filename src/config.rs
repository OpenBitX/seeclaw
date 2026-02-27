use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::errors::{SeeClawError, SeeClawResult};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub llm: LlmConfig,
    #[serde(default)]
    pub safety: SafetyConfig,
    #[serde(default)]
    pub prompts: PromptsConfig,
    #[serde(default)]
    pub mcp: McpConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmConfig {
    pub active_provider: String,
    pub providers: HashMap<String, ProviderEntry>,
    /// Role-to-model mapping. If a role is absent, falls back to active_provider defaults.
    #[serde(default)]
    pub roles: RolesConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEntry {
    pub display_name: String,
    pub api_base: String,
    /// Default model for this provider (used as fallback when no role config exists).
    pub model: String,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    /// "anthropic" for Claude, None for OpenAI-compatible
    pub adapter: Option<String>,
    /// Optional API key stored in config.toml (falls back to env var SEECLAW_<ID>_API_KEY).
    #[serde(default)]
    pub api_key: Option<String>,
}

/// Maps agent roles to specific provider+model combinations.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RolesConfig {
    /// Fast router/classifier: decides next state. Usually non-streaming.
    pub routing: Option<RoleEntry>,
    /// Main conversational LLM: streaming reply shown to user.
    pub chat: Option<RoleEntry>,
    /// Tool-calling / function-call capable model.
    pub tools: Option<RoleEntry>,
    /// Vision / image-understanding model.
    pub vision: Option<RoleEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleEntry {
    /// Must match a key under [llm.providers.*].
    pub provider: String,
    /// Model name sent to the API.
    pub model: String,
    /// Use SSE streaming. Set false for fast classifier calls.
    #[serde(default = "default_true")]
    pub stream: bool,
    /// Overrides the provider-level temperature for this role.
    pub temperature: Option<f64>,
}

fn default_temperature() -> f64 {
    0.1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    #[serde(default)]
    pub allow_terminal_commands: bool,
    #[serde(default)]
    pub allow_file_operations: bool,
    #[serde(default)]
    pub require_approval_for: Vec<String>,
    #[serde(default = "default_max_failures")]
    pub max_consecutive_failures: u32,
    #[serde(default)]
    pub max_loop_duration_minutes: u32,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            allow_terminal_commands: false,
            allow_file_operations: false,
            require_approval_for: vec!["execute_terminal".into(), "mcp_call".into()],
            max_consecutive_failures: default_max_failures(),
            max_loop_duration_minutes: 0,
        }
    }
}

fn default_max_failures() -> u32 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromptsConfig {
    #[serde(default)]
    pub tools_file: String,
    #[serde(default)]
    pub system_template: String,
    #[serde(default)]
    pub experience_summary_template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: Vec<McpServerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerEntry {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Returns the path to an *existing* config.toml for reading.
fn find_config_path() -> SeeClawResult<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join("config.toml");
            if candidate.exists() {
                tracing::debug!(path = %candidate.display(), "config found next to executable");
                return Ok(candidate);
            }
        }
    }
    let cwd = std::env::current_dir()?;
    let candidate = cwd.join("config.toml");
    if candidate.exists() {
        tracing::debug!(path = %candidate.display(), "config found in working directory");
        return Ok(candidate);
    }
    Err(SeeClawError::Config(
        "config.toml not found next to executable or in working directory".into(),
    ))
}

/// Returns the canonical path where config should be **written**.
/// Prefers the exe-adjacent path (works for production bundles).
/// Falls back to cwd (works for `cargo tauri dev`).
/// Does NOT require the file to already exist.
fn write_config_path() -> SeeClawResult<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            return Ok(parent.join("config.toml"));
        }
    }
    Ok(std::env::current_dir()?.join("config.toml"))
}

pub fn load_config() -> SeeClawResult<AppConfig> {
    let path = find_config_path()?;
    let content = std::fs::read_to_string(&path)?;
    let config: AppConfig = toml::from_str(&content)?;
    tracing::info!(path = %path.display(), provider = %config.llm.active_provider, "config loaded");
    Ok(config)
}

pub fn save_config(config: &AppConfig) -> SeeClawResult<()> {
    // Use write_config_path so saving works even on first run (no existing file required).
    let path = write_config_path()?;
    let content = toml::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    tracing::info!(path = %path.display(), "config saved");
    Ok(())
}
