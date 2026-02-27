use std::sync::Arc;

use tauri::{AppHandle, State};
use tokio::sync::Mutex;

use crate::agent_engine::state::AgentEvent;
use crate::config::{load_config, save_config, AppConfig};
use crate::llm::registry::ProviderRegistry;
use crate::llm::tools::load_builtin_tools;
use crate::llm::types::ChatMessage;
use crate::AgentHandle;

/// Ping command for IPC verification.
#[tauri::command]
pub async fn ping() -> Result<String, String> {
    Ok("pong".to_string())
}

/// Get app version.
#[tauri::command]
pub async fn get_version() -> Result<String, String> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}

/// Send a goal to the AgentEngine and start the run loop.
#[tauri::command]
pub async fn start_task(
    _app: AppHandle,
    handle: State<'_, Arc<AgentHandle>>,
    task: String,
) -> Result<(), String> {
    tracing::info!(task = %task, "start_task: forwarding GoalReceived to AgentEngine");
    handle
        .tx
        .send(AgentEvent::GoalReceived(task))
        .await
        .map_err(|e| {
            tracing::error!("start_task: channel send failed: {e}");
            format!("agent channel closed: {e}")
        })?;
    tracing::info!("start_task: GoalReceived sent successfully");
    Ok(())
}

/// Signal the AgentEngine to stop.
#[tauri::command]
pub async fn stop_task(
    _app: AppHandle,
    handle: State<'_, Arc<AgentHandle>>,
) -> Result<(), String> {
    tracing::info!("stop_task: sending Stop to AgentEngine");
    handle
        .tx
        .send(AgentEvent::Stop)
        .await
        .map_err(|e| {
            tracing::error!("stop_task: channel send failed: {e}");
            format!("agent channel closed: {e}")
        })?;
    Ok(())
}

/// Confirm or deny a pending high-risk action.
#[tauri::command]
pub async fn confirm_action(
    _app: AppHandle,
    handle: State<'_, Arc<AgentHandle>>,
    approved: bool,
) -> Result<(), String> {
    tracing::info!(approved = approved, "confirm_action: forwarding to AgentEngine");
    let event = if approved {
        AgentEvent::UserApproved
    } else {
        AgentEvent::UserRejected
    };
    handle
        .tx
        .send(event)
        .await
        .map_err(|e| format!("agent channel closed: {e}"))?;
    Ok(())
}

/// Direct chat command — bypasses the agent engine, uses the "chat" role config.
/// Emits "llm_stream_chunk" events to the frontend as chunks arrive.
#[tauri::command]
pub async fn start_chat(
    app: AppHandle,
    state: State<'_, Arc<Mutex<ProviderRegistry>>>,
    messages: Vec<ChatMessage>,
) -> Result<(), String> {
    let tools = load_builtin_tools().map_err(|e| e.to_string())?;
    let (provider, cfg) = {
        let registry = state.lock().await;
        registry.call_config_for_role("chat").map_err(|e| e.to_string())?
    };
    provider
        .chat(messages, tools, &cfg, &app)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Return the current AppConfig as JSON for the settings UI.
/// API keys are redacted (replaced with "***") before sending to the frontend.
#[tauri::command]
pub async fn get_config() -> Result<serde_json::Value, String> {
    let mut cfg = load_config().map_err(|e| e.to_string())?;
    // Redact api_key values for security
    for entry in cfg.llm.providers.values_mut() {
        if entry.api_key.as_deref().map(|k| !k.is_empty()).unwrap_or(false) {
            entry.api_key = Some("***".to_string());
        }
    }
    serde_json::to_value(&cfg).map_err(|e| e.to_string())
}

/// Save settings from the UI back to config.toml.
/// If api_key is "***" (redacted sentinel), preserve the existing key.
#[tauri::command]
pub async fn save_config_ui(payload: serde_json::Value) -> Result<(), String> {
    let new_cfg: AppConfig = serde_json::from_value(payload).map_err(|e| e.to_string())?;
    // Load existing config to preserve redacted API keys
    let mut existing = load_config().unwrap_or_else(|_| new_cfg.clone());
    // Merge: copy all fields from new_cfg, but skip api_key="***"
    existing.llm.active_provider = new_cfg.llm.active_provider.clone();
    existing.llm.roles = new_cfg.llm.roles.clone();
    existing.safety = new_cfg.safety.clone();
    for (id, new_entry) in &new_cfg.llm.providers {
        if let Some(existing_entry) = existing.llm.providers.get_mut(id) {
            existing_entry.display_name = new_entry.display_name.clone();
            existing_entry.api_base = new_entry.api_base.clone();
            existing_entry.model = new_entry.model.clone();
            existing_entry.temperature = new_entry.temperature;
            // Only update api_key if it's not the redacted sentinel
            if new_entry.api_key.as_deref() != Some("***") {
                existing_entry.api_key = new_entry.api_key.clone();
            }
        } else {
            existing.llm.providers.insert(id.clone(), new_entry.clone());
        }
    }
    save_config(&existing).map_err(|e| e.to_string())
}
