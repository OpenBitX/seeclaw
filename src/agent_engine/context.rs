//! Immutable resource context shared by all nodes.
//!
//! `NodeContext` holds references to long-lived resources that do NOT change
//! across node executions: the Tauri app handle, the LLM provider registry,
//! perception config, YOLO detector, safety config, etc.
//!
//! Nodes receive `&NodeContext` (immutable borrow) — they can read resources
//! but not mutate the context itself.

use std::sync::Arc;

use tauri::{AppHandle, Wry};
use tokio::sync::Mutex;

use crate::agent_engine::history::SessionHistory;
use crate::agent_engine::loop_control::LoopController;
use crate::config::PerceptionConfig;
use crate::llm::registry::ProviderRegistry;
use crate::perception::yolo_detector::YoloDetector;

/// Immutable resource container passed to every node.
pub struct NodeContext {
    /// Tauri application handle — used for emitting frontend events.
    pub app: AppHandle<Wry>,
    /// LLM provider registry (behind Mutex because providers are shared).
    pub registry: Arc<Mutex<ProviderRegistry>>,
    /// Perception configuration (grid size, YOLO paths, UIA flags, etc.).
    pub perception_cfg: PerceptionConfig,
    /// Grid resolution loaded from config (rows = cols = grid_n).
    pub grid_n: u32,
    /// YOLO detector instance (None if model file missing or disabled).
    pub yolo_detector: Arc<Mutex<Option<YoloDetector>>>,
    /// Loop controller for timeout / failure limits.
    pub loop_ctrl: Arc<Mutex<LoopController>>,
    /// Session history writer (JSONL).
    pub history: Arc<Mutex<SessionHistory>>,
}

impl NodeContext {
    pub fn new(
        app: AppHandle<Wry>,
        registry: Arc<Mutex<ProviderRegistry>>,
        perception_cfg: PerceptionConfig,
        yolo_detector: Option<YoloDetector>,
        loop_ctrl: LoopController,
    ) -> Self {
        let grid_n = perception_cfg.grid_n.clamp(4, 26);
        Self {
            app,
            registry,
            perception_cfg,
            grid_n,
            yolo_detector: Arc::new(Mutex::new(yolo_detector)),
            loop_ctrl: Arc::new(Mutex::new(loop_ctrl)),
            history: Arc::new(Mutex::new(SessionHistory::new())),
        }
    }
}
