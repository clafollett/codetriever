#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.8"
# ///

import json
import sys
from pathlib import Path
from typing import Dict, Optional

# Add path resolution for proper imports
sys.path.insert(0, str(Path(__file__).parent.parent))  # Get to maos directory
# Import MAOS backend utilities
try:
    from utils.backend import MAOSBackend, extract_file_path_from_tool_input
    from utils.path_utils import MAOS_DIR
except ImportError:
    # Fallback if backend not available
    MAOSBackend = None
    MAOS_DIR = Path.cwd() / '.maos'


class MAOSPostCoordinator:
    """MAOS post-tool coordination for cleanup and progress tracking"""
    
    def __init__(self, hook_metadata=None):
        self.hook_metadata = hook_metadata or {}
        self.backend = MAOSBackend() if MAOSBackend else None
    
    def get_active_session_id(self):
        """Get active session ID"""
        if not self.backend:
            return None
        
        # Check for active session
        active_file = MAOS_DIR / "active_session.json"
        if active_file.exists():
            try:
                with open(active_file) as f:
                    return json.load(f).get('session_id')
            except (json.JSONDecodeError, KeyError, FileNotFoundError):
                pass
        
        return None
    
    def handle_file_operation_completion(self, tool_name, tool_input, tool_response):
        """Handle completion of file operations"""
        if not self.backend:
            return
        
        file_path = extract_file_path_from_tool_input(tool_input)
        if not file_path:
            return
        
        try:
            session_id = self.get_active_session_id()
            if not session_id:
                return
            
            # Release file lock if this was a write operation
            if tool_name in ["Edit", "Write", "MultiEdit"]:
                agent_id = self.hook_metadata.get('maos_agent_id')
                if agent_id:
                    self.backend.release_file_lock(file_path, agent_id, session_id)
            
            # Update progress with completion status using hook context
            agent_type = self.hook_metadata.get('maos_agent_type')
            if not agent_type:
                # No agent context - this is main session, not sub-agent
                return
                
            success = tool_response.get('success', True) if tool_response else True
            
            details = {
                'file_path': file_path,
                'success': success,
                'completed': True
            }
            
            if not success and tool_response:
                details['error'] = tool_response.get('error', 'Unknown error')
            
            # Use agent_type for progress tracking (consistent with pre_tool_handler)
            self.backend.update_progress(agent_type, session_id, f"{tool_name}_completed", details)
            
        except Exception as e:
            # Non-blocking error
            pass
    
    def handle_task_completion(self, tool_input, tool_response):
        """Handle Task tool completion - potentially agent completion"""
        if not self.backend:
            return
        
        try:
            session_id = self.get_active_session_id()
            if not session_id:
                return
            
            # If this was a sub-agent task, mark it as completed
            if tool_input and tool_input.get('subagent_type'):
                agent_type = tool_input['subagent_type']
                
                details = {
                    'agent_type': agent_type,
                    'completed': True,
                    'success': tool_response.get('success', True) if tool_response else True
                }
                
                self.backend.update_progress(agent_type, session_id, "agent_completed", details)
                
                print(f"📋 MAOS: Agent {agent_type} completed task", file=sys.stderr)
            
            # Trigger cleanup of completed worktrees
            self.cleanup_completed_worktrees()
            
        except Exception as e:
            # Non-blocking error
            pass
    
    def cleanup_completed_worktrees(self):
        """Periodic cleanup of completed worktrees"""
        if not self.backend:
            return
        
        try:
            # Only run cleanup occasionally to avoid overhead
            import random
            if random.random() > 0.1:  # 10% chance of running cleanup
                return
            
            self.backend.cleanup_completed_worktrees()
            
        except Exception as e:
            # Non-blocking error
            pass
    
    def update_session_activity(self, tool_name):
        """Update session activity timestamp"""
        if not self.backend:
            return
        
        try:
            session_id = self.get_active_session_id()
            if not session_id:
                return
            
            # Update session activity
            session_dir = MAOS_DIR / "sessions" / session_id
            if session_dir.exists():
                activity_file = session_dir / "activity.json"
                
                activity_data = {
                    'last_activity': json.loads(json.dumps(Path().cwd().name, default=str)),  # Current timestamp
                    'last_tool': tool_name,
                    'total_operations': 1
                }
                
                # Load existing activity
                if activity_file.exists():
                    try:
                        with open(activity_file) as f:
                            existing = json.load(f)
                            activity_data['total_operations'] = existing.get('total_operations', 0) + 1
                    except (json.JSONDecodeError, FileNotFoundError):
                        pass
                
                # Save updated activity
                from datetime import datetime
                activity_data['last_activity'] = datetime.now().isoformat()
                
                with open(activity_file, 'w') as f:
                    json.dump(activity_data, f, indent=2)
            
        except Exception as e:
            # Non-blocking error
            pass


def handle_maos_post_tool(tool_name: str, tool_input: Dict, tool_response: Dict = None, hook_metadata: Optional[Dict] = None):
    """Main MAOS post-tool processing function"""
    # Initialize MAOS post-coordinator
    coordinator = MAOSPostCoordinator(hook_metadata)
    
    # Handle file operation completion
    if tool_name in ["Edit", "Write", "MultiEdit", "Read"]:
        coordinator.handle_file_operation_completion(tool_name, tool_input, tool_response)
    
    # Handle task completion (potential agent completion)
    elif tool_name == "Task":
        coordinator.handle_task_completion(tool_input, tool_response)
    
    # Update session activity for all operations
    coordinator.update_session_activity(tool_name)
    
    # Periodic cleanup
    coordinator.cleanup_completed_worktrees()