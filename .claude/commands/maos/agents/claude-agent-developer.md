---
allowed-tools: Task
description: Create or edit Claude Code sub-agent configuration files
argument-hint: <agent creation/modification task>
---

# Claude Agent Developer

$ARGUMENTS

Spawn the claude-agent-developer agent using the Task tool with:
- subagent_type: "claude-agent-developer"
- description: "Develop Claude agent"
- prompt: Include the full agent template from `.claude/agents/claude-agent-developer.md` and the user's task above