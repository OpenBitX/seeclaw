### SeeClaw 分步开发计划 (Development Plans)

#### Phase 1: 基础设施与 UI 骨架 (Foundation & UI Skeleton)

**目标**：Tauri 框架跑通，建立中性双主题 UI 骨架，前后端 IPC 通讯验证。

* **Step 1**: 初始化 Tauri 2 项目（React + TypeScript + Vite），yarn 初始化前端依赖。
* **Step 2**: 搭建目录结构：`src/`（Rust）、`src-ui/src/{components,store,theme,hooks,utils,types}`、`scripts/`。
* **Step 3**: 引入 `@mui/joy`，在 `src-ui/src/theme/` 建立统一主题（light + dark），配色参考 OpenAI 风格（灰/黑/白/米色），禁止渐变色。
* **Step 4**: 编写主界面骨架：单列对话区 + 底部输入框（参考 Claude 布局）。
* **Step 5**: Rust 侧 dummy `invoke` + `emit` 验证 IPC。
* **Step 6**: 编写 `scripts/dev.ps1` 和 `scripts/build.ps1` PowerShell 脚本。

#### Phase 2: config.toml 与多 LLM Provider 抽象 (Configuration & LLM Layer)

**目标**：建立 `config.toml` 读取机制和可插拔 LLM 调用层。

* **Step 1**: 定义 `config.toml` 完整结构（`[llm]`、`[safety]`、`[[mcp.servers]]`），Rust 侧 serde 反序列化为强类型 struct。
* **Step 2**: 定义 `LlmProvider` Trait（`stream_chat` 异步接口）。
* **Step 3**: 实现 `ZhipuProvider`（GLM-4V），SSE 流式解析 + Tauri emit 转发。
* **Step 4**: 实现 `OpenAiProvider` 骨架（结构一致，仅 endpoint/auth 不同）。
* **Step 5**: `config.toml` 的 `active_provider` 字段驱动 Provider 选择。API Key 从 `.env` 读取。

#### Phase 3: Rust 核心状态机 (State Machine & Loop Control)

**目标**：在 Rust 内存中建立 Agent 引擎，含循环控制策略。

* **Step 1**: `src/agent_engine/state.rs` — 定义 `AgentState` 枚举、`AgentEvent`、`AgentAction`。
* **Step 2**: `src/agent_engine/engine.rs` — 异步状态机主循环（Idle → Observing → Planning → Executing → Evaluating → Loop）。
* **Step 3**: 循环控制引擎：支持无限循环 / 定时停止 / 连续失败 N 次自动停止。阈值从 `config.toml [safety]` 读取。
* **Step 4**: 失败/超时自动暂停时，emit `"loop_stopped"` 通知前端。
* **Step 5**: 历史记录 JSONL 持久化（AppData 目录，按 session UUID）。

#### Phase 4: 视觉感知管道 (Vision Pipeline)

**目标**：Rust 截屏 + 本地检测，确保画框准确。

* **Step 1**: `xcap` 截屏 + DPI 元信息采集（`ScreenshotMeta`）。
* **Step 2**: `ort` 加载 YOLO-Nano UI 模型，单例 Tauri Managed State。
* **Step 3**: 预处理（CHW float32 tensor）→ 推理 → 后处理（NMS → `Vec<UIElement>`）。
* **Step 4**: SoM Grid 兜底（当 YOLO 检测为空时绘制标注网格）。
* **Step 5**: 组装统一 `PerceptionContext`。

#### Phase 5: 执行与人机协同 (Executor & Human-in-the-Loop)

**目标**：解析 LLM Tool Call，执行物理操作，处理安全拦截。

* **Step 1**: `enigo` 封装 — 鼠标移动/点击、键盘输入（中英文均支持，中文走剪贴板+Ctrl+V）。
* **Step 2**: 坐标反向映射（归一化 → 逻辑 → 物理像素 → 多显示器偏移）。
* **Step 3**: Safety 拦截：`config.toml [safety].require_approval_for` 中列出的操作 → 挂起 → 前端审批卡片。
* **Step 4**: 前端 Settings 页面 — 可视化配置安全策略（允许/禁止操作类型、循环策略）。

#### Phase 6: 流式 UI 与对话体验 (Streaming UI & Chat UX)

**目标**：前端丝滑展示思考过程和工具执行。

* **Step 1**: MobX Store 监听 `llm_stream_chunk` 事件，实时更新 `reasoningStream` / `contentStream`。
* **Step 2**: 可折叠思考区块（Framer Motion spring 动画）。
* **Step 3**: 工具调用卡片（ActionCard）— 不同操作类型不同颜色标签。
* **Step 4**: 状态胶囊 — 顶部显示当前 Agent 状态，layout 动画平滑切换。
* **Step 5**: 循环状态 UI — 显示当前循环次数、已用时间、失败计数。

#### Phase 7: MCP Client 集成 (MCP Integration)

**目标**：连接外部 MCP Server，动态扩展 Agent 工具集。

* **Step 1**: `src/mcp/transport.rs` — 实现 `McpTransport` Trait（stdio / HTTP+SSE）。
* **Step 2**: `src/mcp/client.rs` — `list_tools` + `call_tool` 接口。
* **Step 3**: Agent 启动时读取 `config.toml [[mcp.servers]]`，连接所有 MCP Server，合并工具列表。
* **Step 4**: MCP 工具注入 LLM Tool Call 定义，Planner 可像原生工具一样调用。

#### Phase 8: 用户 Skills 系统 (User-Importable Skills)

**目标**：用户可导入自定义自动化技能包。

* **Step 1**: 定义 Skill 标准格式（`skill.toml` — name, description, steps）。
* **Step 2**: `src/skills/loader.rs` — 扫描 `skills/` 目录，解析并注册为高级 Tool。
* **Step 3**: 前端 UI — Skills 管理页面（导入、查看、删除已安装 Skills）。
* **Step 4**: Agent Planner 在 Prompt 中将可用 Skills 列为可调用工具。

#### Phase 9: RAG 与长期经验 (RAG & Experience Loop)

**目标**：Agent 从历史经验中学习，持续进化。

* **Step 1**: `src/rag/embedder.rs` — 文本向量化（本地小模型或 LLM API embedding endpoint）。
* **Step 2**: `src/rag/index.rs` — 纯 Rust 向量索引（usearch / hnsw_rs），CRUD 操作。
* **Step 3**: Observer 任务结束钩子 — LLM 总结经验 → 追加 `experience.md` → 更新索引。
* **Step 4**: Planner Prompt 注入 — 检索 Top-K 相关经验放入 `<experience>` 段。

#### Phase 10: 打磨与测试 (Polish & Testing)

* **Step 1**: Rust 单元测试（状态机跃迁、坐标映射、config 解析）。
* **Step 2**: 前端组件测试（`scripts/test-ui.ps1` 调用 `yarn test`）。
* **Step 3**: 集成测试 — 端到端自动化场景（打开记事本 → 输入文字 → 保存）。
* **Step 4**: 性能调优 — 截屏/推理延迟、UI 渲染帧率。
* **Step 5**: `scripts/build.ps1` — Tauri 打包为 Windows 安装包。
