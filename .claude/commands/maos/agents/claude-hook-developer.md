---
allowed-tools: Task
description: Develop and maintain Claude Code hook systems
argument-hint: <hook development task>
---

# Claude Hook Developer

$ARGUMENTS

Spawn the claude-hook-developer agent using the Task tool with:
- subagent_type: "claude-hook-developer"
- description: "Develop Claude hook"
- prompt: Include the full agent template from `.claude/agents/claude-hook-developer.md` and the user's task above