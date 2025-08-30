#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.8"
# ///

import json
import sys
import os
from pathlib import Path
from typing import Dict, Optional

# Add path resolution for proper imports
sys.path.insert(0, str(Path(__file__).parent.parent))  # Get to maos directory
# Import MAOS backend utilities
try:
    from utils.backend import MAOSBackend, extract_file_path_from_tool_input
    from utils.path_utils import PROJECT_ROOT
except ImportError:
    # Fallback if backend not available
    MAOSBackend = None
    PROJECT_ROOT = Path.cwd()


class MAOSCoordinator:
    """MAOS coordination layer for Claude Code sub-agents"""
    
    def __init__(self, hook_metadata=None):
        self.hook_metadata = hook_metadata or {}
        self.backend = MAOSBackend() if MAOSBackend else None
        self._session_id = None
    
    def get_session_id(self):
        """Get or create session ID"""
        if not self.backend:
            return "fallback-session"
        
        if not self._session_id:
            self._session_id = self.backend.get_or_create_session(self.hook_metadata)
        return self._session_id
    
    def handle_subagent_spawning(self, tool_input):
        """Handle Task tool for sub-agent spawning with lazy workspace creation"""
        if not self.backend:
            print("MAOS backend not available, skipping orchestration", file=sys.stderr)
            return
        
        agent_type = tool_input.get('subagent_type')
        if not agent_type:
            return
        
        try:
            session_id = self.get_session_id()
            
            # Register agent with full hook context for proper matching
            hook_context = {
                **self.hook_metadata,
                'subagent_type': agent_type,
                'spawned_at': session_id
            }
            agent_id = self.backend.register_pending_agent(agent_type, session_id, hook_context)
            
            # Modify the prompt to include conditional workspace instruction
            original_prompt = tool_input.get('prompt', '')
            workspace_path = f"{PROJECT_ROOT}/worktrees/{agent_type}-{session_id}"
            
            workspace_instruction = f"""

WORKSPACE MANAGEMENT:
Your workspace will be created automatically when you first perform file operations.
When using Read, Write, Edit, or MultiEdit tools, the system will:
1. Create your isolated workspace at: {workspace_path}/
2. All subsequent file operations must use paths within this workspace
3. Use absolute paths starting with your workspace directory

Note: The workspace is created on-demand to save resources. Don't manually create directories - let the system handle workspace setup when you first need it.

AGENT CONTEXT: Agent ID: {agent_id}, Type: {agent_type}"""
            
            tool_input['prompt'] = original_prompt + workspace_instruction
            
            # Store agent context in hook metadata instead of environment (eliminates race conditions)
            self.hook_metadata['maos_agent_id'] = agent_id
            self.hook_metadata['maos_agent_type'] = agent_type
            
            print(f"🚀 MAOS: Registered {agent_type} for lazy workspace creation", file=sys.stderr)
            print(f"📋 Session: {session_id}, Agent ID: {agent_id}", file=sys.stderr)
            
        except Exception as e:
            # Don't block the operation, just log the error
            print(f"⚠️  MAOS agent registration failed: {e}", file=sys.stderr)
            print("Continuing without workspace isolation", file=sys.stderr)
    
    def should_create_workspace(self, tool_name, tool_input):
        """
        Selective worktree creation rules - only create workspaces for file modification tools.
        
        MAOS philosophy: Workspace isolation is expensive and should only be used when necessary
        for conflict prevention in multi-agent file operations.
        """
        # File modification tools that require workspace isolation
        WORKSPACE_REQUIRED_TOOLS = {
            "Write", "Edit", "MultiEdit", "NotebookEdit"
        }
        
        # Tools that read files but don't modify them - no workspace needed
        READ_ONLY_TOOLS = {
            "Read", "Grep", "Glob", "LS"
        }
        
        # Non-file tools - no workspace needed
        NON_FILE_TOOLS = {
            "Bash", "Task", "WebFetch", "WebSearch", "BashOutput", "KillBash", "TodoWrite"
        }
        
        # Only create workspace for file modification tools
        if tool_name in WORKSPACE_REQUIRED_TOOLS:
            return True
        elif tool_name in READ_ONLY_TOOLS or tool_name in NON_FILE_TOOLS:
            return False
        else:
            # Default: be conservative and create workspace for unknown tools
            print(f"🤔 MAOS: Unknown tool '{tool_name}' - creating workspace conservatively", file=sys.stderr)
            return True
    
    def handle_file_operations(self, tool_name, tool_input):
        """Handle file operation with selective lazy workspace creation using atomic directory operations"""
        if not self.backend:
            return
        
        file_path = extract_file_path_from_tool_input(tool_input)
        if not file_path:
            return
        
        try:
            session_id = self.get_session_id()
            
            # Get agent context from hook metadata (race-condition free)
            agent_id = self.hook_metadata.get('maos_agent_id')
            agent_type = self.hook_metadata.get('maos_agent_type')
            
            if not agent_id or not agent_type:
                # No agent context - this is main session, not sub-agent
                return
            
            # Use new atomic directory-based state management
            state_manager = self.backend._get_state_manager(session_id)
            agent_state = state_manager.get_agent_state(agent_id)
            
            # Apply selective worktree creation rules
            if agent_state == "pending" and self.should_create_workspace(tool_name, tool_input):
                # Create workspace atomically only for file modification tools
                workspace_path = self.backend.create_workspace_if_needed(agent_id, session_id)
                if workspace_path:
                    print(f"🏗️  MAOS: Created workspace for {agent_type} at {workspace_path}", file=sys.stderr)
                    print(f"🎯 Triggered by: {tool_name} operation on {file_path}", file=sys.stderr)
                    # Enforce workspace usage
                    self.enforce_workspace_path(tool_name, file_path, workspace_path)
            elif agent_state == "pending":
                # Agent is pending but tool doesn't require workspace - log this
                print(f"📖 MAOS: Agent {agent_type} using read-only tool {tool_name} - no workspace needed", file=sys.stderr)
                    
            elif agent_state == "active":
                # Get workspace path from active agent data
                active_agents = state_manager.get_active_agents()
                for agent_data in active_agents:
                    if agent_data.get("agent_id") == agent_id:
                        workspace_path = agent_data.get("workspace_path")
                        if workspace_path and self.should_create_workspace(tool_name, tool_input):
                            # Workspace exists and tool requires it, enforce its usage
                            self.enforce_workspace_path(tool_name, file_path, workspace_path)
                        break
            
            # Handle file locking for modification tools
            if self.should_create_workspace(tool_name, tool_input):
                lock_info = self.backend.check_file_lock(file_path, session_id, agent_id)
                if lock_info:
                    print(f"⚠️  File {file_path} is being edited by {lock_info.get('agent_id', 'another agent')}", 
                          file=sys.stderr)
                    print(f"Operation: {lock_info.get('operation', 'unknown')}", file=sys.stderr)
                
                # Acquire lock for file modification operations
                if tool_name in ["Edit", "Write", "MultiEdit"]:
                    lock_acquired = self.backend.acquire_file_lock(file_path, agent_id, session_id, tool_name, timeout=5.0)
                    if not lock_acquired:
                        print(f"🔒 Could not acquire lock on {file_path} - operation may conflict", file=sys.stderr)
            
        except Exception as e:
            # Non-blocking error
            print(f"⚠️  MAOS file operation handling failed: {e}", file=sys.stderr)
    
    def update_progress(self, tool_name, tool_input):
        """Update progress tracking using hook context"""
        if not self.backend:
            return
        
        try:
            session_id = self.get_session_id()
            
            # Get agent context from hook metadata (race-condition free)
            agent_type = self.hook_metadata.get('maos_agent_type')
            if not agent_type:
                # No agent context - this is main session, not sub-agent
                return
            
            # Extract relevant details for progress tracking
            details = {}
            if tool_name in ["Edit", "Write", "MultiEdit"]:
                file_path = extract_file_path_from_tool_input(tool_input)
                if file_path:
                    details['file_path'] = file_path
            elif tool_name == "Bash":
                command = tool_input.get('command', '')[:100]  # Truncate long commands
                details['command'] = command
            
            # Use agent_type instead of agent_id for progress tracking (legacy compatibility)
            self.backend.update_progress(agent_type, session_id, tool_name, details)
            
        except Exception as e:
            # Non-blocking error
            pass
    
    def enforce_workspace_path(self, tool_name, file_path, workspace_path):
        """Enforce that file operations use the assigned workspace"""
        # Convert to Path objects for comparison
        file_path_obj = Path(file_path)
        workspace_path_obj = Path(workspace_path)
        
        # Check if file path is absolute and outside workspace
        if file_path_obj.is_absolute():
            try:
                # Resolve paths for accurate comparison
                file_resolved = file_path_obj.resolve()
                workspace_resolved = workspace_path_obj.resolve()
                
                if not str(file_resolved).startswith(str(workspace_resolved)):
                    # Block the operation
                    print(f"\n❌ BLOCKED: File operations must use assigned workspace", file=sys.stderr)
                    print(f"   Attempted: {file_path}", file=sys.stderr)
                    print(f"   ✅ Use instead: {workspace_path}/{file_path_obj.name}", file=sys.stderr)
                    print(f"\n   Your workspace: {workspace_path}/", file=sys.stderr)
                    print(f"   All file operations MUST use paths within this directory\n", file=sys.stderr)
                    sys.exit(2)  # Exit code 2 blocks the operation
            except Exception:
                # If we can't resolve paths, be conservative and block
                if not file_path.startswith(workspace_path):
                    print(f"\n❌ BLOCKED: File operations must use assigned workspace", file=sys.stderr)
                    print(f"   Your workspace: {workspace_path}/\n", file=sys.stderr)
                    sys.exit(2)


def handle_maos_pre_tool(tool_name: str, tool_input: Dict, hook_metadata: Optional[Dict] = None):
    """Main MAOS pre-tool processing function"""
    # Initialize MAOS coordinator
    coordinator = MAOSCoordinator(hook_metadata)
    
    # Handle sub-agent spawning through Task tool
    if tool_name == "Task" and tool_input.get('subagent_type'):
        coordinator.handle_subagent_spawning(tool_input)
    
    # Handle file operations for conflict detection
    elif tool_name in ["Edit", "Write", "MultiEdit", "Read"]:
        coordinator.handle_file_operations(tool_name, tool_input)
    
    # Update progress tracking
    coordinator.update_progress(tool_name, tool_input)