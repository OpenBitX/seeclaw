# SeeClaw 系统核心架构与工作流白皮书

## 一、 终极技术栈规范 (Tech Stack Standard)

### 底层引擎 (Rust Core — `src/`)

| 职责 | Crate | 说明 |
|---|---|---|
| 宿主框架 | `tauri 2` | 轻量桌面容器，无 Node/Python 运行时 |
| 异步运行时 | `tokio` | 承载 LLM 请求、状态机轮询、MCP 调用 |
| 本地视觉推理 | `ort 2` | ONNX Runtime（可选，YOLO-Nano UI 检测） |
| 截屏 | `xcap` | 跨平台截屏，含 DPI metadata |
| 键鼠模拟 | `enigo` | 系统级物理输入（支持中英文） |
| 错误处理 | `thiserror` | 统一 `SeeClawError` 枚举 |
| 日志 | `tracing` | 结构化日志 |
| 配置 | `toml` + `serde` | 读取根目录 `config.toml` |
| 轻量向量 DB | `usearch` 或 `hnsw_rs` | 本地 RAG 索引（无外部服务依赖） |
| HTTP 客户端 | `reqwest` | SSE 流式 + REST 调用 |

### 交互表现层 (UI — `src-ui/`)

| 职责 | 依赖 | 说明 |
|---|---|---|
| 框架 | `React 18` + `TypeScript` + `Vite` | 严格 TS interface 约束 |
| 组件库 | `Joy UI (@mui/joy)` | 统一主题管理，禁止 inline style |
| 状态管理 | `MobX` + `mobx-react-lite` | `enforceActions: 'always'`，禁止 Zustand |
| 动画 | `Framer Motion` | 状态流转、卡片入场、折叠 |
| 包管理 | `yarn` | 统一使用 yarn，禁止 npm/pnpm 混用 |

### 项目目录规范

```
seeclaw/
├── src/                          # Rust 核心代码
│   ├── main.rs
│   ├── agent_engine/             # 状态机、循环控制
│   ├── llm/                      # 多 Provider LLM 抽象层
│   │   ├── provider.rs           # LlmProvider Trait
│   │   ├── registry.rs           # Provider 注册与路由
│   │   ├── providers/            # 各 Provider 实现
│   │   ├── sse_parser.rs         # 共享 SSE 解析
│   │   └── tools.rs              # 从 prompts/ 加载 Tool 定义
│   ├── perception/               # 截屏、视觉感知
│   ├── executor/                 # 键鼠执行、坐标映射
│   ├── mcp/                      # MCP Client 适配层
│   ├── rag/                      # 轻量向量检索 + experience.md
│   ├── skills/                   # 用户可导入 Skill 加载器
│   ├── commands.rs               # Tauri invoke 入口
│   └── errors.rs                 # SeeClawError 统一定义
├── prompts/                      # ★ 所有提示词和 Tool 定义（纯 JSON/文本，与 Rust 解耦）
│   ├── tools/
│   │   ├── builtin.json          # 内置原子 Tool 的 JSON Schema 定义
│   │   └── mcp_template.json     # MCP 动态注入 Tool 的模板
│   ├── system/
│   │   ├── agent_system.md       # Agent System Prompt 模板（含 {elements_xml} 占位符）
│   │   └── experience_summary.md # 经验总结提示词模板
│   └── README.md                 # 提示词维护说明
├── src-ui/                       # React 前端
│   ├── src/
│   │   ├── components/           # UI 组件
│   │   │   ├── chat/             # 对话流组件
│   │   │   ├── settings/         # Settings 面板组件
│   │   │   └── shared/           # 通用组件
│   │   ├── store/                # MobX Store
│   │   ├── theme/                # 统一主题（light/dark token 定义）
│   │   ├── hooks/                # 可复用 React hooks
│   │   ├── utils/                # 工具函数（格式化、类型守卫等）
│   │   ├── types/                # 全局 TypeScript interface/type
│   │   ├── App.tsx
│   │   └── main.tsx
│   └── package.json
├── scripts/                      # PowerShell 脚本（构建、测试、打包）
│   ├── dev.ps1
│   ├── build.ps1
│   ├── test-ui.ps1
│   └── lint.ps1
├── config.toml                   # 应用全局配置（LLM Provider、安全策略等）
├── .env                          # 敏感 token（不提交 Git）
├── experience.md                 # Agent 长期经验文档（自动增量更新）
└── Cargo.toml
```

---

## 二、 核心数据抽象 (The Unified Interface)

为解决"截图画框"与"OS 无障碍树"的格式割裂，Rust 侧定义**统一感知抽象**。

```rust
// src/perception/types.rs

pub struct UIElement {
    pub id: String,
    pub node_type: ElementType,
    pub bbox: [f32; 4],             // 归一化 [xmin, ymin, xmax, ymax] (0.0–1.0)
    pub content: Option<String>,
    pub confidence: f32,
}

pub struct PerceptionContext {
    pub image_base64: Option<String>,
    pub elements: Vec<UIElement>,
    pub resolution: (u32, u32),
    pub meta: ScreenshotMeta,       // DPI/多显示器元信息
}
```

---

## 三、 多 LLM Provider 抽象层 (LLM Abstraction)

所有 LLM 调用经过统一 Trait。新增 Provider 只需实现该 Trait + 在 `config.toml` 中注册。

```rust
// src/llm/provider.rs

#[async_trait::async_trait]
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

**config.toml 完整示例**:
```toml
[llm]
active_provider = "zhipu"          # 当前激活的 Provider（可通过 UI Settings 修改）

# 内置预设 Provider — 用户可在 UI Settings 中修改 api_base / model
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

[llm.providers.claude]
display_name = "Anthropic Claude"
api_base = "https://api.anthropic.com/v1/messages"
model = "claude-opus-4-5"
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

# 用户可在 UI Settings 中 "添加自定义 Provider"，自动追加到此处
# [llm.providers.my_custom]
# display_name = "我的私有部署"
# api_base = "http://localhost:11434/v1/chat/completions"
# model = "llama3"
# temperature = 0.3

[safety]
allow_terminal_commands = false
allow_file_operations = false
require_approval_for = ["execute_terminal", "file_delete", "install_package", "mcp_call"]
max_consecutive_failures = 5
max_loop_duration_minutes = 0      # 0 = 无限循环

[prompts]
tools_file = "prompts/tools/builtin.json"      # 内置 Tool 定义文件路径
system_template = "prompts/system/agent_system.md"
experience_summary_template = "prompts/system/experience_summary.md"
```

---

## 四、 原子 Tool Catalog & Prompts 规范 (Tool Definitions)

### 4.1 设计原则

1. **Tool 定义与代码解耦**：所有 Tool 的 JSON Schema 定义存放在 `prompts/tools/builtin.json`，Rust 代码在启动时加载，不硬编码 JSON 字面量。
2. **完全原子化**：每个 Tool 只做一件事，LLM 通过组合多个 Tool Call 完成复杂操作。
3. **统一 JSON Schema 格式**：所有 Tool 遵循 OpenAI function calling 格式，所有 Provider 兼容。
4. **多语言兼容**：System Prompt 末尾追加用户语言指令，LLM 自动适配输出语言。

### 4.2 完整原子 Tool Catalog

`prompts/tools/builtin.json` 定义以下全部原子工具：

```json
[
  {
    "type": "function",
    "function": {
      "name": "mouse_click",
      "description": "Single left-click on a UI element by its ID from the perception context.",
      "parameters": {
        "type": "object",
        "properties": {
          "element_id": { "type": "string", "description": "Element ID as shown in the annotated screenshot." }
        },
        "required": ["element_id"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "mouse_double_click",
      "description": "Double-click on a UI element by its ID.",
      "parameters": {
        "type": "object",
        "properties": {
          "element_id": { "type": "string" }
        },
        "required": ["element_id"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "mouse_right_click",
      "description": "Right-click on a UI element to open context menu.",
      "parameters": {
        "type": "object",
        "properties": {
          "element_id": { "type": "string" }
        },
        "required": ["element_id"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "scroll",
      "description": "Scroll the viewport. Use 'short' for one step (3 lines), 'long' for one page.",
      "parameters": {
        "type": "object",
        "properties": {
          "direction": { "type": "string", "enum": ["up", "down", "left", "right"] },
          "distance": { "type": "string", "enum": ["short", "long"], "description": "short=3 lines, long=1 page" },
          "element_id": { "type": "string", "description": "Optional: scroll within a specific element." }
        },
        "required": ["direction", "distance"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "type_text",
      "description": "Type text into the currently focused input field. Supports both Chinese and English.",
      "parameters": {
        "type": "object",
        "properties": {
          "text": { "type": "string" },
          "clear_first": { "type": "boolean", "description": "If true, select-all and delete before typing." }
        },
        "required": ["text"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "hotkey",
      "description": "Press a keyboard shortcut combination. Examples: ctrl+c, ctrl+v, alt+f4, ctrl+shift+t, win+d.",
      "parameters": {
        "type": "object",
        "properties": {
          "keys": { "type": "string", "description": "Key combo as plus-separated string, e.g. 'ctrl+shift+esc'." }
        },
        "required": ["keys"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "key_press",
      "description": "Press a single key. Use for navigation keys: Enter, Escape, Tab, ArrowUp, ArrowDown, Delete, Backspace, etc.",
      "parameters": {
        "type": "object",
        "properties": {
          "key": { "type": "string" }
        },
        "required": ["key"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "get_viewport",
      "description": "Capture a fresh screenshot of the current screen state and return an annotated image with element IDs. Use this before any click to verify the current UI state.",
      "parameters": {
        "type": "object",
        "properties": {
          "annotate": { "type": "boolean", "description": "If true, draw bounding boxes and IDs on the screenshot." }
        },
        "required": []
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "execute_terminal",
      "description": "Execute a PowerShell command. REQUIRES human approval before execution. Use only when GUI interaction is insufficient.",
      "parameters": {
        "type": "object",
        "properties": {
          "command": { "type": "string" },
          "reason": { "type": "string", "description": "Explain why terminal execution is necessary." }
        },
        "required": ["command", "reason"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "mcp_call",
      "description": "Call a tool provided by a connected MCP server. List available MCP tools with get_viewport first.",
      "parameters": {
        "type": "object",
        "properties": {
          "server_name": { "type": "string", "description": "MCP server name as defined in config.toml." },
          "tool_name": { "type": "string" },
          "arguments": { "type": "object" }
        },
        "required": ["server_name", "tool_name", "arguments"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "invoke_skill",
      "description": "Execute a pre-defined user skill (e.g. create-excel-pivot, fill-word-template). Skills are composite actions that automate complex repetitive tasks.",
      "parameters": {
        "type": "object",
        "properties": {
          "skill_name": { "type": "string", "description": "Skill name as registered in the skills/ directory." },
          "inputs": { "type": "object", "description": "Named input variables the skill requires." }
        },
        "required": ["skill_name", "inputs"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "wait",
      "description": "Wait for a specified duration before the next action. Use after triggering an operation that needs time to complete (e.g. file save, app launch).",
      "parameters": {
        "type": "object",
        "properties": {
          "milliseconds": { "type": "integer", "minimum": 100, "maximum": 10000 }
        },
        "required": ["milliseconds"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "finish_task",
      "description": "Signal that the assigned task has been completed successfully.",
      "parameters": {
        "type": "object",
        "properties": {
          "summary": { "type": "string", "description": "Brief summary of what was accomplished." }
        },
        "required": ["summary"]
      }
    }
  },
  {
    "type": "function",
    "function": {
      "name": "report_failure",
      "description": "Signal that the task cannot be completed and explain why.",
      "parameters": {
        "type": "object",
        "properties": {
          "reason": { "type": "string" },
          "last_attempted_action": { "type": "string" }
        },
        "required": ["reason"]
      }
    }
  }
]
```

### 4.3 Prompts 文件规范

**`prompts/system/agent_system.md`** — System Prompt 模板，使用 `{placeholder}` 语法：

```markdown
You are SeeClaw, a desktop GUI automation agent running on Windows.

## Available Screen Elements
{elements_xml}

## Relevant Experience
{experience_context}

## Available Skills
{skills_list}

## Available MCP Tools
{mcp_tools_list}

## Rules
- Always call `get_viewport` to verify the current screen state before acting.
- Use the element IDs shown in the annotated screenshot for all click/scroll actions.
- For high-risk actions (execute_terminal, mcp_call), always explain your reasoning first.
- If stuck after 3 attempts on the same sub-task, call `report_failure`.
- Reason step-by-step before every tool call.

## User Language
{user_language_hint}
```

`{user_language_hint}` 在运行时动态注入，例如：
- 中文用户：`The user speaks Chinese. Respond in Chinese.`
- 英文用户：`The user speaks English. Respond in English.`

**Rust 侧注入逻辑**：检测用户输入文本是否包含 CJK 字符，自动判断语言并填充 `{user_language_hint}`。

---

## 五、 核心工作流引擎 (The Engine Pipeline)

Rust 端 `agent_engine` 采用严格的异步单向数据流:
**Router → Perception → Planner → Executor → Observer → (Loop)**

### 阶段 1: 触发与路由 (Trigger & Router)

1. 用户输入指令，MobX `is_running: true`，调用 `invoke("start_task", { task, loop_config })`
2. Router 判断任务类型（GUI / Terminal / File / MCP Tool Call）
3. 输出 `RoutePlan`，激活对应管道

### 阶段 2: 统一感知层 (Perception)

1. 截屏获取当前画面 + DPI 元信息
2. 双轨解析（策略模式 `VisionProvider` Trait）:
   - 策略 A: OS 无障碍树 → `Vec<UIElement>`
   - 策略 B: ONNX YOLO 目标检测（本地）
   - 策略 C: SoM 网格兜底
3. 组装标准 `PerceptionContext`

### 阶段 3: 规划 (Planner & LLM Tool Call)

1. Prompt 组装: `PerceptionContext` + RAG 检索的相关经验 → System Prompt
2. 通过统一 `LlmProvider` Trait 流式调用当前激活的 LLM
3. SSE 流式块 → Tauri `emit` → MobX → 实时打字机渲染

### 阶段 4: 执行 (Executor)

1. 解析 Tool Call：`click_element` / `type_text` / `execute_terminal` / `scroll` / `hotkey` 等
2. 反向坐标映射（归一化 → 物理像素，处理 DPI + 多显示器偏移）
3. 安全拦截（Human-in-the-Loop）：高危操作挂起等待用户审批
4. `enigo` 物理模拟（支持中英文系统级输入）

### 阶段 5: 验证与循环 (Observer & Loop Control)

1. 执行后等待 UI 响应（500ms–1s）
2. 重新感知，LLM 评估动作是否生效
3. **循环控制策略**:
   - `24h 持续循环`：直到 LLM 判断完成
   - `用户自定义时长`：config.toml `max_loop_duration_minutes`
   - `失败次数上限`：连续失败 N 次自动停止并通知用户
   - `finish_task` Tool Call：LLM 主动结束
4. 循环结束/异常 → 调用 RAG 模块将本次经验写入 `experience.md`

---

## 五、 MCP Client 集成

SeeClaw 作为 MCP Client，可连接外部 MCP Server 获取额外工具能力。

```rust
// src/mcp/client.rs

pub struct McpClient {
    server_url: String,
    transport: McpTransport,  // stdio / http+sse
}

impl McpClient {
    pub async fn list_tools(&self) -> Result<Vec<McpTool>, SeeClawError>;
    pub async fn call_tool(&self, name: &str, args: serde_json::Value) -> Result<serde_json::Value, SeeClawError>;
}
```

**config.toml**:
```toml
[[mcp.servers]]
name = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "./workspace"]

[[mcp.servers]]
name = "custom-excel"
command = "python"
args = ["./mcp-servers/excel-server.py"]
```

MCP 工具在 LLM Tool Call 列表中动态注册，Planner 可像原生工具一样调用。

---

## 六、 用户可导入 Skills 系统

用户可将自定义自动化技能（操作 Excel、Word 等）以标准格式导入。

```toml
# skills/excel-pivot/skill.toml
[skill]
name = "excel-pivot"
description = "在 Excel 中创建数据透视表"
version = "1.0.0"

[[skill.steps]]
action = "click_element"
target = "数据透视表按钮"

[[skill.steps]]
action = "type_text"
text = "{field_name}"
```

Rust 侧 `src/skills/loader.rs` 在启动时扫描 `skills/` 目录，将已导入 Skill 注册为 LLM 可调用的高级 Tool。

---

## 七、 轻量级 RAG + 长期经验 (Experience Loop)

### 向量索引

使用纯 Rust 向量库（`usearch` / `hnsw_rs`），无需外部服务。索引文件存放在 AppData 目录。

### experience.md

根目录 `experience.md` 是 Agent 的**长期记忆**。每次任务结束后：

1. Observer 评估本次执行的成功/失败因素
2. 通过 LLM 总结一条经验规则
3. 增量追加到 `experience.md`
4. 同步更新向量索引

下次任务开始时，Planner 检索与当前目标最相关的历史经验注入 Prompt。

```markdown
<!-- experience.md 示例 -->
## 2026-02-27 — 清空回收站
- **结果**: 成功
- **经验**: 右键回收站图标后弹出菜单需要等待 800ms 才稳定，500ms 会导致误点

## 2026-02-27 — 打开 Excel 创建透视表
- **结果**: 失败 2 次后成功
- **失败原因**: "数据"选项卡在 WPS 中的位置与 Office 不同，需要先识别当前应用
- **经验**: 操作 Office 类应用前，先截屏识别是 WPS 还是 Microsoft Office
```

---

## 八、 UI 设计规范 (State & Theme Standard)

### 设计原则

- **参考 Claude / ChatGPT 的简洁大气风格**
- 配色: 灰色、黑色、白色、米色为主色调
- **禁止渐变色**，**禁止滥用毛玻璃**
- 支持 **明亮/黑暗双主题**
- 所有色值通过 theme token 引用，禁止 magic number

### Settings 面板 — Provider 配置 UI

Settings 面板包含 **LLM Provider** 子页，支持：

1. **选择当前 Provider**：下拉菜单，列出 `config.toml` 中所有 `[llm.providers.*]`，显示 `display_name`
2. **编辑已有 Provider**：点击任意 Provider → 展开表单，可修改：
   - `api_base`（Base URL）
   - `model`
   - `temperature`
   - API Key（写入 `.env`，界面显示 `***` 脱敏）
3. **添加自定义 Provider**：点击 "+ 添加"，填写以上字段，提交后追加到 `config.toml`
4. **删除 Provider**：删除按钮，不能删除当前激活的 Provider

```typescript
// src-ui/src/types/settings.ts
export interface ProviderConfig {
  id: string;           // config.toml 中的 key，如 "zhipu"
  displayName: string;
  apiBase: string;
  model: string;
  temperature: number;
  hasApiKey: boolean;   // 只告诉前端是否已配置，不暴露 key 值
}

export interface AppSettings {
  activeProvider: string;
  providers: ProviderConfig[];
  safety: SafetyConfig;
  loopDefaults: LoopConfig;
  theme: 'light' | 'dark' | 'system';
  userLanguage: 'auto' | 'zh' | 'en';
}
```

Rust 侧 Tauri command：
```rust
// src/commands.rs
#[tauri::command]
async fn get_settings() -> Result<AppSettings, String>;

#[tauri::command]
async fn save_provider_config(provider: ProviderConfig, api_key: Option<String>) -> Result<(), String>;

#[tauri::command]
async fn delete_provider(provider_id: String) -> Result<(), String>;

#[tauri::command]
async fn set_active_provider(provider_id: String) -> Result<(), String>;
```

### 状态同步 (MobX Store)

```typescript
// src-ui/src/types/agent.ts
export interface AgentStatus {
  state: 'idle' | 'routing' | 'observing' | 'planning' | 'executing'
       | 'waiting_for_user' | 'evaluating' | 'error';
  reasoningStream: string;
  contentStream: string;
  actionHistory: ActionCard[];
  loopConfig: LoopConfig;
  failureCount: number;
}

export interface LoopConfig {
  mode: 'until_done' | 'timed' | 'failure_limit';
  maxDurationMinutes?: number;
  maxFailures?: number;
}

export interface SafetyConfig {
  allowTerminalCommands: boolean;
  allowFileOperations: boolean;
  requireApprovalFor: string[];
}
```

### 动画规范 (Framer Motion)

- 状态胶囊: `<motion.div layout>` 平滑切换宽度/颜色
- 思考折叠: spring 动画 `stiffness: 300, damping: 30`
- 工具卡片入场: `initial={{ opacity: 0, y: 10, scale: 0.95 }}`
- **禁止过度动画**，保持 Claude 级别的克制感

---

## 九、 架构优势总结

1. **高度解耦**: 视觉模型、LLM Provider、MCP Server 均通过 Trait/Interface 插拔，核心管道零修改
2. **极致性能**: Rust 承载所有繁重 IO；React + MobX 细粒度精准渲染；Tauri 轻量桥接
3. **可进化**: RAG + experience.md 让 Agent 每次执行都在积累经验
4. **可扩展**: 用户可导入 Skills、连接 MCP Server、在 config.toml 一行切换 LLM Provider
5. **安全可控**: Human-in-the-Loop + UI 可配置安全规则 + 失败自动停止
