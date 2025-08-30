---
allowed-tools: Task
description: Create and maintain Claude Code custom slash commands
argument-hint: <command development task>
---

# Claude Command Developer

$ARGUMENTS

Spawn the claude-command-developer agent using the Task tool with:
- subagent_type: "claude-command-developer"
- description: "Develop slash command"
- prompt: Include the full agent template from `.claude/agents/claude-command-developer.md` and the user's task above