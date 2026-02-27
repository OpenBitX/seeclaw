# SeeClaw 系统架构与流转规范

## 核心原则 (Core Tenets)
1. **单一事实来源 (Single Source of Truth)**: 所有的核心业务状态（当前执行步骤、历史记录、大模型上下文）严格存储在 Rust 的内存/本地数据库中。UI 只是状态的映射。
2. **状态驱动 (State-Driven)**: Rust 端的 `agent_engine` 是一个异步状态机（FSM）。
3. **接口扩展性 (Extensibility)**: 视觉解析必须实现 `VisionProvider` Trait，以保证未来可以无缝接入方案 A。

## 核心数据流 (Data Flow)
1. **触发 (UI -> Rust)**: 用户在 UI 点击“执行”，UI 发送 `invoke("start_agent", { prompt: "..." })`。
2. **路由 (Rust Internal)**: `agent_engine` 接收指令，`router` 决定是否需要视觉交互。
3. **感知 (Rust Internal)**: 
   - 调用截图 API。
   - 优先经过 `onnx_yolo` 进行本地目标检测画框。
   - 若本地模型置信度低，退化使用 `som_grid` 画网格。
4. **决策 (Rust -> LLM -> Rust)**: 将带有标记的图片通过 `llm_client` 发送给 GLM-4.6V，要求返回标记 ID 或网格坐标。
5. **执行 (Rust Internal)**: `executors` 解析坐标，处理跨屏幕 DPI 缩放，调用底层 API 点击。
6. **反馈 (Rust -> UI)**: 状态机每进入一个新状态，通过 `app_handle.emit_all("agent_state_update", payload)` 通知 UI，MUI 组件自动重绘更新进度条或日志。

