#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "python-dotenv",
# ]
# ///

import argparse
import json
import os
import sys
import subprocess
import random
from pathlib import Path

try:
    from dotenv import load_dotenv
    load_dotenv()
except ImportError:
    pass  # dotenv is optional



# Add path resolution for proper imports
sys.path.insert(0, str(Path(__file__).parent.parent))
from utils.path_utils import LOGS_DIR, TTS_DIR
from utils.config import is_notification_tts_enabled, get_active_tts_provider
from utils.async_logging import log_hook_data_sync


def get_tts_script_path():
    """
    Determine which TTS script to use based on configuration.
    """
    # Get active provider from config
    provider = get_active_tts_provider()
    
    # Map providers to script paths using TTS_DIR constant
    script_map = {
        "macos": TTS_DIR / "macos.py",
        "elevenlabs": TTS_DIR / "elevenlabs.py", 
        "openai": TTS_DIR / "openai.py",
        "pyttsx3": TTS_DIR / "pyttsx3.py"
    }
    
    tts_script = script_map.get(provider)
    if tts_script and tts_script.exists():
        return str(tts_script)
    
    return None

def fire_tts_notification():
    """Fire TTS notification immediately - no blocking."""
    try:
        if not is_notification_tts_enabled():
            return False
            
        tts_script = get_tts_script_path()
        if not tts_script:
            return False
        
        # Get engineer name if available
        engineer_name = os.getenv('ENGINEER_NAME', '').strip()
        
        # Create notification message with 30% chance to include name
        if engineer_name and random.random() < 0.3:
            notification_message = f"{engineer_name}, your agent needs your input"
        else:
            notification_message = "Your agent needs your input"
        
        # Fire TTS in background - don't wait for completion
        subprocess.Popen([
            "uv", "run", tts_script, notification_message
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        
        return True
        
    except Exception:
        return False


def main():
    """Optimized notification hook - TTS first, logging fire-and-forget."""
    try:
        # Parse arguments
        parser = argparse.ArgumentParser()
        parser.add_argument('--notify', action='store_true', help='Enable TTS notifications')
        args = parser.parse_args()
        
        # Read JSON input from stdin
        input_data = json.loads(sys.stdin.read())
        
        # Validate Claude Code provided required fields
        if 'session_id' not in input_data:
            print(f"❌ WARNING: Claude Code did not provide session_id!", file=sys.stderr)
            # Don't exit - notifications should still work
        
        # 🚀 FIRE TTS IMMEDIATELY - TOP PRIORITY
        # Fire TTS only if --notify flag is set AND not generic waiting message
        if args.notify and input_data.get('message') != 'Claude is waiting for your input':
            fire_tts_notification()
        
        # 📝 LOGGING IN FIRE-AND-FORGET MODE (don't wait)
        # Enhance Claude Code's input with our timestamp
        from datetime import datetime
        log_data = {
            'timestamp': datetime.now().isoformat(),
            **input_data,  # Preserve all Claude Code fields as-is
        }
        
        log_path = LOGS_DIR / "notification.jsonl"
        log_hook_data_sync(log_path, log_data)
        
        # Exit immediately - don't wait for logging to complete
        sys.exit(0)
        
    except json.JSONDecodeError:
        sys.exit(0)  # Graceful exit on bad JSON
    except Exception:
        sys.exit(0)  # Graceful exit on any error


if __name__ == '__main__':
    main()