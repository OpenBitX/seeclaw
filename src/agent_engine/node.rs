//! Core Node trait and NodeOutput for the LangGraph-style execution framework.
//!
//! Every node in the agent graph implements the `Node` trait. The graph engine
//! calls `execute()` on the current node, then uses the returned `NodeOutput`
//! together with the edge definitions to determine the next node.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::state::SharedState;

// ── Shared cancellation utility ────────────────────────────────────────────

/// Yields until the stop flag is set. Use inside `tokio::select!` in any node
/// that needs cooperative cancellation.
///
/// ```rust
/// use tokio::select;
/// select! {
///     result = some_async_call() => { ... }
///     _ = poll_stop(state.stop_flag.clone()) => return Ok(NodeOutput::End),
/// }
/// ```
pub async fn poll_stop(flag: Arc<AtomicBool>) {
    loop {
        if flag.load(Ordering::Relaxed) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

// ── NodeOutput ─────────────────────────────────────────────────────────────

/// The return value of a node execution, telling the graph what to do next.
#[derive(Debug, Clone)]
pub enum NodeOutput {
    /// Continue along the registered edge (static or conditional).
    Continue,
    /// Jump directly to a named node, bypassing normal edge resolution.
    GoTo(String),
    /// Terminate graph execution (task finished or fatal error).
    End,
}

// ── Node trait ──────────────────────────────────────────────────────────────

/// A single processing unit in the agent graph.
///
/// Design principles:
/// - **Stateless**: all mutable data lives in `SharedState`.
/// - **Single responsibility**: each node does exactly one thing.
/// - **Composable**: nodes can be freely added / removed / reordered in the graph.
#[async_trait]
pub trait Node: Send + Sync {
    /// A unique human-readable name for this node (used as graph key).
    fn name(&self) -> &str;

    /// Execute the node's logic.
    ///
    /// - Read / mutate `state` (shared mutable data).
    /// - Use `ctx` for immutable resources (registry, app handle, etc.).
    /// - Return `NodeOutput` to guide graph traversal.
    async fn execute(
        &self,
        state: &mut SharedState,
        ctx: &NodeContext,
    ) -> Result<NodeOutput, String>;
}
