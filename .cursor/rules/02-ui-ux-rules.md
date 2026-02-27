# React 前端开发规范

**语言强制要求**：所有代码（变量名、函数名、类型名）、代码注释必须使用英文。

## 1. 严禁前端业务逻辑

前端组件不允许出现任务解析、状态持久化计算。前端只做两件事：触发 Tauri `invoke` 和监听 Tauri `event`。

## 2. TypeScript 严格模式

- `tsconfig.json` 开启 `strict: true`
- 所有跨组件数据必须定义 `interface` / `type`，统一放在 `src-ui/src/types/` 目录
- 禁止使用 `any`，必须为所有 Tauri event payload 和 invoke 参数定义类型

## 3. 设计风格 — 参考 Claude / ChatGPT

- **简洁、大气、克制**，不追求炫酷
- 配色以**灰色、黑色、白色、米色**为主色调，模拟 OpenAI 风格
- **禁止使用渐变色**（`linear-gradient` / `radial-gradient`）
- **禁止滥用毛玻璃**：`backdrop-filter: blur()` 仅允许用于全屏 Modal 覆盖层。卡片、侧边栏、聊天气泡等一律使用实色背景

## 4. 双主题支持 (Light / Dark)

- 使用 `@mui/joy` 的 `CssVarsProvider` + `extendTheme` 定义 light 和 dark 两套 colorScheme
- 主题定义**统一放在 `src-ui/src/theme/` 目录**，禁止散落在组件中
- 禁止在组件内写硬编码颜色值（`color: '#xxx'`），必须引用 theme token
- 禁止 magic number：所有间距、圆角、字号均使用 theme token 或设计常量

## 5. 状态管理

- 使用 `MobX`（`mobx` + `mobx-react-lite`）严格模式
- 所有业务状态存入 MobX Store，严禁 Zustand
- 组件内 `useState` 仅用于纯 UI 交互状态（如 tooltip 显隐），严禁管理业务数据

## 6. 可复用代码组织

| 目录 | 放什么 |
|---|---|
| `src-ui/src/utils/` | 纯函数工具：格式化时间、JSON 解析守卫、字符串截断等 |
| `src-ui/src/hooks/` | 自定义 hooks：`useTauriEvent`、`useThemeMode`、`useLoopStatus` 等 |
| `src-ui/src/types/` | 全局 interface / type：`AgentStatus`、`ActionCard`、`SafetyConfig` 等 |
| `src-ui/src/theme/` | `extendTheme` 定义 + 颜色 token + typography token |

凡是被 2 个及以上组件使用的逻辑/函数，必须提取到对应目录。

## 7. Joy UI 组件使用原则

- 变体优先级：`solid`（主操作）> `outlined`（次操作）> `soft`（标签）> `plain`（文本按钮）
- 高危操作：`color="danger"`
- 允许/确认操作：`color="success"`
- 对话流设计参考 Claude：消息列表 + 底部输入框，简洁单列布局

## 8. 安全配置 UI

- 提供 Settings 页面，允许用户配置：
  - 允许/禁止终端命令执行
  - 允许/禁止文件操作
  - 需要人工审批的操作类型列表
  - 循环策略（无限 / 定时 / 失败上限）
- 配置变更通过 Tauri invoke 写入 `config.toml`

## 9. 包管理

统一使用 `yarn`。`package.json` 中 scripts 字段可定义 `dev`、`build`、`lint`、`test` 等常用命令。
