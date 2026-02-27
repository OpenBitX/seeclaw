use std::sync::Arc;

use base64::Engine as _;
use tauri::{AppHandle, Emitter, Wry};
use tokio::sync::{mpsc, Mutex};

use crate::agent_engine::history::{HistoryEntry, SessionHistory};
use crate::agent_engine::loop_control::LoopController;
use crate::agent_engine::state::{AgentAction, AgentEvent, AgentState, ActionResult, LoopConfig};
use crate::executor::input;
use crate::llm::registry::ProviderRegistry;
use crate::llm::tools::load_builtin_tools;
use crate::llm::types::{ChatMessage, ContentPart, ImageUrl, MessageContent, ToolCall};
use crate::perception::screenshot::capture_primary;
use crate::perception::som_grid::{
    build_grid_prompt, col_label, draw_som_grid, grid_cell_to_physical, parse_grid_label,
};
use crate::perception::types::ScreenshotMeta;

const GRID_N: u32 = 12;

const SYSTEM_PROMPT: &str = "\
You are SeeClaw, a desktop GUI automation agent running on Windows.

Rules:
- Always call `get_viewport` first to see the current screen state before clicking.
- Use the grid cell label (e.g. \"C4\") as the `element_id` for all click/scroll actions.
- After completing a click, call `finish_task` with a brief summary.
- If you cannot find the target, call `report_failure`.
- Reason step-by-step before every tool call.
- Respond in the same language as the user's goal.";

pub struct AgentEngine {
    state: AgentState,
    event_rx: mpsc::Receiver<AgentEvent>,
    loop_ctrl: LoopController,
    history: SessionHistory,
    app: AppHandle<Wry>,
    registry: Arc<Mutex<ProviderRegistry>>,

    // ── Conversation context (reset per goal) ─────────────────────────────
    conv_messages: Vec<ChatMessage>,
    current_goal: String,
    last_meta: Option<ScreenshotMeta>,
    /// tool_call_id of the most recently dispatched tool call.
    pending_tool_id: String,
}

impl AgentEngine {
    pub fn new(
        app: AppHandle<Wry>,
        loop_config: LoopConfig,
        event_rx: mpsc::Receiver<AgentEvent>,
        registry: Arc<Mutex<ProviderRegistry>>,
    ) -> Self {
        Self {
            state: AgentState::Idle,
            event_rx,
            loop_ctrl: LoopController::new(loop_config),
            history: SessionHistory::new(),
            app,
            registry,
            conv_messages: Vec::new(),
            current_goal: String::new(),
            last_meta: None,
            pending_tool_id: String::new(),
        }
    }

    pub async fn run_loop(&mut self) {
        loop {
            if let Err(e) = self.app.emit("agent_state_changed", &self.state) {
                tracing::warn!("emit agent_state_changed failed: {e}");
            }

            if self.loop_ctrl.should_stop() {
                tracing::info!("loop controller triggered stop");
                self.state = AgentState::Done {
                    summary: "Loop limit reached".into(),
                };
                let _ = self.app.emit("agent_state_changed", &self.state);
                break;
            }

            match self.state.clone() {
                // ── Idle: wait for a new goal ──────────────────────────────
                AgentState::Idle => {
                    match self.event_rx.recv().await {
                        Some(AgentEvent::GoalReceived(goal)) => {
                            tracing::info!(goal = %goal, "goal received → Routing");
                            self.current_goal = goal.clone();
                            self.last_meta = None;
                            self.pending_tool_id.clear();

                            // Initialize conversation with system + user message
                            self.conv_messages = vec![
                                ChatMessage {
                                    role: "system".into(),
                                    content: MessageContent::Text(SYSTEM_PROMPT.into()),
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
                            self.state = AgentState::Routing { goal };
                        }
                        Some(AgentEvent::Stop) | None => break,
                        _ => {}
                    }
                }

                // ── Routing → Observing → Planning ────────────────────────
                AgentState::Routing { goal } => {
                    tracing::info!(goal = %goal, "Routing → Observing");
                    self.state = AgentState::Observing { goal };
                }

                AgentState::Observing { goal } => {
                    tracing::info!(goal = %goal, "Observing → Planning");
                    self.state = AgentState::Planning { goal };
                }

                // ── Planning: call LLM with full conversation history ─────
                AgentState::Planning { goal } => {
                    tracing::info!(
                        goal = %goal,
                        messages = self.conv_messages.len(),
                        "Planning → calling LLM"
                    );

                    let tools = load_builtin_tools().unwrap_or_default();
                    let messages = self.conv_messages.clone();

                    // Use `tools` role: vision-capable + function-calling (GLM-4V-Flash)
                    let call_result = {
                        let reg = self.registry.lock().await;
                        reg.call_config_for_role("tools")
                    };

                    let (provider, cfg) = match call_result {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::error!(error = %e, "no LLM config for tools role");
                            self.state = AgentState::Error { message: e.to_string() };
                            continue;
                        }
                    };

                    match provider.chat(messages, tools, &cfg, &self.app).await {
                        Ok(response) => {
                            if let Some(tc) = response.tool_calls.into_iter().next() {
                                // Append the assistant's response (with tool call) to history
                                self.conv_messages.push(ChatMessage {
                                    role: "assistant".into(),
                                    content: MessageContent::Text(response.content.clone()),
                                    tool_call_id: None,
                                    tool_calls: Some(vec![tc.clone()]),
                                });
                                self.pending_tool_id = tc.id.clone();

                                match parse_tool_call_to_action(&tc) {
                                    Ok(action) => {
                                        tracing::info!(
                                            tool = %tc.function.name,
                                            args = %tc.function.arguments,
                                            auto = is_auto_approved(&action),
                                            "Planning → dispatching tool call"
                                        );

                                        if is_auto_approved(&action) {
                                            // Safe action: skip approval, go straight to Executing
                                            self.state = AgentState::Executing { action };
                                        } else {
                                            // Risky action: ask for user approval
                                            let req = serde_json::json!({
                                                "id": &tc.id,
                                                "action": serde_json::to_value(&action).unwrap_or_default(),
                                                "reason": format!("执行: {}", tc.function.name),
                                                "timestamp": chrono::Utc::now().to_rfc3339(),
                                            });
                                            let _ = self.app.emit("action_required", &req);
                                            self.state = AgentState::WaitingForUser {
                                                pending_action: action,
                                            };
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "unknown tool call → Idle");
                                        self.state = AgentState::Idle;
                                    }
                                }
                            } else {
                                // Content-only response (no tool call)
                                tracing::info!("content-only response → Idle");
                                self.state = AgentState::Idle;
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "LLM call failed");
                            self.state = AgentState::Error { message: e.to_string() };
                        }
                    }
                }

                // ── WaitingForUser: human-in-the-loop approval ────────────
                AgentState::WaitingForUser { pending_action } => {
                    tracing::info!(?pending_action, "waiting for user approval");
                    match self.event_rx.recv().await {
                        Some(AgentEvent::UserApproved) => {
                            tracing::info!("user approved → Executing");
                            self.state = AgentState::Executing {
                                action: pending_action,
                            };
                        }
                        Some(AgentEvent::UserRejected)
                        | Some(AgentEvent::Stop)
                        | None => {
                            tracing::info!("user rejected / stop → Idle");
                            self.state = AgentState::Idle;
                        }
                        _ => {}
                    }
                }

                // ── Executing: run the action ─────────────────────────────
                AgentState::Executing { action } => {
                    tracing::info!(?action, "Executing");
                    self.execute_action(action).await;
                }

                // ── Evaluating: check result and loop ─────────────────────
                AgentState::Evaluating { last_result } => {
                    if !last_result.success {
                        self.loop_ctrl.record_failure();
                        tracing::warn!("action failed, failure count incremented");
                    }
                    tracing::info!("Evaluating → Idle");
                    self.state = AgentState::Idle;
                }

                AgentState::Done { .. } | AgentState::Error { .. } => break,
            }

            tokio::task::yield_now().await;
        }
        tracing::info!(session = %self.history.session_id, "agent loop ended");
    }

    // ── Action execution ──────────────────────────────────────────────────────

    async fn execute_action(&mut self, action: AgentAction) {
        match action.clone() {
            // ── GetViewport: take screenshot + draw SoM grid ──────────────
            AgentAction::GetViewport { annotate } => {
                tracing::info!(annotate, "executing get_viewport");
                match capture_primary().await {
                    Ok(shot) => {
                        self.last_meta = Some(shot.meta.clone());

                        // Draw SoM grid overlay
                        let grid_bytes = if annotate {
                            draw_som_grid(&shot.image_bytes, GRID_N).unwrap_or(shot.image_bytes.clone())
                        } else {
                            shot.image_bytes.clone()
                        };
                        let grid_b64 = base64::engine::general_purpose::STANDARD.encode(&grid_bytes);

                        // Emit to frontend for display
                        let _ = self.app.emit(
                            "viewport_captured",
                            serde_json::json!({
                                "image_base64": grid_b64,
                                "grid_n": GRID_N,
                                "physical_width": shot.meta.physical_width,
                                "physical_height": shot.meta.physical_height,
                            }),
                        );

                        let last_col = col_label(GRID_N - 1);

                        // ① Tool result: plain text only.
                        //    GLM does NOT support array content in role=tool messages.
                        self.conv_messages.push(ChatMessage {
                            role: "tool".into(),
                            content: MessageContent::Text(format!(
                                "Screenshot captured. Grid: {n}×{n}, \
                                 columns A–{last} (left→right), rows 1–{n} (top→bottom). \
                                 Use the grid cell label (e.g. \"C4\") as element_id for clicks.",
                                n = GRID_N,
                                last = last_col,
                            )),
                            tool_call_id: Some(self.pending_tool_id.clone()),
                            tool_calls: None,
                        });

                        // ② Separate user turn carrying the actual image.
                        //    GLM-4.6V base64 format: raw base64 string (no data URI prefix).
                        self.conv_messages.push(ChatMessage {
                            role: "user".into(),
                            content: MessageContent::Parts(vec![
                                ContentPart::ImageUrl {
                                    image_url: ImageUrl {
                                        url: grid_b64.clone(),
                                    },
                                },
                                ContentPart::Text {
                                    text: build_grid_prompt(&self.current_goal, GRID_N),
                                },
                            ]),
                            tool_call_id: None,
                            tool_calls: None,
                        });

                        tracing::info!(
                            phys = %format!("{}×{}", shot.meta.physical_width, shot.meta.physical_height),
                            scale = shot.meta.scale_factor,
                            "viewport captured → looping back to Planning"
                        );

                        // Loop back to Planning with the screenshot in context
                        self.state = AgentState::Planning {
                            goal: self.current_goal.clone(),
                        };
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "get_viewport: screenshot failed");
                        self.state = AgentState::Error { message: e.to_string() };
                    }
                }
            }

            // ── MouseClick: grid cell → physical pixel → enigo click ──────
            AgentAction::MouseClick { ref element_id }
            | AgentAction::MouseDoubleClick { ref element_id }
            | AgentAction::MouseRightClick { ref element_id } => {
                let is_double = matches!(action, AgentAction::MouseDoubleClick { .. });
                let is_right = matches!(action, AgentAction::MouseRightClick { .. });

                let (ok, msg) = if let Some(meta) = &self.last_meta {
                    if let Some((col, row)) = parse_grid_label(element_id) {
                        let (px, py) = grid_cell_to_physical(
                            col, row,
                            meta.physical_width,
                            meta.physical_height,
                            GRID_N,
                        );
                        tracing::info!(
                            element_id = %element_id,
                            col, row,
                            x = px, y = py,
                            "click: grid cell → physical pixel"
                        );
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
                        (false, format!("Cannot parse grid label: {element_id}"))
                    }
                } else {
                    (false, "No viewport captured — call get_viewport first".into())
                };

                tracing::info!(success = ok, msg = %msg, "mouse action complete");

                // Append tool result to conversation
                self.conv_messages.push(ChatMessage {
                    role: "tool".into(),
                    content: MessageContent::Text(msg.clone()),
                    tool_call_id: Some(self.pending_tool_id.clone()),
                    tool_calls: None,
                });

                let result = ActionResult {
                    action: action.clone(),
                    success: ok,
                    error: if ok { None } else { Some(msg) },
                    timestamp: chrono::Utc::now(),
                };
                self.push_history(&action, &result);
                self.state = AgentState::Evaluating { last_result: result };
            }

            // ── TypeText ──────────────────────────────────────────────────
            AgentAction::TypeText { ref text, clear_first } => {
                let result_ok = input::type_text(text.clone(), clear_first).await;
                let (ok, msg) = match result_ok {
                    Ok(()) => (true, format!("Typed: {text}")),
                    Err(e) => (false, format!("TypeText failed: {e}")),
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
                    error: if ok { None } else { Some(msg) },
                    timestamp: chrono::Utc::now(),
                };
                self.push_history(&action, &result);
                self.state = AgentState::Evaluating { last_result: result };
            }

            // ── Hotkey ────────────────────────────────────────────────────
            AgentAction::Hotkey { ref keys } => {
                let result_ok = input::press_hotkey(keys.clone()).await;
                let (ok, msg) = match result_ok {
                    Ok(()) => (true, format!("Hotkey: {keys}")),
                    Err(e) => (false, format!("Hotkey failed: {e}")),
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
                    error: if ok { None } else { Some(msg) },
                    timestamp: chrono::Utc::now(),
                };
                self.push_history(&action, &result);
                self.state = AgentState::Evaluating { last_result: result };
            }

            // ── Wait ──────────────────────────────────────────────────────
            AgentAction::Wait { milliseconds } => {
                tracing::info!(ms = milliseconds, "waiting");
                tokio::time::sleep(std::time::Duration::from_millis(milliseconds as u64)).await;
                self.conv_messages.push(ChatMessage {
                    role: "tool".into(),
                    content: MessageContent::Text(format!("Waited {milliseconds}ms")),
                    tool_call_id: Some(self.pending_tool_id.clone()),
                    tool_calls: None,
                });
                let result = ActionResult {
                    action: action.clone(),
                    success: true,
                    error: None,
                    timestamp: chrono::Utc::now(),
                };
                self.push_history(&action, &result);
                self.state = AgentState::Evaluating { last_result: result };
            }

            // ── FinishTask ────────────────────────────────────────────────
            AgentAction::FinishTask { ref summary } => {
                tracing::info!(summary = %summary, "task finished");
                self.conv_messages.push(ChatMessage {
                    role: "tool".into(),
                    content: MessageContent::Text(format!("Task complete: {summary}")),
                    tool_call_id: Some(self.pending_tool_id.clone()),
                    tool_calls: None,
                });
                let result = ActionResult {
                    action: action.clone(),
                    success: true,
                    error: None,
                    timestamp: chrono::Utc::now(),
                };
                self.push_history(&action, &result);
                self.state = AgentState::Done {
                    summary: summary.clone(),
                };
            }

            // ── ReportFailure ─────────────────────────────────────────────
            AgentAction::ReportFailure { ref reason, .. } => {
                tracing::warn!(reason = %reason, "task failure reported");
                self.conv_messages.push(ChatMessage {
                    role: "tool".into(),
                    content: MessageContent::Text(format!("Failure: {reason}")),
                    tool_call_id: Some(self.pending_tool_id.clone()),
                    tool_calls: None,
                });
                self.state = AgentState::Error {
                    message: reason.clone(),
                };
            }

            // ── Unimplemented stubs ───────────────────────────────────────
            other => {
                tracing::warn!(?other, "action not yet implemented → Evaluating");
                let result = ActionResult {
                    action: action.clone(),
                    success: false,
                    error: Some("Not implemented".into()),
                    timestamp: chrono::Utc::now(),
                };
                self.push_history(&action, &result);
                self.state = AgentState::Evaluating { last_result: result };
            }
        }
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

// ── Safety check ─────────────────────────────────────────────────────────────

/// Actions that don't modify the system state can skip user approval.
fn is_auto_approved(action: &AgentAction) -> bool {
    matches!(
        action,
        AgentAction::GetViewport { .. }
            | AgentAction::Wait { .. }
            | AgentAction::FinishTask { .. }
            | AgentAction::ReportFailure { .. }
    )
}

// ── Tool call parser ──────────────────────────────────────────────────────────

fn parse_tool_call_to_action(tc: &ToolCall) -> Result<AgentAction, String> {
    let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
        .unwrap_or(serde_json::json!({}));

    match tc.function.name.as_str() {
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
            last_attempted_action: args["last_attempted_action"]
                .as_str()
                .map(|s| s.to_string()),
        }),
        other => Err(format!("unknown tool: {other}")),
    }
}
