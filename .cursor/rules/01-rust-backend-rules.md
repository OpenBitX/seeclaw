# Rust 后端核心开发规范

**语言强制要求**：所有 Rust 代码（变量名、函数名、结构体名、枚举名、模块名）及代码注释（`//`、`///` doc comment）必须全部使用英文。

## 1. 异步优先

所有 IO 操作、网络请求、LLM 推理必须使用 `tokio` 异步运行时。绝不能阻塞主线程或 Tauri 事件循环。

## 2. 企业级错误处理

严禁在业务代码中使用 `.unwrap()` 或 `.expect()`（仅允许在 `main.rs` 初始化失败场景）。使用 `thiserror` 定义全局 `SeeClawError` 枚举，通过 `Result<T, SeeClawError>` 向上传递。

## 3. 面向 Trait 编程

以下模块**必须先定义 Trait 再写实现**，保证可插拔与 Mock 能力：

| 模块 | Trait | 说明 |
|---|---|---|
| LLM 调用 | `LlmProvider` | 统一 stream_chat 接口，所有 Provider（GLM/OpenAI/Qwen）实现此 Trait |
| 视觉感知 | `VisionParser` | ONNX / Accessibility / SoM 三种策略均实现此 Trait |
| MCP 适配 | `McpTransport` | stdio / HTTP+SSE 两种传输层实现此 Trait |

## 4. Prompts 与 Tool 定义管理

- **Tool 定义与代码解耦**：所有 Tool 的 JSON Schema 存放在 `prompts/tools/builtin.json`。Rust 启动时通过 `include_str!("../../prompts/tools/builtin.json")` 加载，**禁止在 Rust 文件中硬编码 Tool JSON 字面量**。
- **System Prompt 模板**：存放在 `prompts/system/agent_system.md`，使用 `{placeholder}` 占位符，Rust 侧 `str.replace("{elements_xml}", &elements)` 注入，不使用 templating crate（保持简单）。
- **MCP Tool 动态追加**：MCP Server 启动后，`list_tools()` 返回的工具列表追加到内置 Tool 列表后，一起传给 LLM，不修改 JSON 文件。
- **Skill Tool 动态注册**：`skills/` 目录中每个已安装 Skill 在 Agent 启动时自动注册为一个 `invoke_skill` 参数选项，更新到 LLM 的 Tool description。

## 5. 多 LLM Provider 规范

- 所有 Provider 配置从 `config.toml` 的 `[llm.providers.*]` 段读取
- `active_provider` 字段决定当前使用哪个 Provider（可在 UI Settings 实时切换）
- 新增 Provider 只需：① `config.toml` 中添加 `[llm.providers.xxx]` 配置段 ② 设置对应 `.env` 变量
- 大多数 Provider（GLM / OpenAI / DeepSeek / Qwen / OpenRouter）使用同一个 `OpenAiCompatibleProvider` 实现，无需新增代码
- Claude 等非 OpenAI 格式 Provider 单独实现，实现同一 `LlmProvider` Trait
- API Key 从 `.env` 环境变量读取，变量名规范：`SEECLAW_{PROVIDER_ID_UPPERCASE}_API_KEY`
- config.toml 中仅存放非敏感配置（api_base、model、temperature）

## 6. Tool 原子化与执行规范

- **全部动作必须有对应原子 Tool**：不允许 LLM 直接输出"然后点击第3个按钮"此类自然语言描述，所有动作必须表达为 Tool Call
- **已定义的原子 Tool 清单**（详见 `prompts/tools/builtin.json`）：
  `mouse_click` / `mouse_double_click` / `mouse_right_click` / `scroll` / `type_text` / `hotkey` / `key_press` / `get_viewport` / `execute_terminal` / `mcp_call` / `invoke_skill` / `wait` / `finish_task` / `report_failure`
- **中文输入实现**：`type_text` 工具执行时，Executor 检测文本是否含 CJK 字符。若是，先写入剪贴板（`arboard` crate），再模拟 `Ctrl+V` 粘贴，而非逐字符 `enigo` 输入
- **语言自动检测**：Planner 在组装 System Prompt 时，检测用户原始输入是否含 CJK 字符（`text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))`），填充 `{user_language_hint}` 占位符

## 7. 循环控制与系统级输入

- 状态机 Run Loop 内置循环策略引擎：支持无限循环（24h+）、定时停止、失败计数停止
- 连续失败达到 `config.toml` 中 `max_consecutive_failures` 阈值时，自动暂停并通过 Tauri emit 通知前端
- 中文输入通过剪贴板（`arboard` crate）+ `Ctrl+V` 粘贴方案，英文通过 `enigo` 逐字符输入
- 所有可配置阈值从 `config.toml` `[safety]` 段读取，禁止硬编码

## 8. MCP Client

- `src/mcp/` 实现标准 MCP Client 协议
- 支持 stdio 和 HTTP+SSE 两种传输
- MCP Server 列表从 `config.toml` `[[mcp.servers]]` 段读取
- MCP 工具在 Agent 启动时动态注册，追加到 `prompts/tools/builtin.json` 加载的内置工具列表之后

## 9. RAG + 经验回路

- `src/rag/` 使用纯 Rust 向量库实现轻量级 RAG
- 向量索引文件存放在 Tauri AppData 目录
- 每次任务结束后，Observer 通过 LLM 总结经验并增量写入根目录 `experience.md`
- Planner 在组装 Prompt 时检索 Top-K 相关经验，注入 `{experience_context}` 占位符

## 10. 日志规范

集成 `tracing` 库。Agent 状态跃迁、LLM 请求/响应、Tool Call 执行、MCP 调用、Safety 拦截等关键节点均需输出结构化日志（`info!` / `debug!` / `warn!` / `error!`）。日志内容使用英文。
