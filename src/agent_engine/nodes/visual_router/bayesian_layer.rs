//! L2: Keyword-weighted scoring layer.
//!
//! Assigns log-odds weights to vocabulary terms observed in the goal string.
//! Positive weight → evidence of visual intent (user wants to *read* screen content).
//! Negative weight → evidence of action-only intent (user wants to *do* something).
//!
//! The net score is mapped to a confidence value. If confidence exceeds
//! `THRESHOLD`, a decision is returned; otherwise the query passes to L3.
//!
//! This layer requires no model file — weights are compiled in based on
//! corpus analysis of typical agent task phrasings.

use async_trait::async_trait;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::nodes::visual_router::layer::{VisualDecisionLayer, VisualDecisionResult};
use crate::agent_engine::state::TodoStep;

/// Minimum confidence required to return a result instead of passing to L3.
const THRESHOLD: f32 = 0.72;

/// `(keyword, weight)` — positive = visual intent, negative = action-only intent.
///
/// Weights are log-odds style: a single strong signal (±1.0) is enough to
/// decide confidently on its own; multiple weak signals reinforce each other.
static KEYWORD_WEIGHTS: &[(&str, f32)] = &[
    // ── Visual intent (positive) ─────────────────────────────────────────
    // Strong signals
    ("新鲜事",    1.0),  ("热门",      0.8),  ("新闻",      0.8),
    ("what's on", 1.0),  ("news",      0.9),  ("tell me",   0.7),
    // Medium signals
    ("告诉我",    0.6),  ("有什么",    0.5),  ("是什么",    0.5),
    ("内容",      0.4),  ("信息",      0.4),  ("消息",      0.5),
    ("显示",      0.4),  ("页面",      0.5),  ("网页",      0.5),
    ("看看",      0.4),  ("浏览",      0.4),  ("阅读",      0.6),
    ("最新",      0.4),  ("推荐",      0.3),  ("今天",      0.3),
    ("browse",    0.5),  ("read",      0.5),  ("show me",   0.5),
    ("content",   0.4),  ("view",      0.4),  ("whats",     0.4),
    ("search results", 0.7),

    // ── Action-only intent (negative) ────────────────────────────────────
    // Strong signals
    ("打开",     -0.5),  ("启动",     -0.5),  ("关闭",     -0.5),
    ("open",     -0.5),  ("launch",   -0.6),  ("close",    -0.5),
    ("start",    -0.4),
    // Medium signals
    ("运行",     -0.4),  ("创建",     -0.5),  ("删除",     -0.5),
    ("保存",     -0.4),  ("输入",     -0.4),  ("按下",     -0.4),
    ("复制",     -0.3),  ("粘贴",     -0.3),  ("移动",     -0.3),
    ("重命名",   -0.4),  ("安装",     -0.4),
    ("run",      -0.4),  ("execute",  -0.4),  ("create",   -0.5),
    ("delete",   -0.5),  ("type",     -0.4),  ("press",    -0.4),
    ("copy",     -0.3),  ("paste",    -0.3),  ("move",     -0.3),
    ("install",  -0.4),  ("save",     -0.4),
];

pub struct VisualBayesianLayer;

impl VisualBayesianLayer {
    pub fn new() -> Self { Self }

    fn score(goal: &str) -> f32 {
        let lower = goal.to_lowercase();
        KEYWORD_WEIGHTS
            .iter()
            .filter(|(kw, _)| lower.contains(kw))
            .map(|(_, w)| w)
            .sum()
    }
}

#[async_trait]
impl VisualDecisionLayer for VisualBayesianLayer {
    fn name(&self) -> &str { "visual_bayesian" }

    async fn classify(
        &self,
        goal: &str,
        _steps_log: &[String],
        _todo_steps: &[TodoStep],
        _ctx: &NodeContext,
    ) -> Option<VisualDecisionResult> {
        let raw_score = Self::score(goal);
        // Map |raw_score| → confidence, saturating at 1.0.
        // A single strong signal (score ~1.0) → confidence ~0.67; two strong → ~0.89.
        let confidence = (raw_score.abs() / 1.5).min(1.0);

        if confidence < THRESHOLD {
            tracing::debug!(
                layer = "visual_bayesian",
                raw_score,
                confidence,
                threshold = THRESHOLD,
                "score below threshold — passing to L3"
            );
            return None;
        }

        let needs_visual = raw_score > 0.0;
        tracing::debug!(
            layer = "visual_bayesian",
            raw_score,
            confidence,
            needs_visual,
            "visual decision accepted"
        );
        Some(VisualDecisionResult { needs_visual, confidence })
    }
}
