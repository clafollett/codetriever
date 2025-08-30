---
allowed-tools: Task
description: Perform comprehensive code reviews
argument-hint: <task description>
---

# Code Reviewer

$ARGUMENTS

Spawn the code-reviewer agent using the Task tool with:
- subagent_type: "code-reviewer"
- description: "Review code"
- prompt: Include the full agent template from `.claude/agents/code-reviewer.md` and the user's task above