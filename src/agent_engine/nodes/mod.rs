//! All node implementations, plus a helper to register them into a graph.

pub mod action_exec;
pub mod combo_exec;
pub mod direct_exec;
pub mod planner;
pub mod router;
pub mod simple_chat;
pub mod simple_exec;
pub mod stability;
pub mod step_advance;
pub mod step_dispatch;
pub mod summarizer;
pub mod user_confirm;
pub mod verifier;
pub mod visual_router;
pub mod vlm_act;
pub mod vlm_observe;

use crate::agent_engine::graph::Graph;

/// Register all standard nodes into the given graph.
pub fn register_all_nodes(graph: &mut Graph) {
    graph.add_node(Box::new(router::RouterNode::new()));
    graph.add_node(Box::new(simple_chat::SimpleChatNode::new()));
    graph.add_node(Box::new(simple_exec::SimpleExecNode::new()));
    graph.add_node(Box::new(planner::PlannerNode::new()));
    graph.add_node(Box::new(step_dispatch::StepDispatchNode::new()));
    graph.add_node(Box::new(combo_exec::ComboExecNode::new()));
    graph.add_node(Box::new(direct_exec::DirectExecNode::new()));
    graph.add_node(Box::new(vlm_observe::VlmObserveNode::new()));
    graph.add_node(Box::new(vlm_act::VlmActNode::new()));
    graph.add_node(Box::new(action_exec::ActionExecNode::new()));
    graph.add_node(Box::new(user_confirm::UserConfirmNode::new()));
    graph.add_node(Box::new(stability::StabilityNode::new()));
    graph.add_node(Box::new(step_advance::StepAdvanceNode::new()));
    graph.add_node(Box::new(summarizer::SummarizerNode::new()));
    graph.add_node(Box::new(verifier::VerifierNode::new()));
}
