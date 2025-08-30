---
allowed-tools: Task
description: Coordinate multiple specialized agents in parallel for complex tasks
argument-hint: <complex task requiring multiple specialists>
---

# MAOS Orchestrator

$ARGUMENTS

Analyze this complex task and spawn multiple specialized agents IN PARALLEL using the Task tool.
To maximize efficiency, call multiple Task tools in a single response.

## Available Specialized Agents

- **adr-specialist** - Architecture Decision Records documentation
- **api-architect** - API design and integration patterns
- **application-architect** - Application architecture and system structure
- **backend-engineer** - Server-side development, APIs, data processing
- **business-analyst** - Business process analysis and requirements
- **claude-agent-developer** - Claude Code sub-agent configuration
- **claude-command-developer** - Claude Code slash command development
- **claude-hook-developer** - Claude Code hook system development
- **code-reviewer** - Comprehensive code reviews and quality audits
- **data-architect** - Data architecture and database systems
- **data-scientist** - Data analysis and machine learning
- **devops-engineer** - Infrastructure automation and CI/CD
- **frontend-engineer** - Frontend development and UI implementation
- **mobile-engineer** - Mobile app development (iOS/Android)
- **prd-specialist** - Product Requirements Documents
- **product-manager** - Product strategy and feature planning
- **qa-engineer** - Testing strategies and automation
- **researcher** - Technical research and evaluation
- **secops-engineer** - Security operations and incident response
- **security-architect** - Security architecture and threat models
- **solution-architect** - End-to-end enterprise solutions
- **tech-writer** - Technical documentation and guides
- **tester** - Manual and exploratory testing
- **ux-designer** - User-centered design and interfaces

## Orchestration Strategy

1. **Analyze** the task to identify required specializations
2. **Plan** which agents to spawn and their specific responsibilities
3. **Execute** by spawning multiple agents in parallel using the Task tool
4. **Coordinate** through clear task descriptions for each agent
5. **Synthesize** results as agents complete their work

## Example Parallel Execution

For a task requiring frontend, backend, and testing:
- Spawn backend-engineer with API development task
- Spawn frontend-engineer with UI implementation task
- Spawn qa-engineer with test planning task
All in the same response for parallel execution!

Remember: Spawn agents concurrently for maximum efficiency. Each agent works independently on their specialized portion of the task.