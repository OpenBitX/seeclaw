# SeeClaw Agent Architecture

## 核心拓扑

```
User Query
    │
    ▼
┌──────────────────────────────────────┐
│              ROUTER                   │
│  L1: Regex 关键词匹配                 │
│  L2: Bayesian 概率分类                │
│  L3: LLM 兜底 (轻量模型)             │
│                                      │
│  输出: route_type + context          │
│  当前: simple | complex              │
│  可扩展: creative | research | ...   │
└──────────┬───────────────────────────┘
           │
    ┌──────┴──────┐
    │             │
 simple        complex
    │             │
    ▼             ▼
┌────────┐   ┌──────────────────────────────────┐
│ Router  │   │           PLANNER                 │
│ 直接生成│   │  独立 prompt: planner.md           │
│ tool    │   │  按需加载 skills (by category)     │
│ calls   │   │  输出: TodoList (带步骤类型)       │
└───┬────┘   └──────────────┬───────────────────┘
    │                       │
    ▼                       ▼
┌────────┐          ┌──────────────┐
│Executor│          │   TodoList   │──→ 前端 UI (pinned, 可编辑)
│ 执行   │          └──────┬───────┘
└───┬────┘                 │
    │              for each step:
    ▼                      │
  Done            ┌────────┴────────┐────────────────┐
                  │                 │                 │
            ┌─────▼─────┐   ┌──────▼──────┐   ┌─────▼──────┐
            │  direct    │   │visual_locate│   │ visual_act │
            │  盲执行     │   │ VLM 观察    │   │ VLM 自主   │
            └─────┬─────┘   └──────┬──────┘   └─────┬──────┘
                  │                │                 │
                  │                ▼                 ▼
                  │         ┌───────────┐    ┌────────────┐
                  │         │VLM observe│    │ VLM act    │
                  │         │返回定位数据│    │ 理解+决策   │
                  │         └─────┬─────┘    │ +生成tools │
                  │               │          │ +直接执行   │
                  │               ▼          └─────┬──────┘
                  │         ┌──────────┐           │
                  │         │ Executor │           │
                  │         │ 执行动作  │           │
                  │         └────┬─────┘           │
                  │              │                 │
                  └──────┬───────┘─────────────────┘
                         │
                         ▼
                  ┌─────────────┐
                  │ 最后一步？    │
                  └──────┬──────┘
                    Yes  │  No → 下一步 (loop)
                         ▼
                  ┌──────────────┐
                  │   VERIFIER   │
                  │ 截图 + 目标  │
                  │ 比对验证     │
                  └──────┬──────┘
                    pass │  fail → 回到 Planner 重新规划
                         ▼
                       Done
```

## 三种步骤执行模式

Planner 在规划 TodoList 时，为每一步指定执行模式：

| 模式 | 何时使用 | VLM 参与 | 速度 | 解耦度 |
|------|---------|---------|------|--------|
| `direct` | 已知 UI 路径，Planner 能给出精确工具链 | 无 | 最快 | — |
| `visual_locate` | 需要定位元素但操作明确 | 仅观察 | 中等 | 最高 |
| `visual_act` | 复杂视觉任务，需理解上下文后决策 | 理解+执行 | 快(省往返) | 低 |

**示例**：

```
目标: "打开 Edge 搜索今天天气"

Step 1 [direct]:     hotkey("Win+S") → wait(500) → type_text("edge") → key_press("Enter")
Step 2 [direct]:     wait(1500) → hotkey("Ctrl+T")
Step 3 [direct]:     type_text("今天天气") → key_press("Enter")
Step 4 [visual_act]: VLM 截图理解搜索结果页面，提取天气信息，finish_task(summary)
```

```
目标: "在当前表格中填入以下数据..."

Step 1 [visual_locate]: VLM 定位第一个空单元格 → Executor 点击
Step 2 [direct]:         type_text("数据1") → key_press("Tab")
Step 3 [visual_act]:     VLM 理解表格当前状态，自主完成剩余填写
Step 4 [visual_locate]:  VLM 定位保存按钮 → Executor 点击
```

## Router 三层机制

```
Query ──→ L1: Regex ──match──→ route_type
              │
              no match
              ▼
          L2: Bayesian ──confidence > θ──→ route_type
              │
              low confidence
              ▼
          L3: LLM (轻量模型) ──→ route_type + tool_calls(如果 simple)
```

- L1 正则: 维护关键词→路由映射表，如 `打开|启动|运行 → simple`
- L2 贝叶斯: 基于历史任务特征的概率分类器，可在线学习
- L3 LLM: 用小模型做分类，simple 路由时同时生成 tool calls（一次调用两件事）
- 路由类型可扩展: 当前 `simple | complex`，未来可加 `creative | research | ...`

## Prompt 组织

```
prompts/
├── system/
│   ├── router.md            # Router L3 兜底 prompt (分类 + simple 时生成 tools)
│   ├── planner.md           # Planner prompt (目标分析, TodoList 生成, skills 调度)
│   ├── executor.md          # Executor prompt (极简, 仅工具执行上下文)
│   ├── vlm_observe.md       # VLM 观察模式 (元素定位, 状态报告)
│   ├── vlm_act.md           # VLM 自主模式 (理解 + 决策 + 生成 tool calls)
│   └── verifier.md          # 最终验证 prompt (截图 vs 目标比对)
├── skills/                  # 按领域分类, Planner 按需加载
│   ├── os/
│   │   ├── open_software.md
│   │   └── file_operations.md
│   ├── web/
│   │   └── browser_actions.md
│   ├── office/              # 未来扩展
│   └── dev/                 # 未来扩展
└── tools/
    ├── builtin.json         # 核心原子工具 (mouse, keyboard, scroll...)
    └── vlm_act_tools.json   # VLM act 模式专用工具子集
```

## Skills 按需加载

Skills 不再全量注入 context，而是 Planner 根据任务类别按需加载：

```
Planner 收到目标 → 分析任务领域 → 加载对应 category 的 skills
                                    ├── "打开软件" → os/open_software.md
                                    ├── "浏览器操作" → web/browser_actions.md
                                    └── "文件管理" → os/file_operations.md
```

- Router (simple): 只加载匹配到的单个 skill（如果需要）
- Planner (complex): 根据目标关键词加载 1-3 个相关 skills
- VLM (act mode): 加载 vlm 专属 skills（如果有）
- Executor: 不加载 skills，只执行 tool calls

## 状态机

```rust
enum AgentState {
    Idle,
    Planning { goal: String },
    Executing { step_index: usize },
    VlmProcessing { step_index: usize, mode: VlmMode },  // observe | act
    WaitingForStability { step_index: usize },
    WaitingForUser { pending_action: AgentAction },
    Verifying { goal: String },                           // 最终验证
    Error { message: String },
    Done { summary: String },
}

enum VlmMode { Observe, Act }
```

注意: Router 不是状态，是 `Idle → Planning/Executing` 过渡中的同步函数调用。

## TodoStep 结构

```rust
struct TodoStep {
    index: usize,
    description: String,
    mode: StepMode,              // direct | visual_locate | visual_act
    // direct 模式
    tool_calls: Vec<ToolCall>,   // Planner 预生成的工具链
    // visual_locate 模式
    target: Option<String>,      // VLM 要定位的元素描述
    action_template: Option<AgentAction>,  // 定位后执行的动作模板
    // visual_act 模式
    vlm_goal: Option<String>,    // VLM 的子目标描述
    // 状态
    status: StepStatus,          // pending | in_progress | completed | skipped | failed
}

enum StepMode { Direct, VisualLocate, VisualAct }
enum StepStatus { Pending, InProgress, Completed, Skipped, Failed }
```

## TodoList UI 组件

Pinned 在主界面，用户可见进度并可编辑：

```
┌─────────────────────────────────────┐
│ 📋 任务计划                    3/5  │
├─────────────────────────────────────┤
│ ✅ 1. 打开搜索栏          [direct] │
│ ✅ 2. 输入 Edge 并回车     [direct] │
│ 🔄 3. 等待浏览器加载    [v_locate] │
│ ○  4. 在地址栏输入网址     [direct] │
│ ○  5. 提取页面信息        [v_act]  │
├─────────────────────────────────────┤
│ [+ 插入步骤]  [⏭ 跳过当前]         │
└─────────────────────────────────────┘
```

- 配色: 使用现有 MUI Joy 主题 (success/warning/neutral)
- ✅ completed → `success` 色
- 🔄 in_progress → `warning` 色 + 脉冲动画
- ○ pending → `neutral` 色
- ❌ failed → `danger` 色
- ⏭ skipped → `neutral` 色 + 删除线
- 可展开查看步骤详情和执行结果
- 可拖拽排序、跳过、手动标记完成、插入新步骤

## 数据流 (Tauri IPC)

```
Backend                              Frontend
  │                                    │
  ├─ emit("todolist_updated")  ──→     │ agentStore.setTodoList()
  ├─ emit("step_started", idx) ──→     │ agentStore.setCurrentStep(idx)
  ├─ emit("step_completed", idx) ──→   │ agentStore.completeStep(idx)
  ├─ emit("step_failed", idx, err) ──→ │ agentStore.failStep(idx, err)
  │                                    │
  │  ←── invoke("skip_step", idx)      │ 用户跳过步骤
  │  ←── invoke("reorder_steps", [...])│ 用户重排步骤
  │  ←── invoke("insert_step", step)   │ 用户插入步骤
  │  ←── invoke("edit_step", idx, ...) │ 用户编辑步骤
```

## 各角色 Prompt 职责

| 角色 | Prompt 文件 | 输入 | 输出 | 加载的 Skills |
|------|------------|------|------|--------------|
| Router L3 | `router.md` | query, 路由规则 | route_type, tool_calls(simple时) | 无 |
| Planner | `planner.md` | goal, 屏幕状态, 按需skills | TodoList (带 mode) | 按任务类别 |
| Executor | `executor.md` | tool_calls 序列 | 执行结果 | 无 |
| VLM Observe | `vlm_observe.md` | 截图, target 描述 | 元素坐标/页面状态 | 无 |
| VLM Act | `vlm_act.md` | 截图, 子目标, vlm_act_tools | tool_calls 序列 | vlm 专属 |
| Verifier | `verifier.md` | 截图, 原始目标 | pass/fail + 原因 | 无 |

## 关键设计原则

1. **最短路径优先**: simple 任务不经过 Planner，direct 步骤不经过 VLM
2. **按需加载**: Skills 和 context 只在需要时注入，减少 token 消耗
3. **职责单一**: 每个角色有独立 prompt，不混杂多种职责
4. **验证后置**: 只在 TodoList 最后一步完成后做一次总体验证
5. **可扩展路由**: Router 的路由类型是枚举，新增路由只需加分支
6. **用户可控**: TodoList 可编辑，用户能跳过/重排/插入步骤
