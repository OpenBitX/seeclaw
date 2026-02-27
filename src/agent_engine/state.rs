/// Lifecycle states of the SeeClaw agent.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AgentState {
    Idle,
    Routing { goal: String },
    Observing { goal: String },
    Planning { goal: String },
    Executing { action: AgentAction },
    WaitingForUser { pending_action: AgentAction },
    Evaluating { last_result: ActionResult },
    Error(String),
    Done { summary: String },
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
