/// Lifecycle Optimized states of the SeeClaw agent.
/// Improved architecture: Removed Evaluating state (merged into Planning),
/// Removed Routing state (now a micro-operation).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AgentState {
    Idle,
    /// Planner is generating the todo list for the goal OR evaluating completion.
    /// This combines the old Planning and Evaluating states for efficiency.
    Planning { goal: String },
    /// Executing one step from the todo list.
    Executing { action: AgentAction },
    /// Waiting for visual stability after an action.
    WaitingForStability { action: AgentAction },
    WaitingForUser { pending_action: AgentAction },
    Error { message: String },
    Done { summary: String },
}

/// A single step in the planner's todo list.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TodoStep {
    pub index: usize,
    pub description: String,
    /// Whether this step requires a screen capture to locate a UI element.
    pub needs_viewport: bool,
    /// The UI element target description (used as VLM query), if needs_viewport.
    pub target: Option<String>,
    /// The action to execute once the element is located (or directly if no viewport needed).
    pub action: AgentAction,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    /// Planner produces a structured todo list.
    PlanTask { steps: Vec<TodoStep> },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionResult {
    pub action: AgentAction,
    pub success: bool,
    pub error: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LoopConfig {
    pub mode: LoopMode,
    pub max_duration_minutes: Option<u32>,
    pub max_failures: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopMode {
    UntilDone,
    Timed,
    FailureLimit,
}

/// Events that drive state transitions in the AgentEngine run loop.
#[derive(Debug)]
pub enum AgentEvent {
    GoalReceived(String),
    Stop,
    UserApproved,
    UserRejected,
}
