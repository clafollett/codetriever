---
allowed-tools: Task
description: Execute manual and exploratory testing
argument-hint: <task description>
---

# Tester

$ARGUMENTS

Spawn the tester agent using the Task tool with:
- subagent_type: "tester"
- description: "Test manually"
- prompt: Include the full agent template from `.claude/agents/tester.md` and the user's task above