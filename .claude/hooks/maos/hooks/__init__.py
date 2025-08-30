"""
MAOS (Multi-Agent Orchestration System) - Clean Architecture Module

This module contains all MAOS-specific orchestration code separated from 
the main Claude Code hooks for maintainability and clarity.

Components:
- backend.py: Core MAOS backend utilities (git worktree management, session coordination)
- pre_tool_handler.py: Pre-tool execution logic (agent spawning, workspace isolation)
- post_tool_handler.py: Post-tool execution logic (cleanup, progress tracking)
- test_integration.py: Integration tests for hook system
- test_orchestration.py: Comprehensive orchestration test suite
"""

__version__ = "1.0.0"
__author__ = "MAOS Hook Developer"

# Make key functions available at package level
try:
    from .pre_tool_handler import handle_maos_pre_tool
    from .post_tool_handler import handle_maos_post_tool
    from .backend import MAOSBackend
    
    __all__ = ['handle_maos_pre_tool', 'handle_maos_post_tool', 'MAOSBackend']
except ImportError:
    # Graceful degradation if imports fail
    __all__ = []