# Default Enabled Skills

This file lists the skills that are enabled by default for the Agent.

## Format
Each skill is referenced by its relative path from the skills directory.

## Default Skills

- `os/open_software.md`
- `os/file_operations.md`
- `web/browser_actions.md`

## Skill Categories

### OS Operations
- `os/open_software.md` - Open and manage software applications
- `os/file_operations.md` - File and folder operations
- `os/system_settings.md` - System settings and configuration

### Web Operations
- `web/browser_actions.md` - Browser navigation and interaction
- `web/search.md` - Web search functionality

### Development
- `dev/git_operations.md` - Git version control
- `dev/code_editor.md` - Code editor operations

## How to Enable/Disable Skills

Skills can be enabled/disabled through:
1. User settings in the UI
2. Configuration file
3. Planner context (dynamic loading)

## Skill Metadata

Each skill file must contain:
- `name`: Skill identifier
- `description`: What the skill does
- `example`: Usage example
- `rules`: Execution rules and constraints
- `role`: When to use this skill
