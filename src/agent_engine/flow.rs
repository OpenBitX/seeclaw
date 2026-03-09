//! Graph topology — defines the default agent execution flow.
//!
//! **Separation of concerns**: This module ONLY wires nodes and edges.
//! All business logic lives in the individual node implementations under `nodes/`.

use crate::agent_engine::graph::Graph;
use crate::agent_engine::nodes;
use crate::agent_engine::state::{RouteType, StepStatus};

/// Build the default agent graph with all nodes and edges.
///
/// ```text
///  ┌──────────┐
///  │  router   │
///  └────┬──────┘
///       │ conditional: route_type
///       ├─ Chat ─────────────────→ simple_chat → (end)
///       ├─ Simple ───────────────→ simple_exec → action_exec → summarizer → (end)
///       ├─ Complex ──────────────→ planner ──┐
///       └─ ComplexVisual ────────→ planner ──┘
///                                      │
///                                      ▼
///                               ┌──────────────┐
///                               │  step_router  │ ← decides mode per step
///                               └──────┬───────┘
///                                 ┌────┤
///                                 │    ├─ Combo ──→ combo_exec → step_advance
///                                 │    ├─ Chat  ──→ chat_agent ──┐
///                                 │    └─ Vlm   ──→ vlm_act ────┘
///                                 │                       │
///                                 │                       ▼
///                                 │               ┌──────────────┐
///                                 │               │  action_exec  │ ← executes one action
///                                 │               └──────┬───────┘
///                                 │                      │
///                                 │                      ▼
///                                 │              ┌───────────────┐
///                                 │              │ step_evaluate  │ ← inner loop control
///                                 │              └───────┬───────┘
///                                 │                      │
///                                 │    ┌─────────────────┤
///                                 │    │ step_complete    │ continue loop
///                                 │    ▼                  ▼
///                                 │  step_advance    chat_agent / vlm_act
///                                 │    │
///                                 │    │ conditional: more steps?
///                                 │    ├─ yes → step_router (loop)
///                                 │    └─ no  → verifier
///                                 │                │
///                                 │    ┌───────────┤
///                                 │    ▼           ▼
///                                 │ summarizer   planner (replan)
///                                 │    │
///                                 │    ▼
///                                 │  (end)
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
            RouteType::Chat => "simple_chat".to_string(),
            RouteType::Simple => "simple_exec".to_string(),
            RouteType::Complex => "planner".to_string(),
            RouteType::ComplexVisual => "planner".to_string(),
        }
    });

    // ── SimpleChatNode → end (always) ────────────────────────────────────
    // SimpleChatNode always returns End — no outgoing edge needed.

    // ── SimpleExec → action_exec ─────────────────────────────────────
    graph.add_edge("simple_exec", "action_exec");

    // ── Planner → step_router (node itself returns GoTo or End) ─────────
    graph.add_edge("planner", "step_router");

    // ── StepRouter → GoTo target (combo_exec / chat_agent / vlm_act)
    // StepRouterNode uses GoTo(), so no static edge strictly needed,
    // but we add a fallback.
    graph.add_edge("step_router", "chat_agent");

    // ── ComboExec → step_advance (combo bypasses the inner loop) ────────
    graph.add_edge("combo_exec", "step_advance");

    // ── ChatAgent → action_exec (Continue = go execute the action) ──────
    // ChatAgent may also GoTo("step_evaluate") or GoTo("step_router").
    graph.add_edge("chat_agent", "action_exec");

    // ── VlmAct → action_exec (Continue = go execute the action) ─────────
    // VlmAct may also GoTo("step_evaluate") or GoTo("step_router").
    graph.add_edge("vlm_act", "action_exec");

    // ── ActionExec → conditional: approval / stability / step_evaluate ──
    graph.add_conditional_edge("action_exec", |state| {
        if state.needs_approval {
            "user_confirm".to_string()
        } else if state.todo_steps.is_empty() {
            // Simple route or direct action from planner: no todo_steps → go to summarizer
            "summarizer".to_string()
        } else if state.needs_stability {
            "stability".to_string()
        } else {
            "step_evaluate".to_string()
        }
    });

    // ── UserConfirm → action_exec (node uses GoTo) ─────────────────────
    graph.add_edge("user_confirm", "action_exec");

    // ── Stability → step_evaluate ───────────────────────────────────────
    graph.add_edge("stability", "step_evaluate");

    // ── StepEvaluate → conditional: loop back or advance ────────────────
    // StepEvaluateNode uses GoTo() for all routing. Fallback:
    graph.add_edge("step_evaluate", "step_advance");

    // ── StepAdvance → conditional: more steps, verifier, or skip verifier ──
    graph.add_conditional_edge("step_advance", |state| {
        if state.current_step_idx < state.todo_steps.len() {
            "step_router".to_string()
        } else {
            // All steps done — check if any failed.
            // If all succeeded, skip verifier (saves one VLM call + screenshot).
            let has_failure = state.todo_steps.iter().any(|s| {
                matches!(s.status, StepStatus::Failed | StepStatus::Skipped)
            });
            if has_failure {
                "verifier".to_string()
            } else {
                tracing::info!("[StepAdvance] all steps succeeded → skip verifier → summarizer");
                "summarizer".to_string()
            }
        }
    });

    // ── Verifier → summarizer (pass) or planner (fail) ──────────────────
    graph.add_edge("verifier", "summarizer");

    // ── Summarizer → end (always) ──────────────────────────────────────
    // SummarizerNode always returns End. No outgoing edge needed.

    graph
}
