"""
MAOS Directory-Based File Locking System

Atomic file locking using directory operations for zero race conditions.
Each lock is a directory with agent metadata - atomic creation/deletion ensures consistency.
"""

import json
import hashlib
import time
from pathlib import Path
from datetime import datetime, timedelta
from typing import Dict, Optional, List, Any
from .async_logging import log_hook_data


class MAOSFileLockManager:
    """Directory-based atomic file locking for multi-agent coordination"""
    
    def __init__(self, session_id: str, session_dir: Optional[Path] = None):
        self.session_id = session_id
        
        # Always use project root for .maos directory
        if session_dir:
            self.session_path = Path(session_dir)
        else:
            # Get project root using git or fallback
            project_root = self._get_project_root()
            self.session_path = project_root / ".maos" / "sessions" / session_id
            
        # Atomic locking directories
        self.locks_dir = self.session_path / "file_locks"
        self.locks_dir.mkdir(parents=True, exist_ok=True)
    
    def _get_project_root(self) -> Path:
        """Get project root using git or current working directory"""
        try:
            import subprocess
            root = subprocess.check_output(
                ['git', 'rev-parse', '--show-toplevel'],
                stderr=subprocess.DEVNULL,
                text=True
            ).strip()
            return Path(root)
        except Exception:
            # Fall back to current working directory
            return Path.cwd()
    
    def _hash_path_to_lock_key(self, file_path: str) -> str:
        """Convert file path to lock key using SHA-256 hash (safe for filesystem, guaranteed unique)"""
        # Use SHA-256 hash to guarantee uniqueness and prevent path traversal attacks
        hash_obj = hashlib.sha256(file_path.encode('utf-8'))
        safe_key = hash_obj.hexdigest()
        return safe_key
    
    def acquire_lock(self, agent_id: str, file_path: str, operation: str, timeout_seconds: float = 5.0) -> bool:
        """
        Attempt to acquire lock on file path.
        
        Returns True if lock acquired, False if timeout or conflict.
        Uses atomic directory creation - no race conditions possible.
        """
        lock_key = self._hash_path_to_lock_key(file_path)
        lock_dir = self.locks_dir / f"{lock_key}.lock"
        
        start_time = time.time()
        
        while (time.time() - start_time) < timeout_seconds:
            try:
                # Atomic lock acquisition using directory creation
                lock_dir.mkdir(exist_ok=False)  # Fails if directory exists
                
                # Lock acquired successfully - write metadata
                lock_metadata = {
                    "agent_id": agent_id,
                    "file_path": file_path,
                    "operation": operation,
                    "acquired_at": datetime.utcnow().isoformat(),
                    "session_id": self.session_id
                }
                
                with open(lock_dir / "metadata.json", 'w') as f:
                    json.dump(lock_metadata, f, indent=2)
                
                # Log successful lock acquisition
                self._log_lock_event("lock_acquired", agent_id, file_path, {
                    "operation": operation,
                    "timeout_seconds": timeout_seconds
                })
                
                return True
                
            except FileExistsError:
                # Lock held by another agent - check if it's stale
                if self._is_stale_lock(lock_dir):
                    # Clean up stale lock and retry
                    self._force_release_lock(file_path, "stale_lock_cleanup")
                    continue
                
                # Valid lock exists - wait and retry
                time.sleep(0.01)  # 10ms backoff
                continue
        
        # Timeout reached
        self._log_lock_event("lock_timeout", agent_id, file_path, {
            "operation": operation,
            "timeout_seconds": timeout_seconds
        })
        return False
    
    def release_lock(self, agent_id: str, file_path: str) -> bool:
        """
        Release lock on file path.
        
        Returns True if lock released, False if lock not owned by agent.
        """
        lock_key = self._hash_path_to_lock_key(file_path)
        lock_dir = self.locks_dir / f"{lock_key}.lock"
        
        if not lock_dir.exists():
            # No lock exists
            return True
        
        try:
            # Check lock ownership
            metadata_file = lock_dir / "metadata.json"
            if metadata_file.exists():
                with open(metadata_file, 'r') as f:
                    lock_metadata = json.load(f)
                
                if lock_metadata.get("agent_id") != agent_id:
                    # Lock not owned by this agent
                    return False
            
            # Remove lock directory atomically
            self._remove_lock_directory(lock_dir)
            
            # Log successful release
            self._log_lock_event("lock_released", agent_id, file_path, {})
            return True
            
        except Exception as e:
            # Log error but don't fail
            self._log_lock_event("lock_release_error", agent_id, file_path, {
                "error": str(e)
            })
            return False
    
    def _force_release_lock(self, file_path: str, reason: str):
        """Force release a lock (for cleanup/recovery)"""
        lock_key = self._hash_path_to_lock_key(file_path)
        lock_dir = self.locks_dir / f"{lock_key}.lock"
        
        if lock_dir.exists():
            self._remove_lock_directory(lock_dir)
            self._log_lock_event("lock_force_released", "system", file_path, {
                "reason": reason
            })
    
    def _remove_lock_directory(self, lock_dir: Path):
        """Safely remove lock directory and contents"""
        try:
            # Remove metadata file first
            metadata_file = lock_dir / "metadata.json"
            if metadata_file.exists():
                metadata_file.unlink()
            
            # Remove directory
            lock_dir.rmdir()
            
        except Exception:
            # If removal fails, try harder cleanup
            import shutil
            shutil.rmtree(lock_dir, ignore_errors=True)
    
    def _is_stale_lock(self, lock_dir: Path, max_age_minutes: int = 30) -> bool:
        """Check if lock is stale (too old)"""
        try:
            metadata_file = lock_dir / "metadata.json"
            if not metadata_file.exists():
                return True  # Lock without metadata is stale
            
            with open(metadata_file, 'r') as f:
                lock_metadata = json.load(f)
            
            acquired_at = datetime.fromisoformat(lock_metadata.get("acquired_at", ""))
            age = datetime.utcnow() - acquired_at
            
            return age > timedelta(minutes=max_age_minutes)
            
        except Exception:
            return True  # Malformed lock is stale
    
    def is_locked(self, file_path: str, requesting_agent: str) -> bool:
        """Check if file is locked by another agent"""
        lock_key = self._hash_path_to_lock_key(file_path)
        lock_dir = self.locks_dir / f"{lock_key}.lock"
        
        if not lock_dir.exists():
            return False
        
        try:
            metadata_file = lock_dir / "metadata.json"
            if not metadata_file.exists():
                return False
            
            with open(metadata_file, 'r') as f:
                lock_metadata = json.load(f)
            
            lock_owner = lock_metadata.get("agent_id")
            
            # Not locked if same agent owns it
            return lock_owner != requesting_agent
            
        except Exception:
            # Malformed lock - consider it not locked
            return False
    
    def get_lock_info(self, file_path: str) -> Optional[Dict[str, Any]]:
        """Get information about current lock on file"""
        lock_key = self._hash_path_to_lock_key(file_path)
        lock_dir = self.locks_dir / f"{lock_key}.lock"
        
        if not lock_dir.exists():
            return None
        
        try:
            metadata_file = lock_dir / "metadata.json"
            if not metadata_file.exists():
                return None
            
            with open(metadata_file, 'r') as f:
                return json.load(f)
                
        except Exception:
            return None
    
    def release_all_agent_locks(self, agent_id: str) -> List[str]:
        """Release all locks held by an agent (for cleanup)"""
        released_files = []
        
        for lock_dir in self.locks_dir.glob("*.lock"):
            try:
                metadata_file = lock_dir / "metadata.json"
                if not metadata_file.exists():
                    continue
                
                with open(metadata_file, 'r') as f:
                    lock_metadata = json.load(f)
                
                if lock_metadata.get("agent_id") == agent_id:
                    file_path = lock_metadata.get("file_path", "unknown")
                    self._remove_lock_directory(lock_dir)
                    released_files.append(file_path)
                    
            except Exception:
                continue
        
        if released_files:
            self._log_lock_event("agent_locks_released", agent_id, "", {
                "released_files": released_files,
                "count": len(released_files)
            })
        
        return released_files
    
    def cleanup_stale_locks(self) -> int:
        """Clean up all stale locks in session"""
        cleaned_count = 0
        
        # Create list first to avoid modifying directory during iteration
        lock_dirs = list(self.locks_dir.glob("*.lock"))
        
        for lock_dir in lock_dirs:
            if self._is_stale_lock(lock_dir):
                try:
                    metadata_file = lock_dir / "metadata.json"
                    file_path = "unknown"
                    
                    if metadata_file.exists():
                        with open(metadata_file, 'r') as f:
                            lock_metadata = json.load(f)
                        file_path = lock_metadata.get("file_path", "unknown")
                    
                    self._remove_lock_directory(lock_dir)
                    cleaned_count += 1
                    
                    self._log_lock_event("stale_lock_cleaned", "system", file_path, {})
                    
                except Exception:
                    continue
        
        if cleaned_count > 0:
            self._log_lock_event("stale_locks_cleanup", "system", "", {
                "cleaned_count": cleaned_count
            })
        
        return cleaned_count
    
    def _log_lock_event(self, event_type: str, agent_id: str, file_path: str, details: Dict[str, Any]):
        """Log lock events to lifecycle log"""
        log_data = {
            "event_type": event_type,
            "agent_id": agent_id,
            "file_path": file_path,
            "session_id": self.session_id,
            "details": details
        }
        
        # Use unified async logging system
        try:
            import asyncio
            loop = asyncio.get_event_loop()
            if loop.is_running():
                loop.create_task(log_hook_data(self.session_path / "file_locks.jsonl", log_data))
            else:
                asyncio.run(log_hook_data(self.session_path / "file_locks.jsonl", log_data))
        except Exception:
            # Fallback to synchronous logging
            import json
            with open(self.session_path / "file_locks.jsonl", 'a') as f:
                json.dump({**log_data, "timestamp": datetime.utcnow().isoformat()}, f)
                f.write('\n')
    
    def get_all_locks(self) -> List[Dict[str, Any]]:
        """Get information about all current locks"""
        locks = []
        
        for lock_dir in self.locks_dir.glob("*.lock"):
            try:
                metadata_file = lock_dir / "metadata.json"
                if metadata_file.exists():
                    with open(metadata_file, 'r') as f:
                        lock_metadata = json.load(f)
                    locks.append(lock_metadata)
            except Exception:
                continue
        
        return locks