use tokio::sync::{mpsc, broadcast};
use serde::{Serialize, Deserialize};
use tokio::sync::broadcast::error::SendError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentMessage {
    PerceptionReady {
        screenshot: Vec<u8>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    ActionCompleted {
        action_id: String,
        success: bool,
        error: Option<String>,
    },
    VisualStable {
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    PlanRequired {
        goal: String,
        context: PlanContext,
    },
    PlanGenerated {
        steps: Vec<super::state::TodoStep>,
        should_finish: bool,
    },
    StopRequested,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanContext {
    pub last_action_result: Option<String>,
    pub cycle_count: u32,
    pub steps_completed: usize,
}

pub struct EventBus {
    tx: broadcast::Sender<AgentMessage>,
    rx: broadcast::Receiver<AgentMessage>,
    command_tx: mpsc::Sender<AgentMessage>,
    command_rx: mpsc::Receiver<AgentMessage>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, rx) = broadcast::channel(100);
        let (command_tx, command_rx) = mpsc::channel(100);
        
        Self {
            tx,
            rx,
            command_tx,
            command_rx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AgentMessage> {
        self.tx.subscribe()
    }

    pub fn send(&self, msg: AgentMessage) -> Result<(), SendError<AgentMessage>> {
        self.tx.send(msg).map(|_| ())
    }

    pub fn command_sender(&self) -> mpsc::Sender<AgentMessage> {
        self.command_tx.clone()
    }

    pub async fn recv_command(&mut self) -> Option<AgentMessage> {
        self.command_rx.recv().await
    }
}
