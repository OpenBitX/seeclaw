//! L1: Regex keyword matching layer.
//!
//! Maintains a mapping of regex patterns → route types.
//! Fastest layer — pure string matching, no I/O.

use async_trait::async_trait;
use regex::Regex;

use crate::agent_engine::context::NodeContext;
use crate::agent_engine::router::layer::{RouteResult, RouterLayer};
use crate::agent_engine::state::RouteType;

/// Regex-based router layer.
pub struct RegexLayer {
    /// (pattern, route_type) pairs — checked in order.
    rules: Vec<(Regex, RouteType)>,
}

impl RegexLayer {
    pub fn new() -> Self {
        let rules = vec![
            // ── Chat patterns: greetings, knowledge Q&A, casual conversation ──
            // These require NO tools or GUI operations at all.
            (Regex::new(r"(?i)^(你好|hello|hi|hey|嗨|哈喽|早上好|晚上好|下午好|good\s*(morning|afternoon|evening)|早安|晚安)[!！。.~～\s]*$").unwrap(), RouteType::Chat),
            (Regex::new(r"(?i)^(你是谁|who\s+are\s+you|你叫什么|what('s| is) your name)[?？!！。.]*$").unwrap(), RouteType::Chat),
            (Regex::new(r"(?i)^(谢谢|thanks|thank\s+you|多谢|感谢)[!！。.~～\s]*$").unwrap(), RouteType::Chat),
            (Regex::new(r"(?i)^(再见|bye|goodbye|拜拜|see\s+you)[!！。.~～\s]*$").unwrap(), RouteType::Chat),
            // Simple math / factual lookups (no GUI needed)
            (Regex::new(r"(?i)^\d+\s*[\+\-\*\/×÷]\s*\d+\s*(等于几|等于多少|=\s*\?|[?？])\s*$").unwrap(), RouteType::Chat),
            // "你会什么" / "你能做什么" / "what can you do"
            (Regex::new(r"(?i)^(你会什么|你能做什么|你有什么(技能|功能|能力)|what\s+can\s+you\s+do)[?？!！。.]*$").unwrap(), RouteType::Chat),

            // ── Simple patterns: single direct actions ──
            // IMPORTANT: Chinese has no spaces so \S+ matches entire sentences.
            // We restrict target length (≤8 CJK chars) to avoid mis-classifying
            // multi-intent sentences like "打开浏览器搜索今天天气" as Simple.
            (Regex::new(r"(?i)^(打开|启动|运行|关闭|最小化|最大化)\s*[\u4e00-\u9fa5a-zA-Z0-9]{1,8}\s*[!！。.]*$").unwrap(), RouteType::Simple),
            (Regex::new(r"(?i)^(open|launch|start|close|minimize|maximize)\s+[\w.-]{1,30}\s*$").unwrap(), RouteType::Simple),
            (Regex::new(r"(?i)^(点击|click)\s+[\u4e00-\u9fa5a-zA-Z0-9]{1,15}\s*$").unwrap(), RouteType::Simple),
            (Regex::new(r"(?i)^(按|press)\s+(ctrl|alt|shift|win|enter|tab|esc)").unwrap(), RouteType::Simple),
            // NOTE: Single-command info retrieval (IP, OS version, etc.) is NOT
            // matched here because RegexLayer cannot generate tool_calls.
            // These queries fall through to LlmLayer which can classify as Simple
            // AND produce the appropriate execute_terminal tool call in one shot.
            // ── Complex patterns: multi-step or vague tasks ──
            // NOTE: conversational queries (你好, 你是谁, etc.) are intentionally NOT matched here.
            // They fall through to the LLM layer and default to Complex, which is correct:
            // PlannerNode handles content-only LLM responses gracefully.
            (Regex::new(r"(?i)(搜索|查找|查看|浏览|下载|安装|配置|设置|修改|编辑|创建|删除).*并").unwrap(), RouteType::Complex),
            (Regex::new(r"(?i)(然后|接着|之后|同时|并且)").unwrap(), RouteType::Complex),
            (Regex::new(r"(?i)(帮我|请|能不能).{20,}").unwrap(), RouteType::Complex),
        ];
        Self { rules }
    }
}

#[async_trait]
impl RouterLayer for RegexLayer {
    fn name(&self) -> &str {
        "regex"
    }

    async fn classify(&self, query: &str, _ctx: &NodeContext) -> Option<RouteResult> {
        for (pattern, route_type) in &self.rules {
            if pattern.is_match(query) {
                tracing::debug!(
                    layer = "regex",
                    pattern = %pattern,
                    route = ?route_type,
                    "regex match found"
                );
                return Some(RouteResult {
                    route_type: route_type.clone(),
                    confidence: 1.0,
                });
            }
        }
        None
    }
}
