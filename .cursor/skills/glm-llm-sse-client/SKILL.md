---
name: multi-provider-llm-client
description: Implement multi-provider LLM client in Rust with SSE streaming and Tauri event forwarding. Use when asked to "connect to LLM", "implement LLM client", "set up SSE streaming", "add a new LLM provider", "forward LLM chunks to frontend", or "implement the planner stage".
argument-hint: <target-module>
---

# Skill: Multi-Provider LLM Client with SSE Streaming (Rust + Tauri)

## Overview

The LLM layer is built on a `LlmProvider` trait. Each provider (Zhipu/OpenAI/Qwen)
implements this trait. The active provider is selected at runtime from `config.toml`.
All providers share the same SSE parsing and Tauri event forwarding mechanism.

## Step 1 — Cargo.toml Dependencies

```toml
[dependencies]
reqwest = { version = "0.12", features = ["json", "stream"] }
tokio-stream = "0.1"
futures-util = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
async-trait = "0.1"
toml = "0.8"
```

## Step 2 — Shared Data Structures

```rust
// src/llm/types.rs

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: serde_json::Value,  // string or array for multimodal
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, serde::Deserialize)]
pub struct SseDelta {
    pub role: Option<String>,
    pub content: Option<String>,
    pub reasoning_content: Option<String>,
    pub tool_calls: Option<Vec<ToolCallChunk>>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ToolCallChunk {
    pub index: u32,
    pub id: Option<String>,
    pub function: FunctionChunk,
}

#[derive(Debug, serde::Deserialize)]
pub struct FunctionChunk {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

#[derive(Debug, serde::Serialize, Clone)]
pub struct LlmStreamChunk {
    pub reasoning: Option<String>,
    pub content: Option<String>,
    pub tool_call_index: Option<u32>,
    pub tool_call_name: Option<String>,
    pub tool_call_args_delta: Option<String>,
    pub done: bool,
}
```

## Step 3 — Provider Trait

```rust
// src/llm/provider.rs

use async_trait::async_trait;
use tauri::AppHandle;
use crate::llm::types::{ChatMessage, ToolDef};
use crate::errors::SeeClawError;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn stream_chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDef>,
        app: &AppHandle,
    ) -> Result<(), SeeClawError>;
}
```

## Step 4 — Config-Driven Provider Registry

```rust
// src/llm/registry.rs

use std::collections::HashMap;
use crate::llm::provider::LlmProvider;
use crate::llm::providers::{ZhipuProvider, OpenAiCompatibleProvider};
use crate::config::AppConfig;

pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn LlmProvider>>,
    active: String,
}

impl ProviderRegistry {
    pub fn from_config(config: &AppConfig) -> Self {
        let mut providers: HashMap<String, Box<dyn LlmProvider>> = HashMap::new();

        for (name, p) in &config.llm.providers {
            let api_key = std::env::var(format!("SEECLAW_{}_API_KEY", name.to_uppercase()))
                .unwrap_or_default();

            let provider = OpenAiCompatibleProvider::new(
                name.clone(),
                p.api_base.clone(),
                p.model.clone(),
                api_key,
                p.temperature,
            );
            providers.insert(name.clone(), Box::new(provider));
        }

        Self {
            active: config.llm.active_provider.clone(),
            providers,
        }
    }

    pub fn active_provider(&self) -> &dyn LlmProvider {
        self.providers.get(&self.active)
            .expect("active provider not found in registry")
            .as_ref()
    }
}
```

## Step 5 — OpenAI-Compatible Provider (Works for Zhipu, OpenAI, Qwen)

Most LLM APIs follow the OpenAI chat completions format. One implementation covers all.

```rust
// src/llm/providers/openai_compat.rs

use futures_util::StreamExt;
use reqwest::Client;
use tauri::AppHandle;

use crate::llm::provider::LlmProvider;
use crate::llm::types::*;
use crate::llm::sse_parser::parse_sse_line;
use crate::errors::SeeClawError;

pub struct OpenAiCompatibleProvider {
    name: String,
    http: Client,
    api_base: String,
    model: String,
    api_key: String,
    temperature: f64,
}

impl OpenAiCompatibleProvider {
    pub fn new(name: String, api_base: String, model: String, api_key: String, temperature: f64) -> Self {
        Self { name, http: Client::new(), api_base, model, api_key, temperature }
    }
}

#[async_trait::async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    fn name(&self) -> &str { &self.name }

    async fn stream_chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDef>,
        app: &AppHandle,
    ) -> Result<(), SeeClawError> {
        let body = serde_json::json!({
            "model": &self.model,
            "messages": messages,
            "tools": tools,
            "stream": true,
            "temperature": self.temperature,
        });

        let response = self.http
            .post(&self.api_base)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| SeeClawError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(SeeClawError::LlmApiError(format!("{status}: {body}")));
        }

        let mut byte_stream = response.bytes_stream();
        let mut line_buf = String::new();

        while let Some(chunk) = byte_stream.next().await {
            let chunk = chunk.map_err(|e| SeeClawError::NetworkError(e.to_string()))?;
            let text = String::from_utf8_lossy(&chunk);

            for ch in text.chars() {
                if ch == '\n' {
                    let line = line_buf.trim().to_string();
                    line_buf.clear();

                    if line == "data: [DONE]" {
                        let _ = app.emit("llm_stream_chunk", LlmStreamChunk {
                            reasoning: None, content: None,
                            tool_call_index: None, tool_call_name: None,
                            tool_call_args_delta: None, done: true,
                        });
                        return Ok(());
                    }

                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if let Ok(parsed) = parse_sse_line(json_str) {
                            let _ = app.emit("llm_stream_chunk", parsed);
                        }
                    }
                } else {
                    line_buf.push(ch);
                }
            }
        }
        Ok(())
    }
}
```

## Step 6 — SSE Line Parser (Shared)

```rust
// src/llm/sse_parser.rs

use crate::llm::types::*;

pub fn parse_sse_line(json_str: &str) -> Result<LlmStreamChunk, serde_json::Error> {
    let v: serde_json::Value = serde_json::from_str(json_str)?;
    let delta_val = &v["choices"][0]["delta"];
    let delta: SseDelta = serde_json::from_value(delta_val.clone())?;

    let (tool_idx, tool_name, tool_args) = if let Some(calls) = &delta.tool_calls {
        if let Some(call) = calls.first() {
            (Some(call.index), call.function.name.clone(), call.function.arguments.clone())
        } else {
            (None, None, None)
        }
    } else {
        (None, None, None)
    };

    Ok(LlmStreamChunk {
        reasoning: delta.reasoning_content,
        content: delta.content,
        tool_call_index: tool_idx,
        tool_call_name: tool_name,
        tool_call_args_delta: tool_args,
        done: false,
    })
}
```

## Step 7 — Load Tools from prompts/tools/builtin.json

Tool definitions are **NOT hardcoded in Rust**. They live in `prompts/tools/builtin.json`
and are loaded at runtime. This means prompts can be edited without recompiling.

```rust
// src/llm/tools.rs

use crate::llm::types::ToolDef;
use crate::errors::SeeClawError;

/// Built-in tool definitions loaded from prompts/tools/builtin.json at compile time.
/// The file is embedded into the binary so it works in release builds too.
const BUILTIN_TOOLS_JSON: &str = include_str!("../../prompts/tools/builtin.json");

pub fn load_builtin_tools() -> Result<Vec<ToolDef>, SeeClawError> {
    serde_json::from_str(BUILTIN_TOOLS_JSON)
        .map_err(|e| SeeClawError::ConfigError(format!("failed to parse builtin tools: {e}")))
}

/// Merge built-in tools with dynamically registered MCP tools and Skill tools.
pub fn build_full_tool_list(
    mcp_tools: Vec<ToolDef>,
    skill_tools: Vec<ToolDef>,
) -> Result<Vec<ToolDef>, SeeClawError> {
    let mut tools = load_builtin_tools()?;
    tools.extend(mcp_tools);
    tools.extend(skill_tools);
    Ok(tools)
}
```

The complete tool list is defined in `prompts/tools/builtin.json` — see `system-design.md`
section IV for the full JSON. Tools covered:

| Tool | Category | Description |
|---|---|---|
| `mouse_click` | Input | Single left-click by element ID |
| `mouse_double_click` | Input | Double-click by element ID |
| `mouse_right_click` | Input | Right-click (context menu) |
| `scroll` | Input | Scroll with `direction` + `distance` (short=3lines / long=1page) |
| `type_text` | Input | Type text (CJK → clipboard+Ctrl+V) |
| `hotkey` | Input | Keyboard shortcut (e.g. `ctrl+shift+esc`) |
| `key_press` | Input | Single key (Enter / Tab / Escape / Arrow keys) |
| `get_viewport` | Perception | Fresh screenshot with annotated element IDs |
| `execute_terminal` | System | PowerShell command — requires human approval |
| `mcp_call` | Extension | Call a connected MCP server tool |
| `invoke_skill` | Extension | Run a user-imported skill package |
| `wait` | Control | Wait N milliseconds |
| `finish_task` | Control | Signal task completion |
| `report_failure` | Control | Signal unrecoverable failure |

## Step 8 — Frontend: Listening to Stream Events (MobX)

```typescript
// src-ui/src/store/agentStore.ts
import { makeAutoObservable, action } from 'mobx';
import { listen } from '@tauri-apps/api/event';
import type { LlmStreamChunk } from '../types/llm';

class AgentStore {
  reasoningStream = '';
  contentStream = '';
  pendingToolCallArgs: Record<number, string> = {};

  constructor() {
    makeAutoObservable(this);
    this.initStreamListener();
  }

  private initStreamListener() {
    listen<LlmStreamChunk>('llm_stream_chunk', action(({ payload }) => {
      if (payload.done) {
        this.flushPendingToolCall();
        return;
      }
      if (payload.reasoning) this.reasoningStream += payload.reasoning;
      if (payload.content) this.contentStream += payload.content;
      if (payload.tool_call_index != null && payload.tool_call_args_delta) {
        this.pendingToolCallArgs[payload.tool_call_index] =
          (this.pendingToolCallArgs[payload.tool_call_index] ?? '') + payload.tool_call_args_delta;
      }
    }));
  }

  private flushPendingToolCall() {
    // Assembled tool call args — hand off to engine via invoke
  }
}

export const agentStore = new AgentStore();
```

```typescript
// src-ui/src/types/llm.ts
export interface LlmStreamChunk {
  reasoning?: string;
  content?: string;
  tool_call_index?: number;
  tool_call_name?: string;
  tool_call_args_delta?: string;
  done: boolean;
}
```

## config.toml — Complete Provider Configuration

All providers follow the same `[llm.providers.{id}]` schema.
Most use the OpenAI-compatible format — one Rust implementation covers all of them.

```toml
[llm]
active_provider = "zhipu"

# ── OpenAI-compatible providers (use OpenAiCompatibleProvider) ──────────────

[llm.providers.zhipu]
display_name = "智谱 GLM"
api_base = "https://open.bigmodel.cn/api/paas/v4/chat/completions"
model = "glm-4v-plus"
temperature = 0.1

[llm.providers.openai]
display_name = "OpenAI"
api_base = "https://api.openai.com/v1/chat/completions"
model = "gpt-4o"
temperature = 0.2

[llm.providers.deepseek]
display_name = "DeepSeek"
api_base = "https://api.deepseek.com/v1/chat/completions"
model = "deepseek-chat"
temperature = 0.1

[llm.providers.qwen]
display_name = "阿里云 Qwen"
api_base = "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
model = "qwen-vl-max"
temperature = 0.1

[llm.providers.openrouter]
display_name = "OpenRouter"
api_base = "https://openrouter.ai/api/v1/chat/completions"
model = "anthropic/claude-3.5-sonnet"
temperature = 0.2

# ── Non-OpenAI format (requires separate ClaudeProvider implementation) ──────

[llm.providers.claude]
display_name = "Anthropic Claude"
api_base = "https://api.anthropic.com/v1/messages"
model = "claude-opus-4-5"
temperature = 0.2
adapter = "anthropic"           # signals registry to use ClaudeProvider

# ── User-added custom provider (via UI Settings → Add Provider) ──────────────
# [llm.providers.my_local]
# display_name = "本地 Ollama"
# api_base = "http://localhost:11434/v1/chat/completions"
# model = "qwen2.5-vl"
# temperature = 0.3
```

## .env — API Keys (never commit)

Variable naming convention: `SEECLAW_{PROVIDER_ID_UPPERCASE}_API_KEY`

```bash
SEECLAW_ZHIPU_API_KEY=your_glm_key
SEECLAW_OPENAI_API_KEY=sk-...
SEECLAW_DEEPSEEK_API_KEY=sk-...
SEECLAW_QWEN_API_KEY=sk-...
SEECLAW_OPENROUTER_API_KEY=sk-or-...
SEECLAW_CLAUDE_API_KEY=sk-ant-...
```

## Step 9 — System Prompt Loading & Language Detection

```rust
// src/llm/prompt_builder.rs

use crate::perception::PerceptionContext;

const SYSTEM_TEMPLATE: &str = include_str!("../../prompts/system/agent_system.md");

/// Detect if the input text is primarily Chinese (CJK).
fn detect_language_hint(text: &str) -> &'static str {
    let has_cjk = text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c));
    if has_cjk {
        "The user speaks Chinese. Always respond in Chinese (Simplified)."
    } else {
        "The user speaks English. Always respond in English."
    }
}

pub fn build_system_prompt(
    ctx: &PerceptionContext,
    experience_context: &str,
    skills_list: &str,
    mcp_tools_list: &str,
    user_goal: &str,
) -> String {
    let elements_xml = build_elements_xml(ctx);
    let language_hint = detect_language_hint(user_goal);

    SYSTEM_TEMPLATE
        .replace("{elements_xml}", &elements_xml)
        .replace("{experience_context}", experience_context)
        .replace("{skills_list}", skills_list)
        .replace("{mcp_tools_list}", mcp_tools_list)
        .replace("{user_language_hint}", language_hint)
}

fn build_elements_xml(ctx: &PerceptionContext) -> String {
    let mut xml = String::from("<elements>\n");
    for el in &ctx.elements {
        xml.push_str(&format!(
            "  <node id=\"{}\" type=\"{:?}\" text=\"{}\" conf=\"{:.2}\"/>\n",
            el.id, el.node_type,
            el.content.as_deref().unwrap_or(""),
            el.confidence,
        ));
    }
    xml.push_str("</elements>");
    xml
}
```

## Important Notes

- **Tool definitions are NOT hardcoded** — they live in `prompts/tools/builtin.json` and are loaded via `include_str!`
- Most providers (GLM / OpenAI / DeepSeek / Qwen / OpenRouter) share one `OpenAiCompatibleProvider` Rust impl — no extra code needed
- Claude uses a separate `ClaudeProvider` due to different request/response format
- API keys are ALWAYS read from `.env` environment variables, NEVER from `config.toml`
- Adding a new OpenAI-compatible provider: add `[llm.providers.xxx]` in `config.toml` + set env var — zero Rust code needed
- Language detection is automatic from user input — no separate locale setting required
- `temperature: 0.1` for tool calls keeps output deterministic
- Log every outgoing request and incoming SSE chunk with `tracing::debug!`
