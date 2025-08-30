#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.8"
# ///

import json
import subprocess
import sys
import time
import uuid
from pathlib import Path
from datetime import datetime
from typing import Dict, Optional
from .state_manager import MAOSStateManager
from .file_locking import MAOSFileLockManager
from .path_utils import PROJECT_ROOT, MAOS_DIR, LOGS_DIR, HOOKS_DIR, WORKTREES_DIR


def run_git_command(cmd, cwd=None):
    """Run git command with explicit directory specification."""
    if cwd is None:
        cwd = PROJECT_ROOT
    # Always run git with explicit directory
    full_cmd = ['git', '-C', str(cwd)] + cmd
    return subprocess.run(full_cmd, capture_output=True, text=True)

class MAOSBackend:
    """Backend utilities for MAOS orchestration - not user-facing"""
    
    def __init__(self):
        # Use absolute paths from global constants
        self.maos_dir = MAOS_DIR
        self.sessions_dir = self.maos_dir / "sessions"
        self.worktrees_dir = WORKTREES_DIR
        
        # Ensure directories exist (with parents)
        self.maos_dir.mkdir(parents=True, exist_ok=True)
        self.sessions_dir.mkdir(parents=True, exist_ok=True)
        self.worktrees_dir.mkdir(parents=True, exist_ok=True)
        
        # State managers per session (lazy-loaded)
        self._state_managers: Dict[str, MAOSStateManager] = {}
        
        # File lock managers per session (lazy-loaded)
        self._lock_managers: Dict[str, MAOSFileLockManager] = {}
    
    def get_or_create_session(self, hook_metadata: Optional[Dict] = None) -> str:
        """Get active session ID or create new one"""
        # Try to get from hook metadata first
        if hook_metadata and hook_metadata.get('session_id'):
            session_id = hook_metadata['session_id']
        else:
            # Check for active session
            active_file = self.maos_dir / "active_session.json"
            if active_file.exists():
                try:
                    with open(active_file) as f:
                        session_id = json.load(f).get('session_id')
                        if session_id:
                            return session_id
                except (json.JSONDecodeError, KeyError):
                    pass
            
            # Create new session
            session_id = f"sess-{int(time.time())}"
        
        # Initialize session
        self.init_session(session_id, hook_metadata)
        return session_id
    
    def init_session(self, session_id: str, hook_metadata: Optional[Dict] = None):
        """Initialize a new MAOS session"""
        session_dir = self.sessions_dir / session_id
        session_dir.mkdir(parents=True, exist_ok=True)
        
        session_data = {
            "id": session_id,
            "start": datetime.now().isoformat(),
            "metadata": hook_metadata or {},
            "status": "active"
        }
        
        # Write session data
        with open(session_dir / "session.json", 'w') as f:
            json.dump(session_data, f, indent=2)
        
        # Initialize directory-based state manager (modern atomic operations)
        state_manager = self._get_state_manager(session_id)
        
        # Update active session pointer
        with open(self.maos_dir / "active_session.json", 'w') as f:
            json.dump({"session_id": session_id}, f, indent=2)
    
    def _get_state_manager(self, session_id: str) -> MAOSStateManager:
        """Get or create state manager for session (lazy loading)"""
        if session_id not in self._state_managers:
            session_dir = self.sessions_dir / session_id
            self._state_managers[session_id] = MAOSStateManager(session_id, session_dir)
        return self._state_managers[session_id]
    
    def _get_lock_manager(self, session_id: str) -> MAOSFileLockManager:
        """Get or create file lock manager for session (lazy loading)"""
        if session_id not in self._lock_managers:
            session_dir = self.sessions_dir / session_id
            self._lock_managers[session_id] = MAOSFileLockManager(session_id, session_dir)
        return self._lock_managers[session_id]
    
    def register_pending_agent(self, agent_type: str, session_id: str, hook_data: Optional[Dict] = None) -> str:
        """Register an agent for lazy workspace creation using atomic directory operations"""
        # Generate truly unique, session-scoped agent ID using UUID
        unique_suffix = str(uuid.uuid4())[:8]  # Short UUID for readability
        agent_id = f"{agent_type}-{session_id}-{unique_suffix}"
        
        # Use directory-based state manager for atomic operations
        state_manager = self._get_state_manager(session_id)
        
        # Register with full hook context
        hook_context = hook_data or {}
        state_manager.register_pending_agent(agent_id, agent_type, hook_context)
        
        return agent_id
    
    def get_agent_info(self, agent_id: str, session_id: str) -> Optional[Dict]:
        """Get information about a pending or active agent using atomic directory operations"""
        state_manager = self._get_state_manager(session_id)
        
        # Check current state
        state = state_manager.get_agent_state(agent_id)
        if not state:
            return None
        
        # Get agent data based on state
        if state == "pending":
            pending_agents = state_manager.get_pending_agents()
            for agent in pending_agents:
                if agent.get("agent_id") == agent_id:
                    return {
                        "type": agent.get("agent_type"),
                        "session": agent.get("session_id"),
                        "registered_at": agent.get("timestamp"),
                        "workspace_created": False,
                        "workspace_path": None,
                        "status": "pending"
                    }
        elif state == "active":
            active_agents = state_manager.get_active_agents()
            for agent in active_agents:
                if agent.get("agent_id") == agent_id:
                    return {
                        "type": agent.get("agent_type"),
                        "session": agent.get("session_id"),
                        "registered_at": agent.get("timestamp"),
                        "workspace_created": True,
                        "workspace_path": agent.get("workspace_path"),
                        "status": "active"
                    }
        
        return None
    
    def create_workspace_if_needed(self, agent_id: str, session_id: str) -> Optional[str]:
        """Create workspace for agent if not already created using atomic state transitions"""
        state_manager = self._get_state_manager(session_id)
        agent_info = self.get_agent_info(agent_id, session_id)
        
        if not agent_info:
            return None
        
        # If workspace already created, return its path
        if agent_info.get("workspace_created") and agent_info.get("workspace_path"):
            return agent_info["workspace_path"]
        
        # Create workspace
        agent_type = agent_info["type"]
        workspace_path = self.prepare_workspace(agent_type, session_id)
        
        # Atomically transition from pending to active
        success = state_manager.transition_to_active(agent_id, workspace_path)
        if success:
            return workspace_path
        else:
            # Agent may have been transitioned by another process
            updated_info = self.get_agent_info(agent_id, session_id)
            return updated_info.get("workspace_path") if updated_info else None
    
    def prepare_workspace(self, agent_type: str, session_id: str) -> str:
        """Create git worktree for agent"""
        workspace = self.worktrees_dir / f"{agent_type}-{session_id}"
        branch = f"agent/session-{session_id}/{agent_type}"
        
        try:
            # Create worktree using our git wrapper
            result = run_git_command([
                "worktree", "add", "-b", branch, str(workspace)
            ])
            if result.returncode != 0:
                raise subprocess.CalledProcessError(result.returncode, result.args, result.stdout, result.stderr)
            
            # Track in session coordination
            self.register_agent(agent_type, session_id, str(workspace))
            
            return str(workspace)
            
        except subprocess.CalledProcessError as e:
            print(f"⚠️  Git worktree creation failed: {e.stderr if e.stderr else str(e)}", file=sys.stderr)
            
            # Try without -b flag in case branch exists
            try:
                result = run_git_command([
                    "worktree", "add", str(workspace), branch
                ])
                if result.returncode != 0:
                    raise subprocess.CalledProcessError(result.returncode, result.args)
                
                print(f"✅ Git worktree created (existing branch): {workspace}", file=sys.stderr)
                self.register_agent(agent_type, session_id, str(workspace))
                return str(workspace)
                
            except subprocess.CalledProcessError as e2:
                print(f"⚠️  Git worktree fallback failed: {e2.stderr if e2.stderr else str(e2)}", file=sys.stderr)
                
                # Try with unique naming
                try:
                    timestamp = int(time.time())
                    workspace = self.worktrees_dir / f"{agent_type}-{session_id}-{timestamp}"
                    branch = f"agent/session-{session_id}/{agent_type}-{timestamp}"
                    
                    result = run_git_command([
                        "worktree", "add", "-b", branch, str(workspace)
                    ])
                    if result.returncode != 0:
                        raise subprocess.CalledProcessError(result.returncode, result.args)
                    
                    print(f"✅ Git worktree created (unique): {workspace}", file=sys.stderr)
                    self.register_agent(agent_type, session_id, str(workspace))
                    return str(workspace)
                    
                except subprocess.CalledProcessError as e3:
                    print(f"🚨 All git worktree methods failed, creating simple directory: {e3.stderr if e3.stderr else str(e3)}", file=sys.stderr)
                    
                    # Final fallback: create regular directory without git
                    timestamp = int(time.time())
                    workspace = self.worktrees_dir / f"{agent_type}-{session_id}-{timestamp}-fallback"
                    workspace.mkdir(parents=True, exist_ok=True)
                    
                    print(f"📁 Fallback workspace created (no git isolation): {workspace}", file=sys.stderr)
                    print(f"⚠️  Warning: Agent will not have git isolation - be careful with file operations!", file=sys.stderr)
                    
                    self.register_agent(agent_type, session_id, str(workspace))
                    return str(workspace)
    
    def register_agent(self, agent_type: str, session_id: str, workspace: str):
        """Legacy method - replaced by atomic directory operations"""
        # This method is deprecated - modern code uses state_manager.transition_to_active()
        pass
    
    def check_file_lock(self, file_path: str, session_id: str, requesting_agent: str = "") -> Optional[Dict]:
        """Check if file is locked by another agent using atomic directory operations"""
        lock_manager = self._get_lock_manager(session_id)
        
        if lock_manager.is_locked(file_path, requesting_agent):
            return lock_manager.get_lock_info(file_path)
        
        return None
    
    def acquire_file_lock(self, file_path: str, agent_id: str, session_id: str, operation: str, timeout: float = 5.0) -> bool:
        """Acquire file lock using atomic directory operations"""
        lock_manager = self._get_lock_manager(session_id)
        return lock_manager.acquire_lock(agent_id, file_path, operation, timeout)
    
    def release_file_lock(self, file_path: str, agent_id: str, session_id: str) -> bool:
        """Release file lock using atomic directory operations"""
        lock_manager = self._get_lock_manager(session_id)
        return lock_manager.release_lock(agent_id, file_path)
    
    def update_progress(self, agent_type: str, session_id: str, operation: str, details: Optional[Dict] = None):
        """Update agent progress tracking using JSONL append-only logs"""
        # Modern progress tracking via lifecycle events in state manager
        state_manager = self._get_state_manager(session_id)
        state_manager._log_lifecycle_event("progress_update", f"unknown-{agent_type}", agent_type, {
            "operation": operation,
            "details": details or {}
        })
    
    def cleanup_completed_worktrees(self):
        """Remove completed worktrees that have no uncommitted changes"""
        if not self.worktrees_dir.exists():
            return
        
        for worktree in self.worktrees_dir.iterdir():
            if worktree.is_dir():
                try:
                    # Check if worktree has uncommitted changes
                    result = run_git_command(
                        ["diff", "--quiet"], cwd=worktree
                    )
                    
                    # Also check staged changes
                    staged_result = run_git_command(
                        ["diff", "--cached", "--quiet"], cwd=worktree
                    )
                    
                    # If no uncommitted changes, safe to remove
                    if result.returncode == 0 and staged_result.returncode == 0:
                        run_git_command(["worktree", "remove", str(worktree)])
                        
                except subprocess.CalledProcessError:
                    # Skip worktrees we can't check
                    continue
    
    def get_session_status(self, session_id: str) -> Optional[Dict]:
        """Get current session status and agent information"""
        session_dir = self.sessions_dir / session_id
        if not session_dir.exists():
            return None
        
        status = {}
        
        # Load session data
        session_file = session_dir / "session.json"
        if session_file.exists():
            try:
                with open(session_file) as f:
                    status['session'] = json.load(f)
            except (json.JSONDecodeError, FileNotFoundError):
                status['session'] = {}
        
        # Modern atomic directory-based state
        state_manager = self._get_state_manager(session_id)
        status['state'] = state_manager.get_state_summary()
        status['pending_agents'] = state_manager.get_pending_agents()
        status['active_agents'] = state_manager.get_active_agents()
        
        return status


# Utility functions for hooks
def get_backend() -> MAOSBackend:
    """Get MAOS backend instance"""
    return MAOSBackend()


def extract_file_path_from_tool_input(tool_input: Dict) -> Optional[str]:
    """Extract file path from various tool inputs"""
    # Direct file_path parameter
    if 'file_path' in tool_input:
        return tool_input['file_path']
    
    # MultiEdit edits (take first one)
    if 'edits' in tool_input and tool_input['edits']:
        return tool_input.get('file_path')
    
    return None


# extract_agent_id_from_environment() function removed - replaced with hook context matching
# to eliminate race conditions in multi-agent environments


if __name__ == '__main__':
    # CLI for testing backend utilities
    import sys
    
    if len(sys.argv) < 2:
        print("Usage: python maos_backend.py <command> [args...]")
        print("Commands:")
        print("  status [session_id]  - Show session status")
        print("  cleanup              - Clean up completed worktrees")
        print("  test-workspace <agent_type> - Test workspace creation")
        sys.exit(1)
    
    backend = MAOSBackend()
    command = sys.argv[1]
    
    if command == "status":
        session_id = sys.argv[2] if len(sys.argv) > 2 else None
        if not session_id:
            # Get active session
            active_file = MAOS_DIR / "active_session.json"
            if active_file.exists():
                with open(active_file) as f:
                    session_id = json.load(f).get('session_id')
        
        if session_id:
            status = backend.get_session_status(session_id)
            if status:
                print(json.dumps(status, indent=2))
            else:
                print(f"Session {session_id} not found")
        else:
            print("No active session found")
    
    elif command == "cleanup":
        backend.cleanup_completed_worktrees()
        print("Cleanup completed")
    
    elif command == "test-workspace":
        if len(sys.argv) < 3:
            print("Usage: python maos_backend.py test-workspace <agent_type>")
            sys.exit(1)
        
        agent_type = sys.argv[2]
        session_id = backend.get_or_create_session()
        workspace = backend.prepare_workspace(agent_type, session_id)
        print(f"Created workspace: {workspace}")
        print(f"Session: {session_id}")
    
    else:
        print(f"Unknown command: {command}")
        sys.exit(1)
