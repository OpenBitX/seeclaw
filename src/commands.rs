use tauri::AppHandle;

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

/// Placeholder: start a task (will be replaced by real agent engine in Phase 3).
#[tauri::command]
pub async fn start_task(
    _app: AppHandle,
    task: String,
) -> Result<(), String> {
    tracing::info!(task = %task, "start_task invoked (stub)");
    Ok(())
}

/// Placeholder: stop the current task.
#[tauri::command]
pub async fn stop_task(_app: AppHandle) -> Result<(), String> {
    tracing::info!("stop_task invoked (stub)");
    Ok(())
}

/// Placeholder: confirm or deny a pending high-risk action.
#[tauri::command]
pub async fn confirm_action(
    _app: AppHandle,
    approved: bool,
) -> Result<(), String> {
    tracing::info!(approved = approved, "confirm_action invoked (stub)");
    Ok(())
}
