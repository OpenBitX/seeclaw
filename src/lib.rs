pub mod agent_engine;
pub mod commands;
pub mod errors;
pub mod executor;
pub mod llm;
pub mod mcp;
pub mod perception;
pub mod rag;
pub mod skills;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Load .env file if present (ignore error if not found)
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::get_version,
            commands::start_task,
            commands::stop_task,
            commands::confirm_action,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SeeClaw application");
}
