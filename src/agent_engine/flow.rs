//! Graph topology — defines the default agent execution flow.
//!
//! **Separation of concerns**: This module ONLY wires nodes and edges.
//! All business logic lives in the individual node implementations under `nodes/`.

use crate::agent_engine::graph::Graph;
use crate::agent_engine::nodes;
use crate::agent_engine::state::RouteType;

/// Build the default agent graph with all nodes and edges.
///
/// ```text
///  ┌──────────┐
///  │  router   │
///  └────┬──────┘
///       │ conditional: route_type
///       ├─ Simple ──────────────────┐
///       │                           ▼
///       │                   ┌──────────────┐
///       │                   │  direct_exec  │ ← uses simple_tool_calls
///       │                   └──────┬───────┘
///       │                          │
///       │                          ▼
///       │                   ┌──────────────┐
///       │                   │  action_exec  │
///       │                   └──────┬───────┘
///       │                          │ todo_steps empty → summarizer
///       │                          ▼
///       │                   ┌──────────────┐
///       │                   │  summarizer   │ ← LLM generates human answer
///       │                   └──────┬───────┘
///       │                          ▼
///       │                        (end)
///       │
///       └─ Complex ─────────┐
///                           ▼
///                    ┌──────────┐
///                    │ planner  │ ← LLM generates todo_steps
///                    └────┬─────┘
///                         │
///                         ▼
///                  ┌──────────────┐
///                  │ step_dispatch │ ← routes by StepMode
///                  └──────┬───────┘
///                         │
///                    ┌────▼───────┐
///                    │ action_exec │
///                    └────┬───────┘
///                         │
///                    step_advance
///                         │ conditional: more steps?
///                         ├─ step_dispatch (loop)
///                         └─ verifier
///                              │
///                              ├─ pass → summarizer → (end)
///                              └─ fail → planner (replan)
/// ```
pub fn build_default_flow() -> Graph {
    let mut graph = Graph::new();

    // ── Register all nodes ──────────────────────────────────────────────
    nodes::register_all_nodes(&mut graph);

    // ── Entry point ─────────────────────────────────────────────────────
    graph.set_entry_point("router");

    // ── Router → conditional on route_type ──────────────────────────────
    graph.add_conditional_edge("router", |state| {
        match state.route_type {
            RouteType::Simple => "direct_exec".to_string(),
            RouteType::Complex => "planner".to_string(),
        }
    });

    // ── Planner → step_dispatch (node itself returns GoTo or End) ───────
    // Planner returns End for FinishTask/ReportFailure, Continue otherwise.
    graph.add_edge("planner", "step_dispatch");

    // ── StepDispatch → GoTo target (direct_exec / vlm_observe / vlm_act)
    // StepDispatchNode uses GoTo(), so no edge needed here — but we add a
    // fallback static edge just in case.
    graph.add_edge("step_dispatch", "direct_exec");

    // ── DirectExec → action_exec ────────────────────────────────────────
    graph.add_edge("direct_exec", "action_exec");

    // ── VlmObserve → action_exec (Continue) ─────────────────────────────
    graph.add_edge("vlm_observe", "action_exec");

    // ── VlmAct → action_exec (Continue) ─────────────────────────────────
    graph.add_edge("vlm_act", "action_exec");

    // ── ActionExec → conditional: approval / stability / step_advance ───
    graph.add_conditional_edge("action_exec", |state| {
        if state.needs_approval {
            "user_confirm".to_string()
        } else if state.needs_stability {
            "stability".to_string()
        } else if state.todo_steps.is_empty() {
            // Simple route: no todo_steps → skip step_advance/verifier, go
            // straight to the summarizer which generates a human-readable answer.
            "summarizer".to_string()
        } else {
            "step_advance".to_string()
        }
    });

    // ── UserConfirm → action_exec (node uses GoTo) ─────────────────────
    // UserConfirmNode returns GoTo("action_exec") or GoTo("step_advance"),
    // so this is a fallback.
    graph.add_edge("user_confirm", "action_exec");

    // ── Stability → step_advance ────────────────────────────────────────
    graph.add_edge("stability", "step_advance");

    // ── StepAdvance → conditional: more steps or verifier ───────────────
    graph.add_conditional_edge("step_advance", |state| {
        if state.current_step_idx < state.todo_steps.len() {
            "step_dispatch".to_string()
        } else {
            "verifier".to_string()
        }
    });

    // ── Verifier → summarizer (pass) or planner (fail) ───────────────────
    // VerifierNode returns GoTo("summarizer") on pass, GoTo("planner") on fail.
    // Fallback static edge points to summarizer in case the node returns Continue.
    graph.add_edge("verifier", "summarizer");

    // ── Summarizer → end (always) ──────────────────────────────────────
    // SummarizerNode always returns End. No outgoing edge needed.

    graph
}
