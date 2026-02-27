# Rust 后端核心开发规范

1. **异步优先**: 所有核心 IO 操作、网络请求、大模型推理必须使用 `tokio` 异步运行时，绝不能阻塞主线程。
2. **企业级错误处理**: 严禁在业务代码中使用 `.unwrap()` 或 `.expect()`。必须使用 `thiserror` 或 `anyhow` 定义全局 `SeeClawError` 枚举，并通过 `Result<T, SeeClawError>` 向上传递，最终在 `commands.rs` 转换为供前端消费的错误字符串。
3. **面向 Trait 编程**: 对于 `Vision` 和 `LLMClient` 模块，必须先定义 Trait (如 `trait VisionParser { fn parse(&self, img: Image) -> Result<BBox> }`)，然后再写具体实现，保证极高可插拔性。
4. **日志规范**: 必须集成 `tracing` 库，在 Agent 的每一次状态流转、大模型请求前后输出结构化日志 (Info/Debug/Error)。

