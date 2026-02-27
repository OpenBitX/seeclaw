---
name: rust-agent-state-machine
description: Build the SeeClaw enum-based Agent state machine in Rust without LangGraph. Use when asked to "implement the agent loop", "set up the state machine", "add agent states", "build the orchestrator", or "implement human-in-the-loop".
argument-hint: <target-module>
---

# Skill: Enum-Based Agent State Machine in Rust

## Overview

SeeClaw does not use LangGraph or Python orchestration.
The agent engine is a pure Rust enum state machine running inside a `tokio` async task.

## Core Pattern

### Step 1 — Define State & Event Enums

```rust
// src/agent_engine/state.rs

/// Agent lifecycle states — each state owns its relevant data
#[derive(Debug, Clone)]
pub enum AgentState {
    Idle,
    Observing { goal: String },
    Planning  { goal: String, screenshot_b64: String, bboxes: Vec<crate::vision::BBox> },
    Executing { action: AgentAction },
    WaitingForUser { pending_action: AgentAction },
    Evaluating { last_result: ActionResult },
    Error(String),
}

/// Events that trigger state transitions
#[derive(Debug)]
pub enum AgentEvent {
    GoalReceived(String),
    ScreenshotCaptured { b64: String },
    VisionParsed(Vec<crate::vision::BBox>),
    LlmResponseReady(LlmResponse),
    UserApproved,
    UserRejected,
    ActionSucceeded(ActionResult),
    ActionFailed(String),
    Reset,
}
```

### Step 2 — Define Actions

```rust
// src/agent_engine/action.rs

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum AgentAction {
    MouseClick { x: f64, y: f64 },
    KeyboardInput { text: String },
    TerminalCmd { command: String },  // HIGH RISK — requires human approval
    Screenshot,
    Done { summary: String },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ActionResult {
    pub action: AgentAction,
    pub output: String,
    pub success: bool,
}
```

### Step 3 — AgentEngine Struct

```rust
// src/agent_engine/engine.rs

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tauri::AppHandle;

pub struct AgentEngine {
    state: AgentState,
    ctx: Arc<Mutex<AgentContext>>,
    event_rx: mpsc::Receiver<AgentEvent>,
    app: AppHandle,
}

pub struct AgentContext {
    pub history: Vec<serde_json::Value>,   // conversation history for LLM
    pub goal: String,
}

impl AgentEngine {
    pub fn new(app: AppHandle, event_rx: mpsc::Receiver<AgentEvent>) -> Self {
        Self {
            state: AgentState::Idle,
            ctx: Arc::new(Mutex::new(AgentContext { history: vec![], goal: String::new() })),
            event_rx,
            app,
        }
    }
}
```

### Step 4 — Async Run Loop

```rust
// src/agent_engine/engine.rs (continued)

impl AgentEngine {
    pub async fn run_loop(&mut self) {
        loop {
            // Emit current state to frontend for UI sync
            let _ = self.app.emit("agent_state", &self.state_label());

            match &self.state.clone() {
                AgentState::Idle => {
                    // Wait for next user goal event
                    if let Some(AgentEvent::GoalReceived(goal)) = self.event_rx.recv().await {
                        self.ctx.lock().await.goal = goal.clone();
                        self.state = AgentState::Observing { goal };
                    }
                }

                AgentState::Observing { goal } => {
                    // Capture screenshot, then transition to Planning
                    match crate::vision::capture_screenshot().await {
                        Ok(b64) => self.state = AgentState::Planning {
                            goal: goal.clone(),
                            screenshot_b64: b64,
                            bboxes: vec![],
                        },
                        Err(e) => self.state = AgentState::Error(e.to_string()),
                    }
                }

                AgentState::WaitingForUser { pending_action } => {
                    // Block until frontend sends approval or rejection
                    match self.event_rx.recv().await {
                        Some(AgentEvent::UserApproved) => {
                            self.state = AgentState::Executing { action: pending_action.clone() };
                        }
                        Some(AgentEvent::UserRejected) => {
                            self.state = AgentState::Idle;
                        }
                        _ => {}
                    }
                }

                // ... other states handled similarly
                _ => { tokio::task::yield_now().await; }
            }
        }
    }

    fn state_label(&self) -> &'static str {
        match &self.state {
            AgentState::Idle => "idle",
            AgentState::Observing { .. } => "observing",
            AgentState::Planning { .. } => "planning",
            AgentState::Executing { .. } => "executing",
            AgentState::WaitingForUser { .. } => "waiting_for_user",
            AgentState::Evaluating { .. } => "evaluating",
            AgentState::Error(_) => "error",
        }
    }
}
```

## Ownership Safety Rules

- Share `AgentContext` across async tasks using `Arc<Mutex<AgentContext>>`
- Never `.clone()` large screenshot data — pass `Arc<str>` or index into a temporary buffer
- Send events to the engine via `mpsc::Sender<AgentEvent>` — one sender per Tauri command handler
- The engine runs in a dedicated `tokio::spawn` task; never block it with synchronous I/O
