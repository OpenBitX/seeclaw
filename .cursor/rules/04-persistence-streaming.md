# 持久化与流式通讯规范

1. **执行流持久化**: Agent 的当前 `State` 不落盘，但所有的 `Conversation History` (对话历史) 和 `Execution Flow` (工具调用记录、执行结果) 必须追加写入到本地。首选轻量级的 `JSONL` (JSON Lines) 格式存放在操作系统的 AppData 目录下，或者使用 `rusqlite`。
2. **SSE 流式转发**: Rust 侧在请求 GLM-4.6V 时，必须启用 `stream: true`。Rust 获取到数据块后，不要积压，立刻通过 Tauri 的 `app_handle.emit("llm_stream_chunk", chunk)` 推送给前端。
3. **关键节点阻塞 (Human-in-the-loop)**: 当 Rust 侧的 Router 或 Planner 决定执行 `终端命令 (TERMINAL_CMD)` 等高危操作时，状态机必须挂起进入 `WaitingForUser` 状态，并通过 Tauri 发送一个需要确认的 Event 给前端，等待前端 `invoke("confirm_action")` 后再继续执行。