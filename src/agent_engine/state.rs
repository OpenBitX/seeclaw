//! Shared mutable state that flows through every node in the graph.
//!
//! This replaces the old `AgentState` enum. State transitions are now implicit
//! — the graph's conditional edges read fields from `SharedState` to decide
//! which node runs next.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::llm::types::ChatMessage;
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
    /// Multi-step workflow requiring planning.
    Complex,
}

impl Default for RouteType {
    fn default() -> Self {
        Self::Complex
    }
}

// ── Step mode & status ─────────────────────────────────────────────────────

/// Execution mode for a single step in the TodoList.
/// Assigned by the Planner and consumed by StepDispatch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepMode {
    /// Pre-defined combo sequence from a skill — zero LLM, pure local execution.
    Combo,
    /// Known UI path — Planner provides exact tool calls, no VLM needed.
    Direct,
    /// Need VLM to locate an element, but the action is predetermined.
    VisualLocate,
    /// Complex visual task — VLM understands context and generates tool calls.
    VisualAct,
}

impl Default for StepMode {
    fn default() -> Self {
        Self::Combo
    }
}

/// Lifecycle status of a single TodoStep.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
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

/// A single step in the planner's TodoList (aligned with arch.md design).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoStep {
    pub index: usize,
    pub description: String,
    /// Execution mode: combo / direct / visual_locate / visual_act.
    #[serde(default)]
    pub mode: StepMode,
    /// Skill name to invoke (e.g. "os/open_software"). Used by Combo mode.
    #[serde(default)]
    pub skill: Option<String>,
    /// Parameters for the skill combo (e.g. {"software_name": "Edge"}).
    #[serde(default)]
    pub params: Option<serde_json::Value>,
    /// Pre-generated tool calls for `Direct` mode.
    #[serde(default)]
    pub tool_calls: Vec<ToolCallData>,
    /// Element description for VLM to locate (`VisualLocate` mode).
    pub target: Option<String>,
    /// Action template to execute after VLM locates element (`VisualLocate`).
    pub action_template: Option<AgentAction>,
    /// Sub-goal description for VLM autonomous mode (`VisualAct`).
    pub vlm_goal: Option<String>,
    /// Current lifecycle status.
    #[serde(default)]
    pub status: StepStatus,
}

/// Lightweight tool call data embedded in a TodoStep.
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
    /// Planner produces a structured todo list (used only during parse).
    PlanTask { steps: Vec<TodoStep> },
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
            todo_steps: Vec::new(),
            current_step_idx: 0,
            current_action: None,
            needs_stability: false,
            needs_approval: false,
            action_user_approved: false,
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
    pub fn reset_for_replan(&mut self) {
        self.todo_steps.clear();
        self.current_step_idx = 0;
        self.current_action = None;
        self.needs_stability = false;
        self.needs_approval = false;
        self.action_user_approved = false;
    }
}
