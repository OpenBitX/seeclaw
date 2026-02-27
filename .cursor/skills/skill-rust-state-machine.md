# 技能：如何在 Rust 中构建原生 Agent 状态机

由于我们不使用 LangGraph，在 Rust 中构建企业级编排器需采用**基于枚举的状态机模式 (Enum-based State Machine)**。

核心模式：
1. 定义 `AgentState` 枚举 (如 `Idle`, `Observing`, `Planning`, `Executing`, `Evaluating`)。
2. 定义 `AgentEvent` 枚举 (如 `GoalReceived`, `VisionParsed`, `ActionGenerated`)。
3. 创建 `AgentEngine` 结构体，内部持有状态和共享上下文 (Context)。
4. 实现一个异步的 `run_loop` 方法，使用 `match state` 处理不同阶段的逻辑，并通过返回新的 `AgentState` 进行状态跃迁。
必须保证状态跃迁过程中的数据所有权安全，推荐使用 `Arc<Mutex<Context>>` 共享不可变/可变数据。

