use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use crate::agent_engine::state::AgentEvent;
use crate::config::{load_config, save_config, get_config_path, AppConfig};
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

/// Get the path to the config file.
#[tauri::command]
pub async fn get_config_file_path() -> Result<String, String> {
    get_config_path().map_err(|e| e.to_string())
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
    tracing::info!("stop_task: signalling stop via atomic flag + channel");
    // Set the atomic flag FIRST — immediately visible to the engine even mid-operation
    handle
        .stop_flag
        .store(true, std::sync::atomic::Ordering::SeqCst);
    // Also send the channel event as backup for when the engine is blocked on recv()
    let _ = handle.tx.send(AgentEvent::Stop).await;
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
/// If api_key is empty in config.toml, populate from environment variable.
/// API keys are shown to allow editing (not redacted in settings UI).
/// Falls back to a default config if config.toml is missing (first-run scenario).
#[tauri::command]
pub async fn get_config() -> Result<serde_json::Value, String> {
    let mut cfg = load_config().unwrap_or_default();
    
    // Populate api_key from environment variables if not set in config
    for (id, entry) in cfg.llm.providers.iter_mut() {
        if entry.api_key.as_deref().map(|k| k.is_empty()).unwrap_or(true) {
            // Try to read from environment variable SEECLAW_{ID}_API_KEY
            let env_key = format!("SEECLAW_{}_API_KEY", id.to_uppercase());
            if let Ok(key) = std::env::var(&env_key) {
                if !key.is_empty() {
                    tracing::debug!(provider = id, "populated api_key from environment variable");
                    entry.api_key = Some(key);
                }
            }
        }
    }
    
    serde_json::to_value(&cfg).map_err(|e| e.to_string())
}

/// Save settings from the UI back to config.toml.
/// After saving, rebuilds the in-memory ProviderRegistry and emits
/// a "config_updated" event to the frontend for MobX sync.
#[tauri::command]
pub async fn save_config_ui(
    app: AppHandle,
    registry_state: State<'_, Arc<Mutex<ProviderRegistry>>>,
    payload: serde_json::Value,
) -> Result<(), String> {
    let new_cfg: AppConfig = serde_json::from_value(payload).map_err(|e| e.to_string())?;
    
    // Save the new config directly
    save_config(&new_cfg).map_err(|e| {
        tracing::error!(error = %e, "Failed to save config");
        e.to_string()
    })?;
    
    tracing::info!("Configuration saved successfully");

    // Rebuild in-memory registry so changes take effect immediately
    let new_registry = ProviderRegistry::from_config(&new_cfg);
    *registry_state.lock().await = new_registry;

    // Notify the frontend so MobX store can sync
    if let Err(e) = app.emit(
        "config_updated",
        serde_json::to_value(&new_cfg).unwrap_or_default(),
    ) {
        tracing::warn!("Failed to emit config_updated event: {e}");
    }

    Ok(())
}
