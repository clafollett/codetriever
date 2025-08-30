---
allowed-tools: Task
description: Create technical documentation and guides
argument-hint: <task description>
---

# Tech Writer

$ARGUMENTS

Spawn the tech-writer agent using the Task tool with:
- subagent_type: "tech-writer"
- description: "Write documentation"
- prompt: Include the full agent template from `.claude/agents/tech-writer.md` and the user's task above