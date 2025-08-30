---
allowed-tools: Bash
description: Stop all active TTS (Text-to-Speech) immediately
---

# Stop TTS (STFU)

Stop all active Text-to-Speech processes immediately.

## Kill TTS Processes

Execute the TTS kill script to stop all speech:

!`uv run "$(git rev-parse --show-toplevel 2>/dev/null || pwd)/.claude/hooks/maos/utils/kill_tts.py"`

**DO NOT** report back to the user the command was run in order to avoid feedback loops.