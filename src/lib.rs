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

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::flow::build_default_flow;
use crate::agent_engine::loop_control::LoopController;
use crate::agent_engine::state::{AgentEvent, GraphResult, LoopConfig, LoopMode, SharedState};
use crate::llm::registry::ProviderRegistry;
use crate::perception::yolo_detector::YoloDetector;

/// Handle passed to Tauri commands so they can send events into the agent loop.
pub struct AgentHandle {
    pub tx: mpsc::Sender<AgentEvent>,
    pub stop_flag: Arc<AtomicBool>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                // Default dev filter: 只对 seeclaw_lib 开 debug，其它库降噪
                tracing_subscriber::EnvFilter::new(
                    "seeclaw_lib=debug,tauri=info,reqwest=warn,hyper=warn",
                )
            }),
        )
        .init();

    // Load .env file if present (ignore error if not found)
    let _ = dotenvy::dotenv();

    // Build the provider registry from config; fall back to an empty registry on error.
    // Load config once; extract values needed by different subsystems.
    let (registry, perception_cfg) = match config::load_config() {
        Ok(cfg) => {
            let pcfg = cfg.perception.clone();
            (ProviderRegistry::from_config(&cfg), pcfg)
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to load config; starting with empty LLM registry");
            (ProviderRegistry::new(String::new()), config::PerceptionConfig::default())
        }
    };
    let registry_state: Arc<Mutex<ProviderRegistry>> = Arc::new(Mutex::new(registry));

    // Create the agent event channel (buffer=32).
    let (agent_tx, agent_rx) = mpsc::channel::<AgentEvent>(32);
    let stop_flag = Arc::new(AtomicBool::new(false));
    let agent_handle = Arc::new(AgentHandle { tx: agent_tx, stop_flag: stop_flag.clone() });

    let loop_config = LoopConfig {
        mode: LoopMode::UntilDone,
        max_duration_minutes: None,
        max_failures: Some(5),
    };

    // Try loading the YOLO detector model (non-critical: falls back to SoM grid)
    let yolo_detector = if perception_cfg.use_yolo {
        let class_names = if perception_cfg.class_names.is_empty() {
            crate::perception::yolo_detector::default_ui_class_names()
        } else {
            perception_cfg.class_names.clone()
        };
        YoloDetector::try_new(
            &perception_cfg.yolo_model_path,
            perception_cfg.confidence_threshold,
            perception_cfg.iou_threshold,
            class_names,
        )
    } else {
        None
    };

    tauri::Builder::default()
        .manage(registry_state.clone())
        .manage(agent_handle)
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::get_version,
            commands::get_config_file_path,
            commands::start_task,
            commands::stop_task,
            commands::confirm_action,
            commands::start_chat,
            commands::get_config,
            commands::save_config_ui,
        ])
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let registry_for_ctx = registry_state.clone();
            let stop_flag_for_ctx = stop_flag.clone();
            let perception_cfg_clone = perception_cfg.clone();

            tracing::info!("spawning Graph-based agent loop");
            tauri::async_runtime::spawn(async move {
                agent_loop(
                    app_handle,
                    agent_rx,
                    registry_for_ctx,
                    perception_cfg_clone,
                    yolo_detector,
                    loop_config,
                    stop_flag_for_ctx,
                )
                .await;
                tracing::info!("Agent loop task exited");
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running SeeClaw application");
}

/// Main agent loop: waits for GoalReceived events, then executes the graph.
async fn agent_loop(
    app: tauri::AppHandle,
    mut event_rx: mpsc::Receiver<AgentEvent>,
    registry: Arc<Mutex<ProviderRegistry>>,
    perception_cfg: config::PerceptionConfig,
    yolo_detector: Option<YoloDetector>,
    loop_config: LoopConfig,
    stop_flag: Arc<AtomicBool>,
) {
    use tauri::Emitter;

    // Build the graph once (topology is static)
    let graph = build_default_flow();

    // Build the node context (immutable resources)
    let ctx = NodeContext::new(
        app.clone(),
        registry,
        perception_cfg,
        yolo_detector,
        LoopController::new(loop_config),
    );

    // Goal buffered from a mid-task interruption (see forwarder logic below).
    let mut buffered_goal: Option<String> = None;

    loop {
        // Wait for a GoalReceived event, or consume one buffered from a
        // mid-task interruption (Bug 3 fix: new goals must not be lost).
        let goal = if let Some(g) = buffered_goal.take() {
            g
        } else {
            match event_rx.recv().await {
                Some(AgentEvent::GoalReceived(g)) => g,
                Some(AgentEvent::Stop) => {
                    tracing::info!("agent_loop: stop received while idle");
                    continue;
                }
                Some(_) => continue,
                None => {
                    tracing::info!("agent_loop: channel closed, exiting");
                    break;
                }
            }
        };

        tracing::info!(goal = %goal, "agent_loop: starting task");

        // Reset stop flag for new task
        stop_flag.store(false, std::sync::atomic::Ordering::SeqCst);

        // Reset loop controller
        {
            let mut ctrl = ctx.loop_ctrl.lock().await;
            ctrl.reset();
        }

        // Notify frontend — "routing" because the router node runs first
        let _ = app.emit("agent_state_changed", serde_json::json!({
            "state": "routing",
            "goal": &goal,
        }));

        // Create a new per-task channel for mid-task events (approve/reject/stop)
        let (task_tx, task_rx) = mpsc::channel::<AgentEvent>(32);

        // Shared slot for a goal that arrives while this task is still running.
        let pending_goal: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let pg = pending_goal.clone();
        let sf = stop_flag.clone();

        // Oneshot used to tell the forwarder "graph is done, stop waiting".
        // Without this the forwarder blocks forever on event_rx.recv() after a
        // normal (non-interrupted) task completion, and the "done" event is
        // never emitted to the frontend.
        let (fwd_stop_tx, mut fwd_stop_rx) = tokio::sync::oneshot::channel::<()>();

        let forwarder = tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Graph signalled us to stop (task completed normally)
                    _ = &mut fwd_stop_rx => break,

                    evt = event_rx.recv() => {
                        let Some(evt) = evt else { break };
                        match evt {
                            // New goal mid-execution: store it, interrupt current task.
                            AgentEvent::GoalReceived(new_goal) => {
                                *pg.lock().await = Some(new_goal);
                                sf.store(true, std::sync::atomic::Ordering::SeqCst);
                                let _ = task_tx.send(AgentEvent::Stop).await;
                                break;
                            }
                            other => {
                                let should_break = matches!(other, AgentEvent::Stop);
                                let _ = task_tx.send(other).await;
                                if should_break {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            event_rx
        });

        // Build per-task SharedState
        let mut state = SharedState::new(goal.clone(), stop_flag.clone(), task_rx);

        // Run the graph
        let result = graph.run(&mut state, &ctx).await;

        // Signal the forwarder to exit (it may be blocked on recv()).
        // Any events already in event_rx are untouched and will be read next iteration.
        let _ = fwd_stop_tx.send(());

        // Reclaim the event_rx from the forwarder
        event_rx = match forwarder.await {
            Ok(rx) => rx,
            Err(_) => {
                tracing::error!("agent_loop: forwarder panicked, cannot continue");
                break;
            }
        };

        // Recover goal that arrived mid-task (if any), to process on next iteration.
        buffered_goal = pending_goal.lock().await.take();

        // Report result (skip if we were interrupted by a new goal)
        if buffered_goal.is_none() {
            match result {
                Ok(()) => {
                    let summary = match &state.result {
                        Some(GraphResult::Done { summary }) => summary.clone(),
                        Some(GraphResult::Error { message }) => format!("Error: {message}"),
                        None => "Task completed.".to_string(),
                    };
                    tracing::info!(summary = %summary, "agent_loop: task finished");
                    let _ = app.emit("agent_state_changed", serde_json::json!({
                        "state": "done",
                        "summary": summary,
                    }));
                }
                Err(e) => {
                    tracing::error!(error = %e, "agent_loop: graph execution failed");
                    let _ = app.emit("agent_state_changed", serde_json::json!({
                        "state": "error",
                        "message": e,
                    }));
                }
            }
        } else {
            tracing::info!("agent_loop: task interrupted by new goal, picking up immediately");
        }
    }
}

