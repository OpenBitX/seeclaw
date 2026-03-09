//! Shared mutable state that flows through every node in the graph.
//!
//! This replaces the old `AgentState` enum. State transitions are now implicit
//! — the graph's conditional edges read fields from `SharedState` to decide
//! which node runs next.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::llm::types::{ChatMessage, ContentPart, MessageContent};
use crate::perception::types::{ScreenshotMeta, UIElement};

// ── Route type ─────────────────────────────────────────────────────────────

/// The routing classification produced by the Router pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteType {
    /// Pure conversation / greeting / knowledge Q&A — no tools or GUI needed.
    Chat,
    /// Single GUI action (open app, click button, etc.).
    Simple,
    /// Multi-step workflow requiring planning (no initial screenshot).
    Complex,
    /// Multi-step workflow that *needs* the current screen to plan.
    /// Planner captures a screenshot before generating the todo list.
    ComplexVisual,
}

impl Default for RouteType {
    fn default() -> Self {
        Self::Chat
    }
}

// ── Step mode & status ─────────────────────────────────────────────────────

/// Execution mode for a step. StepRouter selects the actual mode at runtime;
/// Planner only provides a `recommended_mode` hint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepMode {
    /// Pre-defined combo sequence from a skill — zero LLM, pure local execution.
    Combo,
    /// LLM-driven loop: terminal commands, keyboard shortcuts, file ops — no vision.
    Chat,
    /// VLM-driven loop: screenshot → VLM → action → screenshot verify.
    Vlm,
}

impl Default for StepMode {
    fn default() -> Self {
        Self::Chat
    }
}

/// Lifecycle status of a single TodoStep.
/// NOTE: No serde rename — variant names serialize as-is (PascalCase) to match
/// the TypeScript StepStatus type ('Pending' | 'InProgress' | 'Completed' | ...).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
    Skipped,
    Failed,
}

impl Default for StepStatus {
    fn default() -> Self {
        Self::Pending
    }
}

// ── TodoStep ───────────────────────────────────────────────────────────────

/// A single step in the planner's TodoList.
///
/// The Planner outputs high-level sub-goals with recommendations.
/// Execution details (tool_calls, actions) are decided at runtime by
/// the loop agents (ChatAgent / VlmAgent / ComboExec).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoStep {
    pub index: usize,
    /// High-level description of what this step should achieve.
    pub description: String,
    /// Planner's recommended execution mode (hint, not binding).
    #[serde(default)]
    pub recommended_mode: StepMode,
    /// The actual mode selected by StepRouter at runtime.
    #[serde(default)]
    pub mode: StepMode,
    /// Skills that MUST be followed for this step (Planner-assigned).
    #[serde(default)]
    pub required_skills: Vec<String>,
    /// Planner's guidance/instructions for the loop agent executing this step.
    #[serde(default)]
    pub guidance: Option<String>,
    /// Skill name for combo mode (e.g. "open_software").
    #[serde(default)]
    pub skill: Option<String>,
    /// Parameters for the skill combo (e.g. {"software_name": "Edge"}).
    #[serde(default)]
    pub params: Option<serde_json::Value>,
    /// Current lifecycle status.
    #[serde(default)]
    pub status: StepStatus,
}

/// Lightweight tool call data used internally by agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallData {
    pub name: String,
    pub arguments: serde_json::Value,
}

// ── AgentAction ────────────────────────────────────────────────────────────

/// All possible atomic actions the executor can perform.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentAction {
    MouseClick { element_id: String },
    MouseDoubleClick { element_id: String },
    MouseRightClick { element_id: String },
    Scroll { direction: String, distance: String, element_id: Option<String> },
    TypeText { text: String, clear_first: bool },
    Hotkey { keys: String },
    KeyPress { key: String },
    GetViewport { annotate: bool },
    ExecuteTerminal { command: String, reason: String },
    McpCall { server_name: String, tool_name: String, arguments: serde_json::Value },
    InvokeSkill { skill_name: String, inputs: serde_json::Value },
    Wait { milliseconds: u32 },
    FinishTask { summary: String },
    ReportFailure { reason: String, last_attempted_action: Option<String> },
    /// Planner produces a structured plan (used only during parse).
    PlanTask {
        final_goal: String,
        plan_summary: String,
        steps: Vec<TodoStep>,
    },
}

// ── ActionResult ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub action: AgentAction,
    pub success: bool,
    pub error: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

// ── GraphResult ────────────────────────────────────────────────────────────

/// Final outcome of graph execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum GraphResult {
    Done { summary: String },
    Error { message: String },
}

// ── Loop config ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopConfig {
    pub mode: LoopMode,
    pub max_duration_minutes: Option<u32>,
    pub max_failures: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopMode {
    UntilDone,
    Timed,
    FailureLimit,
}

// ── AgentEvent (IPC from frontend) ─────────────────────────────────────────

/// Events sent from the frontend / commands layer into the graph runner.
#[derive(Debug)]
pub enum AgentEvent {
    GoalReceived(String),
    Stop,
    UserApproved,
    UserRejected,
}

// ── SharedState ────────────────────────────────────────────────────────────

/// The mutable data shared across all nodes during a single task execution.
///
/// Nodes read and write fields as needed; the graph engine passes this by
/// `&mut` reference to each node in sequence.
pub struct SharedState {
    // ── Task ────────────────────────────────────────────────────────────
    /// The user's original goal / query.
    pub goal: String,

    // ── Routing ─────────────────────────────────────────────────────────
    /// Classification result from the Router pipeline.
    pub route_type: RouteType,

    // ── Conversation / LLM context ──────────────────────────────────────
    /// The running conversation fed to the planner / LLM.
    pub conv_messages: Vec<ChatMessage>,
    /// Tool-call ID of the most recent pending tool call (for tool-result ack).
    pub pending_tool_id: String,

    // ── Plan context (from Planner) ─────────────────────────────────────
    /// Planner's summary of the overall plan (injected into loop agent context).
    pub plan_summary: String,
    /// Planner's restatement of the user's final goal (for loop agents).
    pub final_goal: String,

    // ── TodoList ────────────────────────────────────────────────────────
    /// Steps generated by the Planner.
    pub todo_steps: Vec<TodoStep>,
    /// Index of the step currently being executed.
    pub current_step_idx: usize,

    // ── Current action ──────────────────────────────────────────────────
    /// The action to be executed by `ActionExecNode`.
    pub current_action: Option<AgentAction>,
    /// Whether the current action needs visual stability check after execution.
    pub needs_stability: bool,
    /// Whether the current action needs user approval.
    pub needs_approval: bool,
    /// Set by `UserConfirmNode` after the user approves an action.
    /// Cleared by `ActionExecNode` once it consumes the approval and proceeds.
    /// This prevents `action_exec` from re-routing to `user_confirm` in a loop.
    pub action_user_approved: bool,

    // ── Dynamic loop control ────────────────────────────────────────────
    /// Current loop mode for the active step (set by StepRouter).
    pub current_loop_mode: StepMode,
    /// Set by loop agents when they want to switch execution mode.
    pub mode_switch_requested: Option<StepMode>,
    /// Set by loop agents when the current sub-goal is complete.
    pub step_complete: bool,
    /// The last execution result text (for StepEvaluate context).
    pub last_exec_result: String,
    /// Per-step conversation for loop agents (reset each step).
    pub step_messages: Vec<ChatMessage>,
    /// Unified iteration counter for the current step (incremented by chat_agent AND vlm_act).
    /// StepRouter resets this to 0 on each new step. StepEvaluate uses it for max-iter guard.
    pub step_iterations: u32,
    /// Brief action history for the current step ("iter 1: hotkey win+d", "iter 2: mouse_click UI_10").
    /// Used by VLM to avoid repeating the same action and to know when to call finish_step.
    pub step_action_history: Vec<String>,
    /// Whether the last action executed successfully (set by ActionExecNode).
    pub last_action_succeeded: bool,
    /// Kind of the last action executed (e.g. "mouse_click", "type_text"). For auto-completion heuristics.
    pub last_action_kind: String,

    // ── Perception ──────────────────────────────────────────────────────
    /// Most recently detected UI elements (YOLO / UIA).
    pub detected_elements: Vec<UIElement>,
    /// Metadata from the last screenshot capture.
    pub last_meta: Option<ScreenshotMeta>,

    // ── Execution log ───────────────────────────────────────────────────
    /// Accumulated step results for the evaluator / verifier.
    pub steps_log: Vec<String>,
    /// How many plan → execute → verify cycles have run (anti-loop guard).
    pub cycle_count: u32,

    // ── Control ─────────────────────────────────────────────────────────
    /// Shared atomic flag for immediate cancellation from the UI.
    pub stop_flag: Arc<AtomicBool>,
    /// Channel to receive user events (approval, rejection, etc.).
    pub event_rx: mpsc::Receiver<AgentEvent>,
    /// Final result of the graph execution.
    pub result: Option<GraphResult>,
}

impl SharedState {
    /// Create a new SharedState for a given goal.
    pub fn new(
        goal: String,
        stop_flag: Arc<AtomicBool>,
        event_rx: mpsc::Receiver<AgentEvent>,
    ) -> Self {
        Self {
            goal,
            route_type: RouteType::default(),
            conv_messages: Vec::new(),
            pending_tool_id: String::new(),
            plan_summary: String::new(),
            final_goal: String::new(),
            todo_steps: Vec::new(),
            current_step_idx: 0,
            current_action: None,
            needs_stability: false,
            needs_approval: false,
            action_user_approved: false,
            current_loop_mode: StepMode::Chat,
            mode_switch_requested: None,
            step_complete: false,
            last_exec_result: String::new(),
            step_messages: Vec::new(),
            step_iterations: 0,
            step_action_history: Vec::new(),
            last_action_succeeded: false,
            last_action_kind: String::new(),
            detected_elements: Vec::new(),
            last_meta: None,
            steps_log: Vec::new(),
            cycle_count: 0,
            stop_flag,
            event_rx,
            result: None,
        }
    }

    /// Check whether the stop flag has been set by the UI.
    pub fn is_stopped(&self) -> bool {
        self.stop_flag.load(Ordering::Relaxed)
    }

    /// Reset state for a new planning cycle (keeps goal and conv_messages).
    /// Strips images from conv_messages to prevent token waste on replan.
    pub fn reset_for_replan(&mut self) {
        // Strip all images from conv_messages before replan — they're stale
        // and would waste tokens. Keep the text content for context continuity.
        for msg in &mut self.conv_messages {
            if let MessageContent::Parts(ref mut parts) = msg.content {
                let mut new_parts = Vec::new();
                let mut had_image = false;
                for part in parts.drain(..) {
                    match part {
                        ContentPart::ImageUrl { .. } => {
                            if !had_image {
                                new_parts.push(ContentPart::Text {
                                    text: "[Screenshot from previous cycle — stripped]".to_string(),
                                });
                                had_image = true;
                            }
                        }
                        other => new_parts.push(other),
                    }
                }
                *parts = new_parts;
            }
        }

        self.todo_steps.clear();
        self.current_step_idx = 0;
        self.current_action = None;
        self.needs_stability = false;
        self.needs_approval = false;
        self.action_user_approved = false;
        self.mode_switch_requested = None;
        self.step_complete = false;
        self.last_exec_result.clear();
        self.step_messages.clear();
        self.step_iterations = 0;
        self.step_action_history.clear();
        self.last_action_succeeded = false;
        self.last_action_kind.clear();
        self.plan_summary.clear();
        self.final_goal.clear();
    }
}
