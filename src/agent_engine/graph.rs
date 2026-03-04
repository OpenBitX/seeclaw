//! Graph execution engine — LangGraph-style node/edge framework.
//!
//! The `Graph` struct holds a set of named nodes and edges. At runtime it:
//! 1. Starts at the `entry_point` node.
//! 2. Executes the current node, getting a `NodeOutput`.
//! 3. Resolves the next node via the edge definition (static or conditional).
//! 4. Repeats until `NodeOutput::End` or stop_flag.
//!
//! **Design**: Graph only manages topology and the run loop.
//! All business logic lives in individual `Node` implementations.

use std::collections::HashMap;
use std::sync::atomic::Ordering;

use tauri::Emitter;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::node::{Node, NodeOutput};
use crate::agent_engine::state::{GraphResult, SharedState};

// ── Edge types ─────────────────────────────────────────────────────────────

/// An outgoing edge from a node — determines where to go next.
pub enum Edge {
    /// Always go to a fixed node.
    Static { to: String },
    /// Evaluate a condition function at runtime to pick the next node.
    Conditional {
        router: Box<dyn Fn(&SharedState) -> String + Send + Sync>,
    },
}

// ── Graph ──────────────────────────────────────────────────────────────────

/// The agent execution graph.
pub struct Graph {
    /// Registered nodes, keyed by node name.
    nodes: HashMap<String, Box<dyn Node>>,
    /// Outgoing edges, keyed by source node name.
    edges: HashMap<String, Edge>,
    /// The name of the first node to execute.
    entry_point: String,
}

impl Graph {
    /// Create a new empty graph.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            entry_point: String::new(),
        }
    }

    /// Register a node.
    pub fn add_node(&mut self, node: Box<dyn Node>) {
        let name = node.name().to_string();
        self.nodes.insert(name, node);
    }

    /// Set a static edge: after `from` finishes, always go to `to`.
    pub fn add_edge(&mut self, from: &str, to: &str) {
        self.edges.insert(
            from.to_string(),
            Edge::Static { to: to.to_string() },
        );
    }

    /// Set a conditional edge: after `from` finishes, call `router(state)` to
    /// get the name of the next node.
    pub fn add_conditional_edge<F>(&mut self, from: &str, router: F)
    where
        F: Fn(&SharedState) -> String + Send + Sync + 'static,
    {
        self.edges.insert(
            from.to_string(),
            Edge::Conditional {
                router: Box::new(router),
            },
        );
    }

    /// Set the entry point (first node to run).
    pub fn set_entry_point(&mut self, name: &str) {
        self.entry_point = name.to_string();
    }

    /// Run the graph to completion.
    ///
    /// This is the main execution loop — it replaces the old `AgentEngine::run_loop()`.
    pub async fn run(
        &self,
        state: &mut SharedState,
        ctx: &NodeContext,
    ) -> Result<(), String> {
        let mut current = self.entry_point.clone();

        loop {
            // ── Stop check ──────────────────────────────────────────────
            if state.stop_flag.load(Ordering::Relaxed) {
                tracing::info!("graph: stop flag detected, terminating");
                state.result = Some(GraphResult::Error {
                    message: "任务已被用户终止".to_string(),
                });
                // Notify frontend
                let _ = ctx.app.emit("agent_state_changed", serde_json::json!({
                    "state": "done",
                    "summary": "任务已被用户终止",
                }));
                break;
            }

            // ── Find the node ───────────────────────────────────────────
            let node = self
                .nodes
                .get(&current)
                .ok_or_else(|| format!("graph: unknown node '{current}'"))?;

            tracing::debug!(node = %current, "graph: executing node");

            // Emit state so frontend can track progress — map node name to UI state kind
            let ui_state = match current.as_str() {
                "router"       => "routing",
                "simple_chat"  => "responding",
                "planner"      => "planning",
                "vlm_observe"  => "observing",
                "vlm_act"      => "observing",
                "summarizer"   => "evaluating",
                "verifier"     => "evaluating",
                "user_confirm" => "waiting_for_user",
                _              => "executing",
            };
            let _ = ctx.app.emit("agent_state_changed", serde_json::json!({
                "state": ui_state,
                "node": current,
            }));

            // ── Execute ─────────────────────────────────────────────────
            let output = node.execute(state, ctx).await;

            match output {
                Ok(NodeOutput::End) => {
                    tracing::info!(node = %current, "graph: node signalled End");
                    break;
                }
                Ok(NodeOutput::GoTo(target)) => {
                    tracing::debug!(from = %current, to = %target, "graph: GoTo");
                    current = target;
                }
                Ok(NodeOutput::Continue) => {
                    // Resolve next node via edge
                    match self.edges.get(&current) {
                        Some(Edge::Static { to }) => {
                            tracing::debug!(from = %current, to = %to, "graph: static edge");
                            current = to.clone();
                        }
                        Some(Edge::Conditional { router }) => {
                            let next = router(state);
                            tracing::debug!(from = %current, to = %next, "graph: conditional edge");
                            current = next;
                        }
                        None => {
                            tracing::warn!(node = %current, "graph: no outgoing edge, terminating");
                            break;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(node = %current, error = %e, "graph: node execution failed");
                    state.result = Some(GraphResult::Error { message: e.clone() });
                    let _ = ctx.app.emit("agent_state_changed", serde_json::json!({
                        "state": "error",
                        "message": e,
                    }));
                    break;
                }
            }

            // Yield to allow other async tasks to progress
            tokio::task::yield_now().await;
        }

        Ok(())
    }
}
