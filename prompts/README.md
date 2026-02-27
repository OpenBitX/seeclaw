# SeeClaw Prompts Directory

This directory contains all prompt templates and tool definitions for the SeeClaw agent.
These are **pure text files** (JSON / Markdown) and contain no Rust code.

## Structure

```
prompts/
├── tools/
│   ├── builtin.json          # All 14 atomic tool definitions (OpenAI function calling format)
│   └── mcp_template.json     # Template for dynamically injecting MCP tools
└── system/
    ├── agent_system.md       # Agent System Prompt template ({placeholder} syntax)
    └── experience_summary.md # Experience summarization prompt template
```

## Placeholder Syntax

System prompts use `{placeholder}` syntax. Rust injects values at runtime via `str.replace()`:

| Placeholder | Source |
|---|---|
| `{elements_xml}` | PerceptionContext — serialized UI element list |
| `{experience_context}` | RAG — top-K retrieved experience entries |
| `{skills_list}` | Skills loader — installed skill names + descriptions |
| `{mcp_tools_list}` | MCP client — tool list from connected servers |
| `{user_language_hint}` | Auto-detected from user input (CJK → Chinese, else English) |

## Maintenance Rules

- Modify tool behavior by editing `builtin.json` — no Rust recompilation needed.
- Add new tools to `builtin.json` following the existing OpenAI function calling format.
- MCP tools are injected dynamically at runtime — do not add them to `builtin.json`.
- Keep prompt templates clean and version-controlled in Git.
