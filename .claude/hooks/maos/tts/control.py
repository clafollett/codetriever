#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.8"
# ///

import os
import signal
import subprocess
from pathlib import Path
from typing import List, Optional


class TTSProcessManager:
    """Manages TTS processes for reliable interruption and cleanup."""
    
    def __init__(self):
        """Initialize TTS process manager."""
        self.pid_file = Path("/tmp/maos_tts.pid")
        self.active_processes: List[subprocess.Popen] = []
    
    def start_tts_process(self, command: List[str]) -> Optional[subprocess.Popen]:
        """Start a TTS process with tracking for later interruption.
        
        Args:
            command: Command to execute (e.g., ["say", "-v", "Lee", "Hello"])
            
        Returns:
            Process handle or None if failed
        """
        try:
            # Start process asynchronously so we can track PID
            process = subprocess.Popen(
                command,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
                text=True
            )
            
            # Store PID for later cleanup
            self.pid_file.write_text(str(process.pid))
            self.active_processes.append(process)
            
            print(f"🎙️ Started TTS process: PID {process.pid}")
            return process
            
        except Exception as e:
            print(f"❌ Failed to start TTS process: {e}")
            return None
    
    def wait_for_completion(self, process: subprocess.Popen, timeout: int = 60) -> bool:
        """Wait for TTS process to complete or timeout.
        
        Args:
            process: Process to wait for
            timeout: Maximum time to wait in seconds
            
        Returns:
            True if completed successfully, False if failed/timeout
        """
        try:
            process.wait(timeout=timeout)
            if process.returncode == 0:
                print("✅ TTS completed successfully")
                return True
            else:
                print(f"❌ TTS failed with code {process.returncode}")
                return False
        except subprocess.TimeoutExpired:
            print(f"⏰ TTS timed out after {timeout}s - killing process")
            self.kill_process(process)
            return False
        finally:
            # Clean up from active processes list
            if process in self.active_processes:
                self.active_processes.remove(process)
    
    def kill_process(self, process: subprocess.Popen) -> bool:
        """Kill a specific TTS process.
        
        Args:
            process: Process to kill
            
        Returns:
            True if killed successfully
        """
        try:
            if process.poll() is None:  # Still running
                process.terminate()
                try:
                    process.wait(timeout=2)
                    print(f"🛑 Terminated TTS process: PID {process.pid}")
                except subprocess.TimeoutExpired:
                    # Force kill if terminate didn't work
                    process.kill()
                    process.wait()
                    print(f"💀 Force killed TTS process: PID {process.pid}")
                return True
        except Exception as e:
            print(f"❌ Failed to kill TTS process: {e}")
            return False
        return True
    
    def kill_all_active_processes(self) -> int:
        """Kill all actively tracked TTS processes.
        
        Returns:
            Number of processes killed
        """
        killed_count = 0
        for process in self.active_processes.copy():
            if self.kill_process(process):
                killed_count += 1
                
        self.active_processes.clear()
        return killed_count
    
    def kill_system_tts_processes(self) -> bool:
        """Kill all system TTS processes using pkill.
        
        This is a fallback method to clean up any orphaned TTS processes.
        
        Returns:
            True if pkill commands succeeded
        """
        success = True
        tts_patterns = ["say", "elevenlabs", "openai_tts", "pyttsx3", "macos_tts"]
        
        for pattern in tts_patterns:
            try:
                result = subprocess.run(
                    ["pkill", "-f", pattern],
                    capture_output=True,
                    timeout=5
                )
                if result.returncode == 0:
                    print(f"🛑 Killed system TTS processes matching '{pattern}'")
                # returncode 1 means no processes found, which is fine
            except Exception as e:
                print(f"❌ Failed to kill system TTS pattern '{pattern}': {e}")
                success = False
                
        return success
    
    def emergency_stop_all(self) -> dict:
        """Emergency stop - kill everything TTS-related.
        
        Returns:
            Dictionary with cleanup results
        """
        results = {
            "active_killed": 0,
            "system_killed": False,
            "pid_file_removed": False
        }
        
        print("🚨 Emergency TTS stop initiated...")
        
        # Kill tracked processes
        results["active_killed"] = self.kill_all_active_processes()
        
        # Kill system processes
        results["system_killed"] = self.kill_system_tts_processes()
        
        # Remove PID file
        try:
            if self.pid_file.exists():
                self.pid_file.unlink()
                results["pid_file_removed"] = True
        except Exception as e:
            print(f"❌ Failed to remove PID file: {e}")
        
        print(f"✅ Emergency stop complete: {results}")
        return results
    
    def get_active_pids(self) -> List[int]:
        """Get list of active TTS process PIDs.
        
        Returns:
            List of PIDs for active processes
        """
        pids = []
        for process in self.active_processes:
            if process.poll() is None:  # Still running
                pids.append(process.pid)
        return pids
    
    def cleanup(self):
        """Clean up process manager resources."""
        self.kill_all_active_processes()
        if self.pid_file.exists():
            try:
                self.pid_file.unlink()
            except:
                pass


# Global process manager instance
_tts_manager = TTSProcessManager()


def get_tts_manager() -> TTSProcessManager:
    """Get the global TTS process manager instance."""
    return _tts_manager


def start_tts(command: List[str]) -> Optional[subprocess.Popen]:
    """Convenience function to start a TTS process with tracking."""
    return _tts_manager.start_tts_process(command)


def emergency_stop_tts() -> dict:
    """Convenience function for emergency TTS stop."""
    return _tts_manager.emergency_stop_all()


if __name__ == "__main__":
    # CLI for testing
    import sys
    
    if len(sys.argv) > 1 and sys.argv[1] == "test":
        print("Testing TTS process manager...")
        manager = get_tts_manager()
        
        # Test emergency stop
        results = manager.emergency_stop_all()
        print(f"Test complete: {results}")
    else:
        print("Usage: python tts_control.py [test]")
        print("This is a utility module, import it to use TTSProcessManager")