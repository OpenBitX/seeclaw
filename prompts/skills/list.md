# Skills Index

Skills are pre-defined deterministic action sequences (combos) that the agent can execute with zero LLM calls — the fastest and most reliable execution path.

## File Format

Each skill is a single `.skill.json` file containing:
- `name` — unique identifier
- `description` — what the skill does (shown to Planner)
- `params` — named parameters the combo accepts
- `triggers` — keyword hints for when this skill applies (shown to Planner)
- `steps` — ordered action sequence with `{param}` placeholders

## Enabled Skills

### OS Operations
- `os/open_software` — 通过 Win+S 系统搜索打开任意 Windows 软件应用
- `os/file_operations` — Windows 文件/文件夹操作，包括打开资源管理器、导航目录

### Web Operations
- `web/browser_actions` — 浏览器导航和交互，包括打开 URL、刷新、标签页管理
- `web/browser_search` — 在浏览器地址栏中搜索关键词

## How Skills Are Used

1. **Planner** sees skill summaries (name + description + triggers) to recommend `combo` mode
2. **StepRouter** checks skill triggers to auto-detect combo matches
3. **ComboExecNode** loads the full `.skill.json` and expands `{param}` placeholders
4. **StepEvaluate** verifies combo success; on failure, falls back to VLM loop
