//! L1: Regex pattern matching — fast, deterministic, zero I/O.
//!
//! Two rule lists are checked in order:
//! - `visual_patterns`  → needs_visual = true  (confidence 1.0)
//! - `action_patterns`  → needs_visual = false (confidence 1.0)
//!
//! Any `VisualAct` step in the plan is an unconditional visual signal:
//! the agent was already doing autonomous visual reasoning, so the final
//! screen state is directly relevant to the answer.

use async_trait::async_trait;
use regex::Regex;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::nodes::visual_router::layer::{VisualDecisionLayer, VisualDecisionResult};
use crate::agent_engine::state::{StepMode, TodoStep};

pub struct VisualRegexLayer {
    /// Patterns that indicate the user wants to *read* on-screen content.
    visual_patterns: Vec<Regex>,
    /// Patterns that indicate a pure action with no reading intent.
    action_patterns: Vec<Regex>,
}

impl VisualRegexLayer {
    pub fn new() -> Self {
        let visual_patterns = vec![
            // Chinese: read / view / news / what's on screen
            Regex::new(r"(?i)(告诉我|有什么|新鲜事|热门|这个页面|网页内容|显示的是|看看|读一下|浏览一下|查看内容|页面上|首页内容|推荐内容|今天.*?内容)").unwrap(),
            // Chinese: "是什么" / "有哪些" after browsing verb
            Regex::new(r"(?i)(打开|浏览|访问).{0,20}(后|然后|并).{0,30}(是什么|有什么|告诉|看看|新鲜|内容|信息|消息)").unwrap(),
            // English: reading / viewing / what's on
            Regex::new(r"(?i)\b(tell me what[' ]s? (on|in|there)|read (the|this|page)|show me (what|the contents?)|what[' ]s? (on|shown|displayed)|news on|content of|browse.*and tell)\b").unwrap(),
        ];

        let action_patterns = vec![
            // Pure single-verb actions with short target (≤12 chars / words)
            Regex::new(r"(?i)^(打开|启动|运行|关闭|最小化|最大化)\s*[\u4e00-\u9fa5a-zA-Z0-9]{1,12}\s*[!！。.]*$").unwrap(),
            Regex::new(r"(?i)^(open|launch|start|close|minimize|maximize)\s+[\w\s.\-]{1,30}\s*$").unwrap(),
            // Typing / key press
            Regex::new(r"(?i)^(输入|键入|按下|按住)\s*.{1,50}$").unwrap(),
            Regex::new(r"(?i)^(type|press|hit)\s+.{1,50}$").unwrap(),
        ];

        Self { visual_patterns, action_patterns }
    }
}

#[async_trait]
impl VisualDecisionLayer for VisualRegexLayer {
    fn name(&self) -> &str { "visual_regex" }

    async fn classify(
        &self,
        goal: &str,
        _steps_log: &[String],
        todo_steps: &[TodoStep],
        _ctx: &NodeContext,
    ) -> Option<VisualDecisionResult> {
        // Vlm steps = agent was doing autonomous visual reasoning → definitely needs visual
        if todo_steps.iter().any(|s| s.mode == StepMode::Vlm) {
            tracing::debug!(layer = "visual_regex", "Vlm step detected → needs_visual=true");
            return Some(VisualDecisionResult { needs_visual: true, confidence: 1.0 });
        }

        for pat in &self.visual_patterns {
            if pat.is_match(goal) {
                tracing::debug!(layer = "visual_regex", pattern = %pat, "visual pattern matched → needs_visual=true");
                return Some(VisualDecisionResult { needs_visual: true, confidence: 1.0 });
            }
        }

        for pat in &self.action_patterns {
            if pat.is_match(goal) {
                tracing::debug!(layer = "visual_regex", pattern = %pat, "action pattern matched → needs_visual=false");
                return Some(VisualDecisionResult { needs_visual: false, confidence: 1.0 });
            }
        }

        None // pass to L2
    }
}
