# SeeClaw 架构重构总结

## 重构目标

将执行模型从 **"Planner 预定义固定步骤 → 四模式静态分发"** 重构为 **"Planner 输出高层计划包 → StepRouter 动态路由 → 三模式循环执行 + Agent 自主模式切换"**。

---

## 核心变化

### 执行模式简化

| 旧模式 | 新模式 |
|--------|--------|
| `Combo` | `Combo` — 零 LLM，直接执行 Skill |
| `Direct` | `Chat` — LLM 循环，处理终端/键盘/文件操作 |
| `VisualLocate` | `Vlm` — VLM 循环，处理视觉交互任务 |
| `VisualAct` | （合并到 Vlm） |

### 图拓扑变化

**旧拓扑：**
```
planner → step_dispatch → {combo_exec, direct_exec, vlm_observe → vlm_act} → action_exec → step_advance → verifier
```

**新拓扑：**
```
planner → step_router → {combo_exec, chat_agent, vlm_act} → action_exec → step_evaluate → step_advance → step_router (循环)
                                                                                          ↘ verifier（所有步骤完成）
```

---

## 文件变更清单

### 新增文件

| 文件 | 说明 |
|------|------|
| `src/agent_engine/nodes/step_router.rs` | 动态路由节点：combo 技能匹配 → 触发器匹配 → 关键词启发式 → Planner 建议 |
| `src/agent_engine/nodes/chat_agent.rs` | ChatAgent 节点：非视觉任务的 LLM 循环执行 |
| `src/agent_engine/nodes/step_evaluate.rs` | 步骤评估节点：内循环控制，决定继续/切换模式/推进步骤 |
| `prompts/system/chat_agent.md` | ChatAgent 系统提示词 |

### 删除文件

| 文件 | 原因 |
|------|------|
| `src/agent_engine/nodes/step_dispatch.rs` | 被 `step_router.rs` 替代 |
| `src/agent_engine/nodes/direct_exec.rs` | 被 `chat_agent.rs` 替代 |
| `src/agent_engine/nodes/vlm_observe.rs` | 功能合并到 `vlm_act.rs` |

### 修改文件

#### `src/agent_engine/state.rs`
- `StepMode` 枚举：`{Combo, Chat, Vlm}`，默认值 `Chat`
- `TodoStep` 字段：移除 `tool_calls`、`target`、`action_template`、`vlm_goal`；新增 `recommended_mode`、`required_skills`、`guidance`
- `AgentAction::PlanTask`：新增 `final_goal` 和 `plan_summary` 字段
- `SharedState`：新增 `plan_summary`、`final_goal`、`current_loop_mode`、`mode_switch_requested`、`step_complete`、`last_exec_result`、`step_messages`

#### `src/agent_engine/tool_parser.rs`
- 完全重写 `parse_plan_task()`，解析新格式（`final_goal` + `plan_summary` + 简化步骤）
- 移除 `parse_step_tool_calls()` 辅助函数及 `ToolCallData` 导入

#### `prompts/system/planner.md`
- Planner 角色：从"蓝图制造者"变为"需求分析者"
- 步骤是**高层子目标**，不再是具体键盘/鼠标操作序列
- `recommended_mode` 枚举：`combo | chat | vlm`

#### `prompts/tools/builtin.json`
- `plan_task` schema 更新：要求 `final_goal`、`plan_summary`、`steps`（含 `recommended_mode`）
- 新增工具：`finish_step`、`switch_to_vlm`、`switch_to_chat`

#### `src/agent_engine/nodes/planner.rs`
- 解构新的 `PlanTask { final_goal, plan_summary, steps }` 变体
- 写入 `state.final_goal` 和 `state.plan_summary`

#### `src/agent_engine/nodes/vlm_act.rs`
- 使用 `step.description` 作为目标（不再是已删除的 `vlm_goal`）
- 注入 `guidance`、`final_goal`、`last_exec_result` 到 VLM 提示
- 支持 `switch_to_chat` 和 `finish_step` 信号
- 所有失败路径路由到 `step_evaluate`

#### `src/agent_engine/nodes/combo_exec.rs`
- 无技能/无 combo 的回退目标从 `vlm_act` 改为 `chat_agent`
- 移除 `vlm_goal` 注入

#### `src/agent_engine/nodes/step_advance.rs`
- 新增每步字段清理：`step_complete`、`mode_switch_requested`、`last_exec_result`、`step_messages`

#### `src/agent_engine/nodes/action_exec.rs`
- 执行后填充 `state.last_exec_result`
- 正常执行路由目标：`step_advance` → `step_evaluate`

#### `src/agent_engine/nodes/user_confirm.rs`
- 拒绝路径路由目标：`step_advance` → `step_evaluate`

#### `src/agent_engine/flow.rs`
- 完全重写，实现新图拓扑

#### `src/agent_engine/nodes/mod.rs`
- 注册新模块：`chat_agent`、`step_evaluate`、`step_router`
- 注销旧模块：`direct_exec`、`step_dispatch`、`vlm_observe`

#### `src/agent_engine/graph.rs`
- 更新 UI 状态标签：`step_router → "routing"`、`chat_agent → "executing"`、`step_evaluate → "evaluating"`
- 移除 `vlm_observe` 条目

#### `src/agent_engine/router/visual_router/regex_layer.rs`
- `StepMode::VisualAct` → `StepMode::Vlm`（修复编译错误）

---

## StepRouter 决策逻辑

```
1. recommended_mode == Combo && skill 在 registry 中存在？
   → GoTo combo_exec

2. match_triggers(step.description) 返回匹配技能？
   → recommended_mode = Combo，GoTo combo_exec

3. is_chat_like() 关键词匹配（terminal/cmd/file/键盘...）？
   → GoTo chat_agent

4. is_vlm_like() 关键词匹配（截图/点击/界面...）？
   → GoTo vlm_act

5. 回退到 step.recommended_mode（Planner 建议）
```

---

## StepEvaluate 控制逻辑

```
step_complete == true           → GoTo step_advance（推进到下一步）
mode_switch_requested.is_some() → GoTo step_router（模式切换）
step_messages.len() >= 15       → 强制失败 + GoTo step_advance
current_loop_mode == Combo      → GoTo step_advance（Combo 原子执行，不循环）
其他                            → GoTo 当前循环 Agent（chat_agent 或 vlm_act）
```

---

## Skill 文件格式变更（Steps 1-3，上一 session）

- 从分散的 `.json`/`.combo.json`/`.manifest.md` 三文件合并为单一 `.skill.json`
- `SkillRegistry` 重写，统一加载 `.skill.json` 格式

---

## 编译状态

```
cargo check → Finished `dev` profile [unoptimized + debuginfo]
零 errors，零 warnings ✅
```

---

## 待办（可选）

- [ ] **ClassifierPipeline 泛化**：将 `visual_router/` 中的三层分类器（regex → bayesian → LLM）抽象为通用 `ClassifierPipeline<T>`
- [ ] **StepRouter Bayesian 层**：在触发器匹配和关键词启发式之间增加 TF-IDF 评分层
- [ ] **运行时测试**：验证 combo 路径、chat_agent 循环、vlm_act 模式切换、max iteration 保护
- [ ] **前端事件更新**：检查 TypeScript UI 中 `step_started` 事件是否需要适配新的 `recommended_mode` 字段
