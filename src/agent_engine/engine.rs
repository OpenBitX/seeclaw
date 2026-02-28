use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine as _;
use tauri::{AppHandle, Emitter, Wry};
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex};

use crate::agent_engine::history::{HistoryEntry, SessionHistory};
use crate::agent_engine::loop_control::LoopController;
use crate::agent_engine::state::{AgentAction, AgentEvent, AgentState, ActionResult, LoopConfig, TodoStep};
use crate::config::PerceptionConfig;
use crate::executor::input;
use crate::llm::registry::ProviderRegistry;
use crate::llm::tools::load_builtin_tools;
use crate::llm::types::{ChatMessage, ContentPart, ImageUrl, MessageContent, StreamChunk, StreamChunkKind, ToolCall};
use crate::perception::annotator;
use crate::perception::screenshot::capture_primary;
use crate::perception::som_grid::{col_label, draw_som_grid, grid_cell_to_physical, parse_grid_label};
use crate::perception::types::{ScreenshotMeta, UIElement};
use crate::perception::yolo_detector::YoloDetector;

const PLANNER_SYSTEM: &str = include_str!("../../prompts/system/tools_agent.md");
const VLM_PROMPT_TEMPLATE: &str = include_str!("../../prompts/system/vlm_grid.md");
const VLM_ANNOTATED_TEMPLATE: &str = include_str!("../../prompts/system/vlm_annotated.md");

pub struct AgentEngine {
    state: AgentState,
    event_rx: mpsc::Receiver<AgentEvent>,
    loop_ctrl: LoopController,
    history: SessionHistory,
    app: AppHandle<Wry>,
    registry: Arc<Mutex<ProviderRegistry>>,
    /// Grid resolution loaded from config (rows = cols = grid_n).
    grid_n: u32,
    /// Perception configuration.
    perception_cfg: PerceptionConfig,
    /// YOLO detector (None if model file missing).
    yolo_detector: Option<YoloDetector>,

    // ── Conversation context (reset per goal) ─────────────────────────────
    conv_messages: Vec<ChatMessage>,
    current_goal: String,
    last_meta: Option<ScreenshotMeta>,
    pending_tool_id: String,
    /// Most recently detected elements — used to resolve element_id → bbox.
    detected_elements: Vec<UIElement>,

    // ── Stop / cancellation ───────────────────────────────────────────────
    /// Shared atomic flag set by `stop_task` command for immediate cancellation.
    stop_flag: Arc<AtomicBool>,

    // ── Todo list state ───────────────────────────────────────────────────
    todo_steps: Vec<TodoStep>,
    current_step_idx: usize,
    /// How many full plan→execute→evaluate cycles have run (anti-loop guard).
    cycle_count: u32,
    /// Accumulated step results for the evaluator.
    steps_log: Vec<String>,
}

impl AgentEngine {
    pub fn new(
        app: AppHandle<Wry>,
        loop_config: LoopConfig,
        event_rx: mpsc::Receiver<AgentEvent>,
        registry: Arc<Mutex<ProviderRegistry>>,
        perception_cfg: PerceptionConfig,
        stop_flag: Arc<AtomicBool>,
    ) -> Self {
        // Try to initialise YOLO detector
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

        Self {
            state: AgentState::Idle,
            event_rx,
            loop_ctrl: LoopController::new(loop_config),
            history: SessionHistory::new(),
            app,
            registry,
            grid_n: perception_cfg.grid_n.clamp(4, 26),
            perception_cfg,
            yolo_detector,
            conv_messages: Vec::new(),
            current_goal: String::new(),
            last_meta: None,
            pending_tool_id: String::new(),
            detected_elements: Vec::new(),
            stop_flag,
            todo_steps: Vec::new(),
            current_step_idx: 0,
            cycle_count: 0,
            steps_log: Vec::new(),
        }
    }

    /// Emit a lightweight activity label to the frontend for progress feedback.
    fn emit_activity(&self, text: &str) {
        let _ = self.app.emit("agent_activity", serde_json::json!({ "text": text }));
    }

    /// Check whether the stop flag has been set by the UI.
    fn is_stopped(&self) -> bool {
        self.stop_flag.load(Ordering::Relaxed)
    }

    /// Hard-reset the engine to Idle after a user-requested stop.
    /// Clears all in-flight state, drains stale Stop events from the channel,
    /// and notifies the frontend.
    fn reset_for_stop(&mut self) {
        tracing::info!("stop requested → resetting engine to Idle");
        self.stop_flag.store(false, Ordering::SeqCst);

        // Drain any stale Stop events so the Idle handler doesn't see them.
        // (GoalReceived / other events are kept — if one snuck in, we'll lose it,
        // but that's extremely unlikely during a stop.)
        loop {
            match self.event_rx.try_recv() {
                Ok(AgentEvent::Stop) => continue,
                Ok(_other) => {
                    // Non-stop event — in practice shouldn't happen during stop
                    tracing::debug!("draining non-stop event during reset");
                    continue;
                }
                Err(_) => break,
            }
        }

        // Close any open streaming message on the frontend
        let _ = self.app.emit("llm_stream_chunk", &StreamChunk {
            kind: StreamChunkKind::Done,
            content: String::new(),
        });

        // Emit a user-visible message that the task was cancelled
        let _ = self.app.emit("llm_stream_chunk", &StreamChunk {
            kind: StreamChunkKind::Content,
            content: "⏹ 任务已被用户终止。".to_string(),
        });
        let _ = self.app.emit("llm_stream_chunk", &StreamChunk {
            kind: StreamChunkKind::Done,
            content: String::new(),
        });

        // Clear all in-flight state
        self.conv_messages.clear();
        self.current_goal.clear();
        self.todo_steps.clear();
        self.current_step_idx = 0;
        self.cycle_count = 0;
        self.steps_log.clear();
        self.pending_tool_id.clear();
        self.detected_elements.clear();
        self.last_meta = None;
        self.loop_ctrl.reset();

        self.state = AgentState::Idle;
        let _ = self.app.emit("agent_state_changed", &self.state);
    }

    /// Helper future that resolves once the stop flag becomes true.
    /// Used with `tokio::select!` to abort long-running LLM calls.
    async fn poll_stop(flag: Arc<AtomicBool>) {
        loop {
            if flag.load(Ordering::Relaxed) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    pub async fn run_loop(&mut self) {
        loop {
            // ── Immediate stop check at the top of every iteration ──────
            if self.is_stopped() {
                self.reset_for_stop();
                continue;
            }

            if let Err(e) = self.app.emit("agent_state_changed", &self.state) {
                tracing::warn!("emit agent_state_changed failed: {e}");
            }

            if self.loop_ctrl.should_stop() {
                tracing::info!("loop controller triggered stop");
                self.state = AgentState::Done { summary: "Loop limit reached".into() };
                let _ = self.app.emit("agent_state_changed", &self.state);
                break;
            }

            match self.state.clone() {
                // 鈹€鈹€ Idle: wait for a new goal 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€
                AgentState::Idle => {
                    match self.event_rx.recv().await {
                        Some(AgentEvent::GoalReceived(goal)) => {
                            tracing::info!(goal = %goal, "goal received → Planning");
                            // Clear stop flag in case it was set from a previous stop
                            self.stop_flag.store(false, Ordering::SeqCst);
                            self.current_goal = goal.clone();
                            self.last_meta = None;
                            self.pending_tool_id.clear();
                            self.detected_elements.clear();
                            self.todo_steps.clear();
                            self.current_step_idx = 0;
                            self.cycle_count = 0;
                            self.steps_log.clear();

                            self.conv_messages = vec![
                                ChatMessage {
                                    role: "system".into(),
                                    content: MessageContent::Text(PLANNER_SYSTEM.into()),
                                    tool_call_id: None,
                                    tool_calls: None,
                                },
                                ChatMessage {
                                    role: "user".into(),
                                    content: MessageContent::Text(goal.clone()),
                                    tool_call_id: None,
                                    tool_calls: None,
                                },
                            ];

                            self.history.push(HistoryEntry {
                                ts: chrono::Utc::now().timestamp_millis(),
                                role: "user".into(),
                                content: Some(goal.clone()),
                                action: None,
                            });
                            let _ = self.history.flush();
                            self.state = AgentState::Planning { goal };
                        }
                        Some(AgentEvent::Stop) => {
                            // Stop received while already idle — just ignore
                            tracing::debug!("Stop received while Idle, ignoring");
                        }
                        None => break, // Channel closed → app shutting down
                        _ => {}
                    }
                }

                // 鈹€鈹€ Routing: pass-through (reserved for future intent routing) 鈹€鈹€
                AgentState::Routing { goal } => {
                    self.state = AgentState::Planning { goal };
                }

                // 鈹€鈹€ Planning: ask planner to produce a todo list 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€
                AgentState::Planning { goal } => {
                    tracing::info!(goal = %goal, cycle = self.cycle_count, "Planning → calling planner LLM");
                    self.emit_activity("正在规划任务步骤…");
                    self.cycle_count += 1;

                    match self.call_planner().await {
                        Ok(()) => {
                            // After call_planner, state is set internally
                        }
                        Err(e) if self.is_stopped() => {
                            // Stopped by user — the loop top will handle the reset
                            tracing::info!("planner aborted by user stop");
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "planner LLM failed");
                            self.state = AgentState::Error { message: e.to_string() };
                        }
                    }
                }

                // 鈹€鈹€ Executing: run one step 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€
                AgentState::Executing { action } => {
                    tracing::info!(?action, step = self.current_step_idx, "Executing step");
                    self.execute_action(action).await;
                }

                // 鈹€鈹€ WaitingForUser: human-in-the-loop approval 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€
                AgentState::WaitingForUser { pending_action } => {
                    tracing::info!(?pending_action, "waiting for user approval");
                    match self.event_rx.recv().await {
                        Some(AgentEvent::UserApproved) => {
                            self.state = AgentState::Executing { action: pending_action };
                        }
                        Some(AgentEvent::UserRejected) | Some(AgentEvent::Stop) | None => {
                            tracing::info!("user rejected / stop 鈫?Idle");
                            self.state = AgentState::Idle;
                        }
                        _ => {}
                    }
                }

                // ── Evaluating: planner self-evaluates after all steps done ──
                AgentState::Evaluating { goal, steps_summary } => {
                    tracing::info!(goal = %goal, "Evaluating completion");
                    self.emit_activity("正在评估任务完成度…");
                    match self.call_evaluator(&goal, &steps_summary).await {
                        Ok(()) => {}
                        Err(e) if self.is_stopped() => {
                            tracing::info!("evaluator aborted by user stop");
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "evaluator LLM failed");
                            self.state = AgentState::Error { message: e.to_string() };
                        }
                    }
                }

                AgentState::Done { ref summary } => {
                    tracing::info!(summary = %summary, "task done → returning to Idle");
                    // Stay in Done briefly so the frontend sees the final state,
                    // then reset to Idle to accept the next goal.
                    self.loop_ctrl.reset();
                    self.state = AgentState::Idle;
                }
                AgentState::Error { ref message } => {
                    tracing::warn!(error = %message, "task error → returning to Idle");
                    self.loop_ctrl.reset();
                    self.state = AgentState::Idle;
                }
            }

            tokio::task::yield_now().await;
        }
        tracing::info!(session = %self.history.session_id, "agent loop ended");
    }

    // 鈹€鈹€ Planner: generate todo list then execute steps 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

    async fn call_planner(&mut self) -> Result<(), String> {
        if self.is_stopped() { return Err("Stopped by user".into()); }

        let tools = load_builtin_tools().map_err(|e| e.to_string())?;
        let messages = self.conv_messages.clone();

        let (provider, cfg) = {
            let reg = self.registry.lock().await;
            reg.call_config_for_role("tools").map_err(|e| e.to_string())?
        };

        // Race the LLM call against the stop flag for immediate cancellation
        let flag = self.stop_flag.clone();
        let response = tokio::select! {
            result = provider.chat(messages, tools, &cfg, &self.app) => {
                result.map_err(|e| e.to_string())?
            }
            _ = Self::poll_stop(flag) => {
                return Err("Stopped by user".into());
            }
        };

        if self.is_stopped() { return Err("Stopped by user".into()); }

        if cfg!(debug_assertions) {
            tracing::debug!(
                content = %response.content,
                tools = ?response.tool_calls.iter()
                    .map(|tc| (&tc.function.name, &tc.function.arguments))
                    .collect::<Vec<_>>(),
                "planner response"
            );
        }

        if let Some(tc) = response.tool_calls.into_iter().next() {
            // Append assistant message with tool call
            self.conv_messages.push(ChatMessage {
                role: "assistant".into(),
                content: MessageContent::Text(response.content.clone()),
                tool_call_id: None,
                tool_calls: Some(vec![tc.clone()]),
            });
            self.pending_tool_id = tc.id.clone();

            match parse_tool_call_to_action(&tc) {
                Ok(action) => {
                    tracing::info!(tool = %tc.function.name, "planner dispatched tool");

                    // plan_task is handled specially: parse steps and start ticking
                    if let AgentAction::PlanTask { ref steps } = action {
                        self.todo_steps = steps.clone();
                        self.current_step_idx = 0;
                        self.steps_log.clear();
                        tracing::info!(steps = steps.len(), "todo list created");

                        // Ack the plan_task tool call
                        self.conv_messages.push(ChatMessage {
                            role: "tool".into(),
                            content: MessageContent::Text(format!(
                                "Plan accepted: {} steps.",
                                steps.len()
                            )),
                            tool_call_id: Some(self.pending_tool_id.clone()),
                            tool_calls: None,
                        });

                        self.advance_to_next_step().await;
                        return Ok(());
                    }

                    // evaluate_completion is also handled specially
                    if let AgentAction::EvaluateCompletion { .. } = action {
                        self.handle_evaluate_completion_tool(&tc).await;
                        return Ok(());
                    }

                    // finish_task / report_failure
                    if matches!(action, AgentAction::FinishTask { .. } | AgentAction::ReportFailure { .. }) {
                        self.state = AgentState::Executing { action };
                        return Ok(());
                    }

                    // Any other direct action (e.g. execute_terminal without needing viewport)
                    if is_auto_approved(&action) {
                        self.state = AgentState::Executing { action };
                    } else {
                        let req = serde_json::json!({
                            "id": &tc.id,
                            "action": serde_json::to_value(&action).unwrap_or_default(),
                            "reason": format!("鎵ц: {}", tc.function.name),
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                        });
                        let _ = self.app.emit("action_required", &req);
                        self.state = AgentState::WaitingForUser { pending_action: action };
                    }
                }
                Err(e) => {
                    // Unknown tool 鈥?inject an error message back into conversation
                    // so the planner can self-correct on the next turn instead of silently dying
                    tracing::warn!(error = %e, tool = %tc.function.name, "unrecognised tool call 鈥?injecting error feedback");
                    self.conv_messages.push(ChatMessage {
                        role: "tool".into(),
                        content: MessageContent::Text(format!(
                            "Error: unknown tool '{}'. Please call plan_task or one of the registered tools.",
                            tc.function.name
                        )),
                        tool_call_id: Some(tc.id.clone()),
                        tool_calls: None,
                    });
                    // Re-enter Planning so the model can recover
                    self.state = AgentState::Planning { goal: self.current_goal.clone() };
                }
            }
        } else {
            // Content-only response 鈥?treat as done
            tracing::info!("planner content-only response 鈫?Idle");
            self.state = AgentState::Idle;
        }

        Ok(())
    }

    /// Advance to the next pending step, or move to Evaluating if all steps done.
    async fn advance_to_next_step(&mut self) {
        // Bail out immediately if stop was requested
        if self.is_stopped() { return; }

        if self.current_step_idx >= self.todo_steps.len() {
            // All steps done 鈫?self-evaluate
            let summary = self.steps_log.join("\n");
            self.state = AgentState::Evaluating {
                goal: self.current_goal.clone(),
                steps_summary: summary,
            };
            return;
        }

        // Inter-step delay: give the OS time to process the previous UI action
        // (e.g. Win+S needs ~500ms before the search box is ready for input).
        if self.current_step_idx > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        let step = self.todo_steps[self.current_step_idx].clone();
        tracing::info!(
            step = step.index,
            desc = %step.description,
            needs_viewport = step.needs_viewport,
            "advancing to step"
        );

        if step.needs_viewport {
            // Need to see the screen first 鈥?take screenshot and ask VLM
            if action_supports_element_id(&step.action) {
                match self.call_vlm_for_step(&step).await {
                    Ok(Some(cell)) => {
                        // VLM found the element 鈥?patch the action's element_id
                        let action = patch_element_id(step.action.clone(), &cell);
                        self.dispatch_step_action(action).await;
                    }
                    Ok(None) => {
                        // VLM couldn't find it
                        let msg = format!(
                            "Step {}: VLM could not locate '{}' on screen",
                            step.index,
                            step.target.as_deref().unwrap_or("target")
                        );
                        tracing::warn!("{}", msg);
                        self.steps_log.push(format!("FAIL: {msg}"));
                        self.loop_ctrl.record_failure();
                        self.current_step_idx += 1;
                        // Continue to next step rather than aborting
                        Box::pin(self.advance_to_next_step()).await;
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "VLM call failed");
                        self.state = AgentState::Error { message: e };
                    }
                }
            } else {
                tracing::info!(step = step.index, "skipping VLM call as action does not support element targeting");
                self.dispatch_step_action(step.action.clone()).await;
            }
        } else {
            self.dispatch_step_action(step.action.clone()).await;
        }
    }

    async fn dispatch_step_action(&mut self, action: AgentAction) {
        if is_auto_approved(&action) {
            self.state = AgentState::Executing { action };
        } else {
            let req = serde_json::json!({
                "id": format!("step-{}", self.current_step_idx),
                "action": serde_json::to_value(&action).unwrap_or_default(),
                "reason": format!("姝ラ {}", self.current_step_idx + 1),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            let _ = self.app.emit("action_required", &req);
            self.state = AgentState::WaitingForUser { pending_action: action };
        }
    }

    // ── VLM: locate element in screenshot ──────────────────────────────────

    /// Look up a detected element by its ID (e.g. "btn_1", "icon_3").
    fn find_element_by_id(&self, id: &str) -> Option<&UIElement> {
        self.detected_elements.iter().find(|e| e.id == id)
    }

    /// Capture screenshot, run perception pipeline, send to VLM, return element ID or grid cell.
    async fn call_vlm_for_step(&mut self, step: &TodoStep) -> Result<Option<String>, String> {
        if self.is_stopped() { return Err("Stopped by user".into()); }

        let target = step.target.as_deref().unwrap_or(&step.description);

        self.emit_activity("正在截取屏幕…");
        let shot = capture_primary().await.map_err(|e| e.to_string())?;
        self.last_meta = Some(shot.meta.clone());

        self.emit_activity("正在分析屏幕元素…");

        // ── Try YOLO + UIA annotation pipeline ──────────────────────────
        let use_annotated = self.yolo_detector.is_some() || self.perception_cfg.enable_ui_automation;

        if use_annotated {
            // Run YOLO detection (blocking)
            let mut elements = if let Some(ref mut detector) = self.yolo_detector {
                match detector.detect(&shot.image_bytes) {
                    Ok(elems) => {
                        tracing::info!(count = elems.len(), "YOLO detections");
                        elems
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "YOLO inference failed");
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            };

            // Merge with UIA
            if self.perception_cfg.enable_ui_automation {
                match crate::perception::ui_automation::collect_ui_elements(&shot.meta).await {
                    Ok(uia) => {
                        tracing::debug!(count = uia.len(), "UIA elements");
                        crate::perception::ui_automation::merge_detections(&mut elements, uia, 0.3);
                    }
                    Err(e) => tracing::warn!(error = %e, "UIA failed"),
                }
            }

            if !elements.is_empty() {
                // Annotate image
                let annotated_bytes = crate::perception::annotator::annotate_image(
                    &shot.image_bytes, &elements,
                ).map_err(|e| e.to_string())?;
                let annotated_b64 = base64::engine::general_purpose::STANDARD.encode(&annotated_bytes);

                // Store detected elements for later use in execute_action
                self.detected_elements = elements.clone();

                // Emit to frontend
                let _ = self.app.emit("viewport_captured", serde_json::json!({
                    "image_base64": &annotated_b64,
                    "grid_n": 0,
                    "physical_width": shot.meta.physical_width,
                    "physical_height": shot.meta.physical_height,
                    "source": "yolo_annotated",
                    "element_count": elements.len(),
                }));

                // Build VLM prompt with element list
                let element_list = annotator::build_element_list(&elements);
                let vlm_prompt = VLM_ANNOTATED_TEMPLATE
                    .replace("{element_list}", &element_list)
                    .replace("{target}", target);

                let data_url = format!("data:image/png;base64,{}", annotated_b64);

                return self.call_vlm_with_image(&data_url, &vlm_prompt, true).await;
            }
        }

        // ── Fallback: SoM Grid ──────────────────────────────────────────
        tracing::info!("Using SoM grid fallback");
        self.detected_elements.clear();

        let grid_bytes = draw_som_grid(&shot.image_bytes, self.grid_n)
            .unwrap_or(shot.image_bytes.clone());
        let grid_b64 = base64::engine::general_purpose::STANDARD.encode(&grid_bytes);

        let _ = self.app.emit("viewport_captured", serde_json::json!({
            "image_base64": grid_b64,
            "grid_n": self.grid_n,
            "physical_width": shot.meta.physical_width,
            "physical_height": shot.meta.physical_height,
            "source": "som_grid",
        }));

        let last_col = col_label(self.grid_n - 1);
        let vlm_prompt = VLM_PROMPT_TEMPLATE
            .replace("{grid_n}", &self.grid_n.to_string())
            .replace("{last_col}", &last_col)
            .replace("{target}", target);

        let data_url = format!("data:image/png;base64,{}", grid_b64);

        self.call_vlm_with_image(&data_url, &vlm_prompt, false).await
    }

    /// Send an image + prompt to the VLM and parse the response.
    /// `is_annotated`: true = parse element_id, false = parse cell label.
    async fn call_vlm_with_image(
        &self,
        data_url: &str,
        vlm_prompt: &str,
        is_annotated: bool,
    ) -> Result<Option<String>, String> {
        let vlm_messages = vec![
            ChatMessage {
                role: "user".into(),
                content: MessageContent::Parts(vec![
                    ContentPart::ImageUrl { image_url: ImageUrl { url: data_url.to_string() } },
                    ContentPart::Text { text: vlm_prompt.to_string() },
                ]),
                tool_call_id: None,
                tool_calls: None,
            },
        ];

        let (provider, mut cfg) = {
            let reg = self.registry.lock().await;
            reg.call_config_for_role("vision").map_err(|e| e.to_string())?
        };
        cfg.silent = true;

        // Race the VLM call against the stop flag
        let flag = self.stop_flag.clone();
        let response = tokio::select! {
            result = provider.chat(vlm_messages, vec![], &cfg, &self.app) => {
                result.map_err(|e| e.to_string())?
            }
            _ = Self::poll_stop(flag) => {
                return Err("Stopped by user".into());
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(45)) => {
                return Err("VLM call timed out after 45s".into());
            }
        };

        if self.is_stopped() { return Err("Stopped by user".into()); }

        tracing::debug!(vlm_raw = %response.content, "VLM response");

        let raw = response.content.trim();
        let json_str = raw
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        match serde_json::from_str::<serde_json::Value>(json_str) {
            Ok(v) => {
                let found = v["found"].as_bool().unwrap_or(false);
                let desc = v["description"].as_str().unwrap_or("").to_string();

                if is_annotated {
                    let element_id = v["element_id"].as_str().map(|s| s.to_string());
                    tracing::info!(found, element_id = ?element_id, desc = %desc, "VLM annotated result");
                    if found { Ok(element_id) } else { Ok(None) }
                } else {
                    let cell = v["cell"].as_str().map(|s| s.to_string());
                    tracing::info!(found, cell = ?cell, desc = %desc, "VLM grid result");
                    if found { Ok(cell) } else { Ok(None) }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, raw = %json_str, "VLM JSON parse failed");
                Ok(extract_cell_label_from_text(raw))
            }
        }
    }

    // 鈹€鈹€ Evaluator: self-evaluate after all steps 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

    async fn call_evaluator(&mut self, goal: &str, steps_summary: &str) -> Result<(), String> {
        if self.is_stopped() { return Err("Stopped by user".into()); }

        // Anti-loop guard: max 3 cycles
        if self.cycle_count > 3 {
            tracing::warn!("max cycles reached 鈫?forcing finish");
            self.state = AgentState::Done {
                summary: format!("Reached max retry cycles. Last steps:\n{steps_summary}"),
            };
            return Ok(());
        }

        let eval_prompt = format!(
            "Goal: {goal}\n\nCompleted steps:\n{steps_summary}\n\n\
             Did you fully achieve the goal? \
             If yes, call `finish_task` with a summary. \
             If not, call `plan_task` with a revised plan (max 3 retries total, this is cycle {}).",
            self.cycle_count
        );

        self.conv_messages.push(ChatMessage {
            role: "user".into(),
            content: MessageContent::Text(eval_prompt),
            tool_call_id: None,
            tool_calls: None,
        });

        // Reuse planner call 鈥?it will either finish_task or plan_task again
        self.call_planner().await
    }

    async fn handle_evaluate_completion_tool(&mut self, tc: &ToolCall) {
        let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
            .unwrap_or(serde_json::json!({}));
        let completed = args["completed"].as_bool().unwrap_or(false);
        let summary = args["summary"].as_str().unwrap_or("").to_string();

        self.conv_messages.push(ChatMessage {
            role: "tool".into(),
            content: MessageContent::Text(format!("Evaluation recorded: completed={completed}")),
            tool_call_id: Some(self.pending_tool_id.clone()),
            tool_calls: None,
        });

        if completed {
            self.state = AgentState::Done { summary };
        } else if self.cycle_count <= 3 {
            // Retry: go back to planning
            self.state = AgentState::Planning { goal: self.current_goal.clone() };
        } else {
            self.state = AgentState::Done {
                summary: format!("Could not complete after 3 cycles: {summary}"),
            };
        }
    }

    // 鈹€鈹€ Action execution 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

    async fn execute_action(&mut self, action: AgentAction) {
        // Bail out immediately if stop was requested
        if self.is_stopped() { return; }

        // Emit fine-grained activity for the current action
        let activity_label = match &action {
            AgentAction::MouseClick { element_id } => format!("正在点击 {element_id}…"),
            AgentAction::MouseDoubleClick { element_id } => format!("正在双击 {element_id}…"),
            AgentAction::MouseRightClick { element_id } => format!("正在右键点击 {element_id}…"),
            AgentAction::TypeText { text, .. } => {
                let preview: String = text.chars().take(20).collect();
                format!("正在输入: {preview}…")
            }
            AgentAction::Hotkey { keys } => format!("正在按下快捷键: {keys}"),
            AgentAction::KeyPress { key } => format!("正在按键: {key}"),
            AgentAction::Wait { milliseconds } => format!("等待 {milliseconds}ms…"),
            AgentAction::ExecuteTerminal { command, .. } => {
                let preview: String = command.chars().take(30).collect();
                format!("正在执行命令: {preview}…")
            }
            AgentAction::Scroll { direction, .. } => format!("正在滚动({direction})…"),
            AgentAction::FinishTask { .. } => "正在完成任务…".to_string(),
            AgentAction::ReportFailure { .. } => "正在报告结果…".to_string(),
            _ => "正在执行操作…".to_string(),
        };
        self.emit_activity(&activity_label);

        let (ok, msg) = match action.clone() {
            AgentAction::MouseClick { ref element_id }
            | AgentAction::MouseDoubleClick { ref element_id }
            | AgentAction::MouseRightClick { ref element_id } => {
                let is_double = matches!(action, AgentAction::MouseDoubleClick { .. });
                let is_right = matches!(action, AgentAction::MouseRightClick { .. });
                if let Some(meta) = &self.last_meta {
                    // Try 1: look up element by ID from YOLO/UIA detections
                    let coords = self.find_element_by_id(element_id)
                        .map(|elem| elem.center_physical(meta));

                    // Try 2: parse as grid cell label (SoM grid fallback)
                    let coords = coords.or_else(|| {
                        parse_grid_label(element_id)
                            .map(|(col, row)| grid_cell_to_physical(col, row, meta.physical_width, meta.physical_height, self.grid_n))
                    });

                    if let Some((px, py)) = coords {
                        let result = if is_right {
                            input::mouse_right_click(px, py).await
                        } else if is_double {
                            input::mouse_double_click(px, py).await
                        } else {
                            input::mouse_click(px, py).await
                        };
                        match result {
                            Ok(()) => (true, format!("Clicked {element_id} at ({px},{py})")),
                            Err(e) => (false, format!("Click failed: {e}")),
                        }
                    } else {
                        (false, format!("Cannot resolve element: {element_id}"))
                    }
                } else {
                    (false, "No viewport — call get_viewport first".into())
                }
            }

            AgentAction::TypeText { ref text, clear_first } => {
                match input::type_text(text.clone(), clear_first).await {
                    Ok(()) => (true, format!("Typed: {text}")),
                    Err(e) => (false, format!("TypeText failed: {e}")),
                }
            }

            AgentAction::Hotkey { ref keys } => {
                match input::press_hotkey(keys.clone()).await {
                    Ok(()) => (true, format!("Hotkey: {keys}")),
                    Err(e) => (false, format!("Hotkey failed: {e}")),
                }
            }

            AgentAction::KeyPress { ref key } => {
                match input::press_hotkey(key.clone()).await {
                    Ok(()) => (true, format!("KeyPress: {key}")),
                    Err(e) => (false, format!("KeyPress failed: {e}")),
                }
            }

            AgentAction::Wait { milliseconds } => {
                // Make wait interruptible by the stop flag
                let flag = self.stop_flag.clone();
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_millis(milliseconds as u64)) => {}
                    _ = Self::poll_stop(flag) => {
                        return; // stop requested, bail out
                    }
                }
                (true, format!("Waited {milliseconds}ms"))
            }

            AgentAction::ExecuteTerminal { ref command, ref reason } => {
                tracing::info!(%command, %reason, "executing terminal command");
                // Spawn the child process so we can kill it on stop
                match Command::new("powershell")
                    .arg("-NoProfile")
                    .arg("-Command")
                    .arg(command)
                    .kill_on_drop(true)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                {
                    Ok(child) => {
                        // Race child completion against the stop flag
                        let flag = self.stop_flag.clone();
                        let output = tokio::select! {
                            result = child.wait_with_output() => result,
                            _ = Self::poll_stop(flag) => {
                                // child is dropped here; kill_on_drop(true) handles cleanup
                                return;
                            }
                        };
                        match output {
                            Ok(out) => {
                                let mut buf = String::new();
                                if !out.stdout.is_empty() { buf.push_str(&String::from_utf8_lossy(&out.stdout)); }
                                if !out.stderr.is_empty() {
                                    if !buf.is_empty() { buf.push_str("\n--- STDERR ---\n"); }
                                    buf.push_str(&String::from_utf8_lossy(&out.stderr));
                                }
                                let truncated = if buf.len() > 4000 {
                                    format!("{}\n[truncated]", &buf[..4000])
                                } else { buf };
                                let ok = out.status.success();
                                (ok, format!("command: {command}\noutput:\n{truncated}"))
                            }
                            Err(e) => (false, format!("wait failed: {e}")),
                        }
                    }
                    Err(e) => (false, format!("spawn failed: {e}")),
                }
            }

            AgentAction::FinishTask { ref summary } => {
                tracing::info!(summary = %summary, "task finished");
                // Emit the completion summary so the user sees a final reply
                let _ = self.app.emit("llm_stream_chunk", &StreamChunk {
                    kind: StreamChunkKind::Content,
                    content: summary.clone(),
                });
                let _ = self.app.emit("llm_stream_chunk", &StreamChunk {
                    kind: StreamChunkKind::Done,
                    content: String::new(),
                });
                self.conv_messages.push(ChatMessage {
                    role: "tool".into(),
                    content: MessageContent::Text(format!("Task complete: {summary}")),
                    tool_call_id: Some(self.pending_tool_id.clone()),
                    tool_calls: None,
                });
                self.state = AgentState::Done { summary: summary.clone() };
                return;
            }

            AgentAction::ReportFailure { ref reason, .. } => {
                tracing::warn!(reason = %reason, "task failure reported");
                // Emit failure message so the user sees what went wrong
                let fail_msg = format!("Task failed: {}", reason);
                let _ = self.app.emit("llm_stream_chunk", &StreamChunk {
                    kind: StreamChunkKind::Content,
                    content: fail_msg,
                });
                let _ = self.app.emit("llm_stream_chunk", &StreamChunk {
                    kind: StreamChunkKind::Done,
                    content: String::new(),
                });
                self.state = AgentState::Error { message: reason.clone() };
                return;
            }

            AgentAction::EvaluateCompletion { completed, ref summary } => {
                tracing::info!(completed, summary = %summary, "EvaluateCompletion step executed");
                if completed {
                    // Task done
                    let _ = self.app.emit("llm_stream_chunk", &StreamChunk {
                        kind: StreamChunkKind::Content,
                        content: summary.clone(),
                    });
                    let _ = self.app.emit("llm_stream_chunk", &StreamChunk {
                        kind: StreamChunkKind::Done,
                        content: String::new(),
                    });
                    self.conv_messages.push(ChatMessage {
                        role: "tool".into(),
                        content: MessageContent::Text(format!("Task complete: {summary}")),
                        tool_call_id: Some(self.pending_tool_id.clone()),
                        tool_calls: None,
                    });
                    self.state = AgentState::Done { summary: summary.clone() };
                    return;
                } else {
                    (true, format!("Evaluation: completed=false, summary={}", summary))
                }
            }

            // GetViewport called directly (model bypassed plan_task) — take screenshot,
            // inject it into conversation, and re-enter Planning so the model can proceed.
            AgentAction::GetViewport { .. } => {
                tracing::warn!("get_viewport called directly — capturing and injecting into conversation");
                match capture_primary().await {
                    Ok(shot) => {
                        self.last_meta = Some(shot.meta.clone());

                        // Try YOLO + annotation, or fall back to grid
                        let (annotated_b64, source_desc) = if let Some(ref mut detector) = self.yolo_detector {
                            let mut elements = detector.detect(&shot.image_bytes).unwrap_or_default();
                            if self.perception_cfg.enable_ui_automation {
                                if let Ok(uia) = crate::perception::ui_automation::collect_ui_elements(&shot.meta).await {
                                    crate::perception::ui_automation::merge_detections(&mut elements, uia, 0.3);
                                }
                            }
                            if !elements.is_empty() {
                                self.detected_elements = elements.clone();
                                let annotated = crate::perception::annotator::annotate_image(&shot.image_bytes, &elements)
                                    .unwrap_or(shot.image_bytes.clone());
                                let b64 = base64::engine::general_purpose::STANDARD.encode(&annotated);
                                let desc = format!(
                                    "Screenshot captured with {} annotated UI elements. Elements have IDs like btn_1, icon_2, input_1. \
                                     Use needs_viewport=true in plan_task steps.",
                                    elements.len()
                                );
                                (b64, desc)
                            } else {
                                self.detected_elements.clear();
                                let grid_bytes = draw_som_grid(&shot.image_bytes, self.grid_n)
                                    .unwrap_or(shot.image_bytes.clone());
                                let b64 = base64::engine::general_purpose::STANDARD.encode(&grid_bytes);
                                let last_col = col_label(self.grid_n - 1);
                                let desc = format!(
                                    "Screenshot captured. Grid: {n}x{n}, columns A-{last} (left to right), rows 1-{n} (top to bottom). \
                                     Use needs_viewport=true in plan_task steps - do NOT call get_viewport directly.",
                                    n = self.grid_n, last = last_col,
                                );
                                (b64, desc)
                            }
                        } else {
                            self.detected_elements.clear();
                            let grid_bytes = draw_som_grid(&shot.image_bytes, self.grid_n)
                                .unwrap_or(shot.image_bytes.clone());
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&grid_bytes);
                            let last_col = col_label(self.grid_n - 1);
                            let desc = format!(
                                "Screenshot captured. Grid: {n}x{n}, columns A-{last} (left to right), rows 1-{n} (top to bottom). \
                                 Use needs_viewport=true in plan_task steps - do NOT call get_viewport directly.",
                                n = self.grid_n, last = last_col,
                            );
                            (b64, desc)
                        };

                        let data_url = format!("data:image/png;base64,{}", annotated_b64);

                        self.conv_messages.push(ChatMessage {
                            role: "tool".into(),
                            content: MessageContent::Text(source_desc),
                            tool_call_id: Some(self.pending_tool_id.clone()),
                            tool_calls: None,
                        });
                        self.conv_messages.push(ChatMessage {
                            role: "user".into(),
                            content: MessageContent::Parts(vec![
                                ContentPart::ImageUrl { image_url: ImageUrl { url: data_url } },
                                ContentPart::Text { text: format!(
                                    "This is the current screen. Now call plan_task to accomplish: {}",
                                    self.current_goal
                                )},
                            ]),
                            tool_call_id: None,
                            tool_calls: None,
                        });
                        let _ = self.app.emit("viewport_captured", serde_json::json!({
                            "image_base64": annotated_b64,
                            "grid_n": self.grid_n,
                            "physical_width": shot.meta.physical_width,
                            "physical_height": shot.meta.physical_height,
                        }));
                        self.state = AgentState::Planning { goal: self.current_goal.clone() };
                    }
                    Err(e) => {
                        self.state = AgentState::Error { message: e.to_string() };
                    }
                }
                return;
            }

            other => {
                tracing::warn!(?other, "action not yet implemented");
                (false, "Not implemented".into())
            }
        };

        self.conv_messages.push(ChatMessage {
            role: "tool".into(),
            content: MessageContent::Text(msg.clone()),
            tool_call_id: Some(self.pending_tool_id.clone()),
            tool_calls: None,
        });

        let result = ActionResult {
            action: action.clone(),
            success: ok,
            error: if ok { None } else { Some(msg.clone()) },
            timestamp: chrono::Utc::now(),
        };
        self.push_history(&action, &result);
        if !ok { self.loop_ctrl.record_failure(); }

        let step_desc = self.todo_steps
            .get(self.current_step_idx)
            .map(|s| s.description.clone())
            .unwrap_or_else(|| format!("step {}", self.current_step_idx));
        self.steps_log.push(format!(
            "Step {}: {} 鈥?{}",
            self.current_step_idx + 1,
            step_desc,
            if ok { msg } else { format!("FAILED: {msg}") }
        ));
        self.current_step_idx += 1;

        Box::pin(self.advance_to_next_step()).await;
    }

    fn push_history(&mut self, action: &AgentAction, result: &ActionResult) {
        self.history.push(HistoryEntry {
            ts: result.timestamp.timestamp_millis(),
            role: "tool".into(),
            content: None,
            action: Some(serde_json::to_value(action).unwrap_or_default()),
        });
        let _ = self.history.flush();
    }
}

// 鈹€鈹€ Safety check 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

fn is_auto_approved(action: &AgentAction) -> bool {
    matches!(
        action,
        AgentAction::GetViewport { .. }
            | AgentAction::Wait { .. }
            | AgentAction::FinishTask { .. }
            | AgentAction::ReportFailure { .. }
            | AgentAction::EvaluateCompletion { .. }
            | AgentAction::MouseClick { .. }
            | AgentAction::MouseDoubleClick { .. }
            | AgentAction::MouseRightClick { .. }
            | AgentAction::TypeText { .. }
            | AgentAction::Hotkey { .. }
            | AgentAction::KeyPress { .. }
            | AgentAction::Scroll { .. }
    )
}

// 鈹€鈹€ Tool call parser 鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€

fn parse_tool_call_to_action(tc: &ToolCall) -> Result<AgentAction, String> {
    // Tolerate malformed JSON arguments 鈥?fall back to empty object
    let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, raw = %tc.function.arguments, "tool args JSON parse failed, using {{}}");
            serde_json::json!({})
        });

    match tc.function.name.as_str() {
        "plan_task" => {
            // Tolerate steps being a JSON string instead of an array (model sometimes stringifies)
            let steps_val = &args["steps"];
            let raw_steps: Vec<serde_json::Value> = if let Some(arr) = steps_val.as_array() {
                arr.clone()
            } else if let Some(s) = steps_val.as_str() {
                serde_json::from_str(s).unwrap_or_default()
            } else {
                tracing::warn!("plan_task: steps field missing or wrong type, using empty list");
                vec![]
            };

            let mut steps = Vec::new();
            for (i, s) in raw_steps.iter().enumerate() {
                // Support both new flat schema (action_type) and old nested schema (action.type)
                let action_type = s["action_type"]
                    .as_str()
                    .or_else(|| s["action"]["type"].as_str())
                    .unwrap_or("wait");

                // Build a synthetic args object merging step-level fields
                let mut step_args = s.clone();
                if step_args.is_object() {
                    // Ensure element_id is empty for viewport steps (VLM fills it later)
                    if s["needs_viewport"].as_bool().unwrap_or(false) {
                        step_args["element_id"] = serde_json::json!("");
                    }
                }

                let action = match parse_action_by_name(action_type, &step_args) {
                    Ok(a) => a,
                    Err(e) => {
                        tracing::warn!(step = i, error = %e, action_type, "unknown action_type in step, defaulting to wait");
                        AgentAction::Wait { milliseconds: 500 }
                    }
                };

                steps.push(TodoStep {
                    index: i,
                    description: s["description"].as_str().unwrap_or("").to_string(),
                    needs_viewport: s["needs_viewport"].as_bool().unwrap_or(false),
                    target: s["target"].as_str().map(|t| t.to_string()),
                    action,
                });
            }
            Ok(AgentAction::PlanTask { steps })
        }
        "evaluate_completion" => Ok(AgentAction::EvaluateCompletion {
            completed: args["completed"].as_bool().unwrap_or(false),
            summary: args["summary"].as_str().unwrap_or("").to_string(),
        }),
        other => parse_action_by_name(other, &args),
    }
}

fn parse_action_by_name(name: &str, args: &serde_json::Value) -> Result<AgentAction, String> {
    match name {
        "mouse_click" => Ok(AgentAction::MouseClick {
            element_id: args["element_id"].as_str().unwrap_or("").to_string(),
        }),
        "mouse_double_click" => Ok(AgentAction::MouseDoubleClick {
            element_id: args["element_id"].as_str().unwrap_or("").to_string(),
        }),
        "mouse_right_click" => Ok(AgentAction::MouseRightClick {
            element_id: args["element_id"].as_str().unwrap_or("").to_string(),
        }),
        "scroll" => Ok(AgentAction::Scroll {
            direction: args["direction"].as_str().unwrap_or("down").to_string(),
            distance: args["distance"].as_str().unwrap_or("short").to_string(),
            element_id: args["element_id"].as_str().map(|s| s.to_string()),
        }),
        "type_text" => Ok(AgentAction::TypeText {
            text: args["text"].as_str().unwrap_or("").to_string(),
            clear_first: args["clear_first"].as_bool().unwrap_or(false),
        }),
        "hotkey" => Ok(AgentAction::Hotkey {
            keys: args["keys"].as_str().unwrap_or("").to_string(),
        }),
        "key_press" => Ok(AgentAction::KeyPress {
            key: args["key"].as_str().unwrap_or("").to_string(),
        }),
        "get_viewport" => Ok(AgentAction::GetViewport {
            annotate: args["annotate"].as_bool().unwrap_or(true),
        }),
        "execute_terminal" => Ok(AgentAction::ExecuteTerminal {
            command: args["command"].as_str().unwrap_or("").to_string(),
            reason: args["reason"].as_str().unwrap_or("").to_string(),
        }),
        "mcp_call" => Ok(AgentAction::McpCall {
            server_name: args["server_name"].as_str().unwrap_or("").to_string(),
            tool_name: args["tool_name"].as_str().unwrap_or("").to_string(),
            arguments: args["arguments"].clone(),
        }),
        "invoke_skill" => Ok(AgentAction::InvokeSkill {
            skill_name: args["skill_name"].as_str().unwrap_or("").to_string(),
            inputs: args["inputs"].clone(),
        }),
        "wait" => Ok(AgentAction::Wait {
            milliseconds: args["milliseconds"].as_u64().unwrap_or(1000) as u32,
        }),
        "finish_task" => Ok(AgentAction::FinishTask {
            summary: args["summary"].as_str().unwrap_or("").to_string(),
        }),
        "report_failure" => Ok(AgentAction::ReportFailure {
            reason: args["reason"].as_str().unwrap_or("").to_string(),
            last_attempted_action: args["last_attempted_action"].as_str().map(|s| s.to_string()),
        }),
        "evaluate_completion" => Ok(AgentAction::EvaluateCompletion {
            completed: args["completed"].as_bool().unwrap_or(false),
            summary: args["summary"].as_str().unwrap_or("").to_string(),
        }),
        other => Err(format!("unknown tool: {other}")),
    }
}

fn action_supports_element_id(action: &AgentAction) -> bool {
    matches!(
        action,
        AgentAction::MouseClick { .. }
            | AgentAction::MouseDoubleClick { .. }
            | AgentAction::MouseRightClick { .. }
            | AgentAction::Scroll { .. }
    )
}

fn patch_element_id(action: AgentAction, cell: &str) -> AgentAction {
    match action {
        AgentAction::MouseClick { .. } => AgentAction::MouseClick { element_id: cell.to_string() },
        AgentAction::MouseDoubleClick { .. } => AgentAction::MouseDoubleClick { element_id: cell.to_string() },
        AgentAction::MouseRightClick { .. } => AgentAction::MouseRightClick { element_id: cell.to_string() },
        AgentAction::Scroll { direction, distance, .. } => AgentAction::Scroll {
            direction, distance, element_id: Some(cell.to_string()),
        },
        other => other,
    }
}

fn extract_cell_label_from_text(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"\b([A-L]{1,2})(\d{1,2})\b").ok()?;
    re.captures(text).map(|c| c[0].to_string())
}

