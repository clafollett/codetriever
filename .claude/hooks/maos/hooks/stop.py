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
import time
from pathlib import Path

try:
    from dotenv import load_dotenv
    load_dotenv()
except ImportError:
    pass  # dotenv is optional

# Add path resolution for proper imports
sys.path.insert(0, str(Path(__file__).parent.parent))
from utils.config import is_response_tts_enabled, is_completion_tts_enabled, get_engineer_name, get_active_tts_provider
from utils.path_utils import PROJECT_ROOT, LOGS_DIR, TTS_DIR
from utils.async_logging import log_hook_data_sync


def get_completion_messages():
    """Return list of friendly completion messages with engineer name."""
    engineer_name = get_engineer_name()
    name_prefix = f"Hey {engineer_name}! " if engineer_name else ""
    name_suffix = f", {engineer_name}!" if engineer_name else "!"
    
    return [
        f"{name_prefix}All done!",
        f"{name_prefix}We're ready for next task!",
        f"Work complete{name_suffix}",
        f"Task finished{name_suffix}",
        f"Job complete{name_suffix}"
    ]


def get_tts_script_path():
    """Determine which TTS script to use based on configuration."""
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


def fire_completion_tts():
    """Fire completion TTS immediately - no blocking."""
    try:
        # Skip completion announcement if response TTS is enabled
        if is_response_tts_enabled():
            return False
        
        # Check if completion TTS is enabled
        if not is_completion_tts_enabled():
            return False
        
        tts_script = get_tts_script_path()
        if not tts_script:
            return False
        
        # Get random completion message
        completion_messages = get_completion_messages()
        completion_message = random.choice(completion_messages)
        
        # Fire TTS in background - don't wait for completion
        subprocess.Popen([
            "uv", "run", tts_script, completion_message
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        
        return True
        
    except Exception:
        return False


def fire_response_tts(input_data):
    """Fire response TTS if enabled."""
    try:
        if not is_response_tts_enabled():
            return False
            
        transcript_path = input_data.get('transcript_path')
        if not transcript_path or not os.path.exists(transcript_path):
            return False
        
        # Get the latest assistant response from transcript
        latest_response = None
        try:
            with open(transcript_path, 'r') as f:
                for line in f:
                    if line.strip():
                        try:
                            data = json.loads(line.strip())
                            # Handle nested message structure
                            msg = data.get('message', {})
                            if msg.get('role') == 'assistant' and msg.get('content'):
                                content = msg['content']
                                # Handle both string content and array of content blocks
                                if isinstance(content, str):
                                    latest_response = content
                                elif isinstance(content, list) and content:
                                    # Extract text from content blocks
                                    for block in content:
                                        if isinstance(block, dict) and block.get('type') == 'text':
                                            latest_response = block.get('text', '')
                                            break
                        except json.JSONDecodeError:
                            continue
        except Exception:
            return False
        
        if not latest_response:
            return False
        
        # Get response TTS script using TTS_DIR constant
        tts_script = TTS_DIR / "response.py"
        
        if not tts_script.exists():
            return False
        
        # Fire TTS in background - don't wait
        subprocess.Popen([
            "uv", "run", str(tts_script), latest_response
        ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        
        return True
        
    except Exception:
        return False


def copy_transcript_to_chat(input_data):
    """Copy new transcript entries to chat.jsonl in append-only mode."""
    try:
        transcript_path = input_data.get('transcript_path')
        if not transcript_path or not os.path.exists(transcript_path):
            return False
        
        # Get the chat log file
        chat_file = LOGS_DIR / 'chat.jsonl'
        chat_file.parent.mkdir(parents=True, exist_ok=True)
        
        # Instead of reading entire transcript and rewriting everything,
        # just append new entries since last processing
        # For now, append the stop event itself as a chat entry
        chat_entry = {
            'event': 'stop',
            'session_id': input_data.get('session_id'),
            'transcript_path': transcript_path,
            'cwd': input_data.get('cwd', '')
        }
        
        # Use unified async logger (adds timestamp automatically)
        log_hook_data_sync(chat_file, chat_entry)
        
        return True
        
    except Exception:
        return False


def main():
    """Optimized stop hook - TTS first, everything else fire-and-forget."""
    try:
        # Parse arguments
        parser = argparse.ArgumentParser()
        parser.add_argument('--chat', action='store_true', help='Copy transcript to chat.json')
        args = parser.parse_args()
        
        # Read JSON input from stdin
        input_data = json.load(sys.stdin)
        
        # Validate Claude Code provided required fields
        if 'session_id' not in input_data:
            print(f"❌ WARNING: Claude Code did not provide session_id!", file=sys.stderr)
            # Don't exit - stop hooks should still work
        
        # 🚀 FIRE TTS IMMEDIATELY - TOP PRIORITY
        start_time = time.time()
        
        # Fire response TTS or completion TTS (mutually exclusive)
        response_tts_fired = fire_response_tts(input_data)
        completion_tts_fired = False
        
        if not response_tts_fired:
            completion_tts_fired = fire_completion_tts()
        
        tts_time = time.time() - start_time
        if response_tts_fired or completion_tts_fired:
            tts_type = "Response" if response_tts_fired else "Completion"
            print(f"🚀 {tts_type} TTS fired in {tts_time*1000:.2f}ms", file=sys.stderr)
        
        # 📝 BACKGROUND OPERATIONS (fire-and-forget)
        
        # Log to JSONL format with enhanced data
        from datetime import datetime
        log_data = {
            'timestamp': datetime.now().isoformat(),
            **input_data,  # Preserve all Claude Code fields as-is
        }
        
        log_path = LOGS_DIR / "stop.jsonl"
        log_hook_data_sync(log_path, log_data)
        
        # Copy transcript to chat if requested
        if args.chat:
            copy_transcript_to_chat(input_data)
        
        # Exit immediately - don't wait for background operations
        sys.exit(0)
        
    except json.JSONDecodeError:
        sys.exit(0)  # Graceful exit on bad JSON
    except Exception:
        sys.exit(0)  # Graceful exit on any error


if __name__ == '__main__':
    main()