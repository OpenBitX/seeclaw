# Git 提交与工作流规范

**语言强制要求**：所有 Commit Message（类型、范围、主题、正文）必须全部使用英文。

## Conventional Commits

所有 Commit Message 必须采用：`<type>(<scope>): <subject>`

- `feat`: 新功能 (e.g. `feat(llm): add OpenAI provider implementation`)
- `fix`: 修复 bug (e.g. `fix(executor): correct DPI scaling on multi-monitor setups`)
- `refactor`: 重构 (e.g. `refactor(llm): extract provider trait into separate module`)
- `chore`: 构建/依赖 (e.g. `chore(deps): bump ort to 2.0`)
- `perf`: 性能优化 (e.g. `perf(vision): cache ONNX session as managed state`)
- `docs`: 文档 (e.g. `docs(arch): update system design for MCP integration`)
- `test`: 测试 (e.g. `test(rag): add unit tests for experience indexing`)
- `style`: 格式调整 (e.g. `style(ui): apply consistent theme tokens`)

## 提交主题规则

- 动词开头，祈使句 (imperative mood)
- 不超过 72 字符
- 不以句号结尾

## 依赖锁文件

- `Cargo.lock` — 提交
- `yarn.lock` — 提交
- `node_modules/` — 不提交

## .gitignore 必须包含

```
.env
node_modules/
target/
dist/
*.onnx
experience.md
```

> `experience.md` 默认不提交（因为包含运行时生成的经验数据）。如需版本管理可由用户自行移除 ignore。
