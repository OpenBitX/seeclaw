# 技能：Joy UI 圆角定制与流式渲染

1. **全局主题**: 使用 `@mui/joy/styles` 的 `CssVarsProvider`。必须在全局覆盖 `radius` 变量，将组件的默认圆角调大（如 `md: '12px', lg: '16px'`），营造现代桌面软件的果冻感/拟物感。
2. **思考过程流式渲染 (Collapsible Reasoning)**: 
   - 前端接收到 `llm_stream_chunk` 时，如果是思考过程（Reasoning），存入局部状态。
   - 使用 Joy UI 的 `Accordion` 或自定义可折叠卡片包裹这段文本。
   - 默认状态设为 `collapsed`，仅通过单行文字高亮（配合 CSS 渐隐动画）显示最新吐出的思考字面量。
3. **特殊动作卡片**: 对于大模型的 Tool Call（如点击坐标、终端命令），不要用普通文本渲染。必须提取出 JSON，在 UI 上渲染成一张带有专属 Icon 的独立卡片。若是高危动作，卡片上需附加绿色的“允许 (Approve)”和红色的“拒绝 (Reject)”按钮。