# 架构改进总结

## 概述

本文档总结了对 SeeClaw Agent 引擎架构的三项关键改进，旨在解决状态机冗余、延迟陷阱和执行层盲目性问题。

## 改进 1: 合并 Plan 与 Evaluate（隐式评估）

### 问题
- 原架构有独立的 `Evaluating` 状态，每次 Action 后都会调用一次 VLM 仅为了判断"目标实现了吗？"
- 执行简单的"点击-输入-点击"操作需要调用 6 次大模型（3 次规划 + 3 次评估）
- `Routing` 状态作为主循环中的阻塞状态不合理，只是一个微操作

### 解决方案
- **取消独立的 `Evaluating` 阶段**：将评估逻辑合并到 `Planning` 状态中
- **移除 `Routing` 状态**：不再作为主循环状态
- **隐式评估机制**：在 `Planning` 阶段，将最新的截图和历史 Action 记录交给 VLM，让 VLM 一次性完成判断：
  - "上一步的动作是否成功？"
  - "如果成功且目标已达成，输出 `finish_task()`"
  - "如果没有，输出下一个工具调用"

### 实现细节

#### 状态机变更
```rust
// 旧状态机
enum AgentState {
    Idle,
    Routing { goal: String },
    Planning { goal: String },
    Executing { action: AgentAction },
    WaitingForUser { pending_action: AgentAction },
    Evaluating { goal: String, steps_summary: String },  // 已移除
    Error { message: String },
    Done { summary: String },
}

// 新状态机
enum AgentState {
    Idle,
    Planning { goal: String },  // 合并了 Planning 和 Evaluating
    Executing { action: AgentAction },
    WaitingForStability { action: AgentAction },  // 新增
    WaitingForUser { pending_action: AgentAction },
    Error { message: String },
    Done { summary: String },
}
```

#### 核心逻辑变更
1. **`advance_to_next_step` 方法**：
   - 当所有步骤完成时，不再进入 `Evaluating` 状态
   - 而是构建评估提示词并注入到对话历史中
   - 重新进入 `Planning` 状态，让 VLM 自我评估并决定下一步

2. **`call_planner_with_context` 方法**：
   - 替代了原来的 `call_planner` 和 `call_evaluator`
   - 处理 `plan_task`、`finish_task` 和 `report_failure` 工具调用
   - 移除了 `evaluate_completion` 工具调用处理

3. **移除的方法**：
   - `call_evaluator`
   - `handle_evaluate_completion_tool`

### 性能提升
- **API 调用量减少 50%**：从 2N 次减少到 N 次（N = 步骤数）
- **延迟降低**：减少了额外的 VLM 调用延迟
- **成本降低**：减少了 LLM API 调用成本

## 改进 2: 引入视觉稳定检测（Visual Stability Check）

### 问题
- 执行层偏向于同步执行（执行动作 -> 马上切状态 -> 截图）
- 忽略了现代 UI/UX 设计中的关键因素：前端动画渲染和网络加载需要时间
- 如果在页面还在做过渡动画时截图，YOLO 和 VLM 都会识别出幻觉
- 硬编码的 `wait` 工具既低效又不可靠

### 解决方案
- **视觉稳定检测机制**：在执行完点击或输入后，感知层有一个微小的局部循环
- **像素差异比较**：比较连续几帧的截图像素差异
- **阈值判定**：只有当屏幕像素差异低于某个阈值（即页面"静止"了），才触发下一次的 Observing 截图
- **类似 `waitForNetworkIdle`**：类似于自动化测试中的网络空闲等待机制

### 实现细节

#### 新增文件：`src/perception/stability.rs`
```rust
pub struct StabilityConfig {
    pub max_wait_ms: u64,           // 最大等待时间
    pub check_interval_ms: u64,       // 检查间隔
    pub stability_threshold: f64,      // 稳定性阈值
    pub min_stable_frames: usize,      // 最小稳定帧数
}

pub struct VisualStabilityDetector {
    config: StabilityConfig,
    last_frame_hash: Option<u64>,
    stable_frame_count: usize,
}

pub async fn wait_for_visual_stability<F, Fut>(
    capture_frame: F,
    config: StabilityConfig,
    stop_flag: Arc<AtomicBool>,
) -> SeeClawResult<bool>

pub async fn wait_for_animation_completion<F, Fut>(
    capture_frame: F,
    config: StabilityConfig,
    stop_flag: Arc<AtomicBool>,
) -> SeeClawResult<bool>
```

#### 状态机集成
新增 `WaitingForStability` 状态：
```rust
AgentState::WaitingForStability { action } => {
    let stability_config = StabilityConfig {
        max_wait_ms: 3000,
        check_interval_ms: 200,
        stability_threshold: 0.02,
        min_stable_frames: 2,
    };

    match wait_for_visual_stability(
        capture_fn,
        stability_config,
        stop_flag,
    ).await {
        Ok(true) => {
            // 视觉稳定，继续下一步
            Box::pin(self.advance_to_next_step()).await;
        }
        Ok(false) => {
            // 超时或停止，继续下一步
            Box::pin(self.advance_to_next_step()).await;
        }
        Err(e) => {
            // 错误，继续下一步
            Box::pin(self.advance_to_next_step()).await;
        }
    }
}
```

#### 智能触发
在 `execute_action` 方法中，根据动作类型决定是否需要等待视觉稳定：
```rust
let needs_stability = matches!(
    action,
    AgentAction::MouseClick { .. }
        | AgentAction::MouseDoubleClick { .. }
        | AgentAction::MouseRightClick { .. }
        | AgentAction::TypeText { .. }
        | AgentAction::Hotkey { .. }
        | AgentAction::KeyPress { .. }
        | AgentAction::Scroll { .. }
);

if needs_stability && ok {
    self.state = AgentState::WaitingForStability { action };
} else {
    Box::pin(self.advance_to_next_step()).await;
}
```

### 性能提升
- **减少幻觉**：避免在动画进行时截图，提高识别准确率
- **自适应等待**：根据实际页面变化动态调整等待时间
- **可靠性提升**：不再依赖硬编码的等待时间

## 改进 3: 基于消息总线的异步架构（框架准备）

### 问题
- 严格的有限状态机（FSM）易于理解，但难以处理复杂的中断和外部输入
- 当前架构紧耦合了主循环：Engine -> Router -> Planner -> Executor 是串行的
- 难以处理用户中途打断、MCP Server 异步返回数据等情况

### 解决方案
- **消息总线架构**：借鉴类似 ROS 的发布/订阅（Pub/Sub）机制
- **独立 Worker 节点**：将核心拆分为独立的 Worker
- **事件驱动**：通过消息传递实现节点间通信

### 实现细节

#### 新增文件：`src/agent_engine/event_bus.rs`
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentMessage {
    PerceptionReady {
        screenshot: Vec<u8>,
        timestamp: DateTime<Utc>,
    },
    ActionCompleted {
        action_id: String,
        success: bool,
        error: Option<String>,
    },
    VisualStable {
        timestamp: DateTime<Utc>,
    },
    PlanRequired {
        goal: String,
        context: PlanContext,
    },
    PlanGenerated {
        steps: Vec<TodoStep>,
        should_finish: bool,
    },
    StopRequested,
}

pub struct EventBus {
    tx: broadcast::Sender<AgentMessage>,
    rx: broadcast::Receiver<AgentMessage>,
    command_tx: mpsc::Sender<AgentMessage>,
    command_rx: mpsc::Receiver<AgentMessage>,
}
```

#### 架构组件
1. **Perception Node**：
   - 持续以低帧率抓取屏幕
   - 更新状态缓冲区
   - 发布 `PerceptionReady` 消息

2. **Planner Node**：
   - 接收当前环境快照
   - 生成行动序列
   - 发布 `PlanGenerated` 消息

3. **Action Node**：
   - 维护任务队列
   - Planner 可以一次性下发多个操作
   - 依次执行，遇到环境突变时清空队列并呼叫 Planner 重新规划

### 当前状态
- **框架已准备**：`EventBus` 和消息类型已实现
- **渐进式迁移**：当前仍使用改进后的 FSM，但为完全异步架构做好了准备
- **向后兼容**：现有代码可以继续使用，未来可以逐步迁移到完全异步架构

## 总结

### 改进效果
1. **性能提升**：
   - API 调用量减少 50%
   - 延迟降低
   - 成本降低

2. **可靠性提升**：
   - 减少视觉识别幻觉
   - 自适应等待时间
   - 更好的错误处理

3. **可扩展性提升**：
   - 为完全异步架构做好准备
   - 支持更复杂的交互场景
   - 更好的模块化

### 文件变更
- **修改**：
  - `src/agent_engine/state.rs`：状态机重构
  - `src/agent_engine/engine.rs`：核心逻辑重构
  - `src/agent_engine/mod.rs`：模块导出更新
  - `src/perception/mod.rs`：模块导出更新

- **新增**：
  - `src/agent_engine/event_bus.rs`：消息总线
  - `src/perception/stability.rs`：视觉稳定检测

### 后续工作
1. 完全迁移到异步架构（使用 EventBus）
2. 优化视觉稳定检测算法
3. 添加更多性能监控指标
4. 完善错误处理和恢复机制
