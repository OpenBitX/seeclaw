pub mod agent_engine;
pub mod commands;
pub mod config;
pub mod errors;
pub mod executor;
pub mod llm;
pub mod mcp;
pub mod perception;
pub mod rag;
pub mod skills;

use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use crate::agent_engine::engine::AgentEngine;
use crate::agent_engine::state::{AgentEvent, LoopConfig, LoopMode};
use crate::llm::registry::ProviderRegistry;

/// Handle passed to Tauri commands so they can send events into the agent loop.
pub struct AgentHandle {
    pub tx: mpsc::Sender<AgentEvent>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
        )
        .init();

    // Load .env file if present (ignore error if not found)
    let _ = dotenvy::dotenv();

    // Build the provider registry from config; fall back to an empty registry on error.
    let registry = match config::load_config() {
        Ok(cfg) => ProviderRegistry::from_config(&cfg),
        Err(e) => {
            tracing::error!(error = %e, "Failed to load config; starting with empty LLM registry");
            ProviderRegistry::new(String::new())
        }
    };
    let registry_state: Arc<Mutex<ProviderRegistry>> = Arc::new(Mutex::new(registry));

    // Create the agent event channel (buffer=32).
    let (agent_tx, agent_rx) = mpsc::channel::<AgentEvent>(32);
    let agent_handle = Arc::new(AgentHandle { tx: agent_tx });

    let loop_config = LoopConfig {
        mode: LoopMode::UntilDone,
        max_duration_minutes: None,
        max_failures: Some(5),
    };

    tauri::Builder::default()
        .manage(registry_state.clone())
        .manage(agent_handle)
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::get_version,
            commands::start_task,
            commands::stop_task,
            commands::confirm_action,
            commands::start_chat,
            commands::get_config,
            commands::save_config_ui,
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let registry_for_engine = registry_state.clone();
            tracing::info!("spawning AgentEngine background task");
            tauri::async_runtime::spawn(async move {
                let mut engine = AgentEngine::new(app_handle, loop_config, agent_rx, registry_for_engine);
                engine.run_loop().await;
                tracing::info!("AgentEngine task exited");
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running SeeClaw application");
}
