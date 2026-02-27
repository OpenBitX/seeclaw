# 全局项目规范

## 敏感信息

- API Key / Token 存放在 `.env`，**绝不提交 Git**
- `.env` 必须在 `.gitignore` 中

## 配置管理

- 根目录 `config.toml` 管理所有应用配置（LLM Provider 选择、模型参数、MCP Server 列表、安全策略、循环策略等）
- 配置结构使用 Rust `serde` 反序列化为强类型 struct，前端通过 Tauri invoke 读取

## 目录结构强制约束

| 类别 | 路径 | 说明 |
|---|---|---|
| Rust 核心 | `src/` | 状态机、LLM、感知、执行、MCP、RAG、Skills |
| 提示词与 Tool 定义 | `prompts/` | **与 Rust 代码解耦**；Tool JSON Schema、System Prompt 模板 |
| ↳ 内置 Tool 定义 | `prompts/tools/builtin.json` | 所有原子 Tool 的 OpenAI function calling JSON |
| ↳ System Prompt 模板 | `prompts/system/` | `agent_system.md`、`experience_summary.md` |
| React 前端 | `src-ui/` | 组件、Store、主题、hooks、utils、types |
| 前端主题 | `src-ui/src/theme/` | 统一 light/dark token 定义，禁止散落 |
| 前端工具函数 | `src-ui/src/utils/` | 可复用纯函数（格式化、类型守卫等） |
| 前端 Hooks | `src-ui/src/hooks/` | 可复用 React hooks |
| 前端类型 | `src-ui/src/types/` | 全局 TS interface / type |
| 脚本 | `scripts/` | PowerShell `.ps1` 脚本（构建、测试、打包） |
| 用户 Skills | `skills/` | 用户导入的自动化技能包 |

## Prompts 维护规则

- `prompts/` 目录下的文件是**纯文本**（JSON / Markdown），不包含 Rust 代码
- Tool 定义修改只需编辑 `prompts/tools/builtin.json`，无需重新编译 Rust
- System Prompt 模板使用 `{placeholder}` 语法，Rust 侧在运行时 `str.replace()` 注入变量
- `prompts/` 目录**提交到 Git**（非敏感内容）

## 包管理器

- **前端**: 统一使用 `yarn`，禁止 npm / pnpm
- **Rust**: `cargo`
- **脚本**: `scripts/` 下所有脚本使用 PowerShell (`.ps1`) 语法

## 代码语言

- 所有代码标识符、注释、Git 提交信息使用**英文**
- 用户面向的 UI 文本、文档使用中文
- AI 对话内容支持中英文
