# 持久化、流式通讯与 RAG 规范

**语言强制要求**：所有持久化数据结构字段名、序列化 key、Tauri event 名称、代码注释必须使用英文。

## 1. 会话持久化

Agent 的当前 `State` 不落盘。`ConversationHistory` 和 `ExecutionFlow` 必须持久化：

- **默认 JSONL**：存放在 `tauri::api::path::app_data_dir()` 目录，文件名 `session_<uuid>.jsonl`
- **迁移 rusqlite 触发条件**：单个 JSONL > 5 MB；历史会话 > 200 个；需要跨会话搜索
- 每行结构：`{"ts": 1700000000, "role": "assistant", "content": "...", "action": null}`

## 2. SSE 流式转发

- Rust 请求 LLM 时必须 `stream: true`
- 收到数据块后**立即** `app_handle.emit("llm_stream_chunk", chunk)` 推送前端，禁止缓冲
- Event 名称统一 `snake_case` 英文

## 3. Human-in-the-Loop 阻塞

当 Router/Planner 决定执行高危操作时：

1. 状态机进入 `WaitingForUser`
2. `app_handle.emit("action_required", payload)` 通知前端
3. 等待 `invoke("confirm_action", { approved: true/false })` 后再继续
4. 严禁绕过此流程直接执行

可被拦截的操作类型由 `config.toml` `[safety].require_approval_for` 配置，用户可在 UI Settings 中修改。

## 4. 轻量级 RAG 持久化

| 存储 | 位置 | 说明 |
|---|---|---|
| 向量索引 | `AppData/seeclaw/rag_index/` | 纯 Rust 向量库索引文件 |
| 原始经验 | `experience.md`（项目根目录） | Markdown 格式长期经验，可读可编辑 |
| 嵌入模型 | 本地小模型或 LLM API | 生成文本向量用 |

### RAG 写入流程

1. 任务结束 → Observer 收集执行记录
2. 调用 LLM 总结：成功/失败原因 + 可复用经验
3. 追加写入 `experience.md`（带日期和任务标题）
4. 对新经验文本生成 embedding，增量更新向量索引

### RAG 读取流程

1. 新任务开始 → Planner 组装 Prompt 前
2. 将用户目标文本生成 embedding
3. 在向量索引中检索 Top-K 最相关经验
4. 注入 System Prompt 的 `<experience>` 段

## 5. config.toml 持久化

- 用户通过 UI Settings 修改的安全策略、循环策略等配置，通过 Tauri invoke 写回 `config.toml`
- Rust 侧使用 `toml::to_string_pretty` 序列化，保持文件可读性
