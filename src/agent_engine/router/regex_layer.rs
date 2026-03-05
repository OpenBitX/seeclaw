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
            (Regex::new(r"(?i)^(你好|你好呀|你好吗|你好啊|你好嘞|hello|hi|hey|hai|嗨|哈喽|早上好|晚上好|下午好|good\s*(morning|afternoon|evening)|早安|晚安)[!！。.~～\s]*$").unwrap(), RouteType::Chat),
            (Regex::new(r"(?i)^(你是谁|你是什么|who\s+are\s+you|你叫什么|what('s| is) your name)[?？!！。.]*$").unwrap(), RouteType::Chat),
            (Regex::new(r"(?i)^(谢谢|thanks|thank\s+you|多谢|感谢)[!！。.~～\s]*$").unwrap(), RouteType::Chat),
            (Regex::new(r"(?i)^(再见|bye|goodbye|拜拜|see\s+you)[!！。.~～\s]*$").unwrap(), RouteType::Chat),
            // Broader greeting/conversational patterns that include trailing particles
            // e.g. "你好吗", "还好吗", "在吗", "在干嘛" etc.
            (Regex::new(r"(?i)^[你您]\s*好.{0,4}[?？!！。.~～\s]*$").unwrap(), RouteType::Chat),
            (Regex::new(r"(?i)^(在吗|在不在|在干嘛|干什么呢|还好吗|开心吗|忙吗|最近怎么样)[?？!！。.~～\s]*$").unwrap(), RouteType::Chat),
            // Simple math / factual lookups (no GUI needed)
            (Regex::new(r"(?i)^\d+\s*[\+\-\*\/×÷]\s*\d+\s*(等于几|等于多少|=\s*\?|[?？])\s*$").unwrap(), RouteType::Chat),
            // "你会什么" / "你能做什么" / "what can you do"
            (Regex::new(r"(?i)^(你会什么|你能做什么|你有什么(技能|功能|能力)|what\s+can\s+you\s+do)[?？!！。.]*$").unwrap(), RouteType::Chat),

            // ── Simple patterns: single direct actions ──
            (Regex::new(r"(?i)^(打开|启动|运行|关闭|最小化|最大化)\s*[\u4e00-\u9fa5a-zA-Z0-9._\-]{1,30}\s*[!！。.]*$").unwrap(), RouteType::Simple),
            (Regex::new(r"(?i)^(open|launch|start|close|minimize|maximize)\s+[\w.-]{1,30}\s*$").unwrap(), RouteType::Simple),
            (Regex::new(r"(?i)^(按|press)\s+(ctrl|alt|shift|win|enter|tab|esc)").unwrap(), RouteType::Simple),

            // ── ComplexVisual patterns: tasks that explicitly reference screen content ──
            // These need the planner to see the current screen before generating a plan.
            // "点击" / "click" tasks always require vision to know WHERE to click.
            // Allow optional prefixes like "帮我"/"请"/"能不能"/"帮忙" before the click verb.
            (Regex::new(r"(?i)^(帮我|请|帮忙|能不能|能否|麻烦)?\s*(点击|双击|右键点击|click|double.?click|right.?click)\s*[\u4e00-\u9fa5a-zA-Z0-9]{1,30}\s*[!！。.]*$").unwrap(), RouteType::ComplexVisual),
            // "点击" + "图标/按钮/链接" with generous gap (up to 30 chars between)
            (Regex::new(r"(?i)(点击|双击|右键|click).{0,30}(图标|按钮|链接|icon|button)").unwrap(), RouteType::ComplexVisual),
            // Any sentence containing a click verb — it always needs vision
            (Regex::new(r"(?i)(帮我|请|帮忙)?.{0,10}(点击|双击|右键点击|click|double.?click|right.?click)").unwrap(), RouteType::ComplexVisual),
            (Regex::new(r"(?i)(屏幕上|画面上|当前(屏幕|页面|窗口)|看到的|显示的|on\s*screen|current\s*(screen|page|window))").unwrap(), RouteType::ComplexVisual),
            (Regex::new(r"(?i)^(截图|截屏|screenshot)").unwrap(), RouteType::ComplexVisual),
            (Regex::new(r"(?i)(那个(按钮|图标|窗口)|这个(按钮|图标|窗口))").unwrap(), RouteType::ComplexVisual),

            // ── Complex patterns: multi-step or vague tasks ──
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
