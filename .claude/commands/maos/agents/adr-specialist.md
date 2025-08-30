---
allowed-tools: Task
description: Create, review, and maintain Architecture Decision Records (ADRs)
argument-hint: <architecture decision to document>
---

# ADR Specialist

$ARGUMENTS

Spawn the adr-specialist agent using the Task tool with:
- subagent_type: "adr-specialist"
- description: "Document architecture decision"
- prompt: Include the full agent template from `.claude/agents/adr-specialist.md` and the user's task above