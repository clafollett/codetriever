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
from pathlib import Path

try:
    from dotenv import load_dotenv
    load_dotenv()
except ImportError:
    pass  # dotenv is optional


# Add path resolution for proper imports
sys.path.insert(0, str(Path(__file__).parent.parent))
from utils.path_utils import PROJECT_ROOT, LOGS_DIR, TTS_DIR
from utils.config import is_completion_tts_enabled, get_active_tts_provider
from utils.async_logging import log_hook_data_sync


def get_tts_script_path():
    """
    Determine which TTS script to use based on config.json provider setting.
    Environment variables are only used for authentication, NOT provider selection.
    """
    # Use config.json to determine provider (canonical authority)
    provider = get_active_tts_provider()
    
    # Map provider to script file using TTS_DIR constant
    provider_scripts = {
        'elevenlabs': TTS_DIR / "elevenlabs.py",
        'openai': TTS_DIR / "openai.py", 
        'macos': TTS_DIR / "macos.py",
        'pyttsx3': TTS_DIR / "pyttsx3.py"
    }
    
    script_path = provider_scripts.get(provider)
    if script_path and script_path.exists():
        return str(script_path)
    
    # Fallback to macos if configured provider not available
    fallback_script = TTS_DIR / "macos.py" 
    if fallback_script.exists():
        return str(fallback_script)
        
    return None


def announce_subagent_completion():
    """Announce subagent completion using the best available TTS service."""
    try:
        # First check if completion TTS is enabled in config
        if not is_completion_tts_enabled():
            return  # TTS disabled via config
            
        tts_script = get_tts_script_path()
        if not tts_script:
            return  # No TTS scripts available
        
        # Use fixed message for subagent completion
        completion_message = "Subagent Complete"
        
        # Call the TTS script with the completion message
        subprocess.run([
            "uv", "run", tts_script, completion_message
        ], 
        capture_output=True,  # Suppress output
        timeout=10  # 10-second timeout
        )
        
    except (subprocess.TimeoutExpired, subprocess.SubprocessError, FileNotFoundError):
        # Fail silently if TTS encounters issues
        pass
    except Exception:
        # Fail silently for any other errors
        pass


def main():
    try:
        # Parse command line arguments
        parser = argparse.ArgumentParser()
        parser.add_argument('--chat', action='store_true', help='Copy transcript to chat.json')
        args = parser.parse_args()
        
        # Read JSON input from stdin
        input_data = json.load(sys.stdin)
        
        # Validate Claude Code provided required fields
        if 'session_id' not in input_data:
            print(f"❌ WARNING: Claude Code did not provide session_id!", file=sys.stderr)
            # Don't exit - subagent stop hooks should still work
            
        # Use unified async logger for subagent stop events
        log_path = LOGS_DIR / "subagent_stop.jsonl"
        log_hook_data_sync(log_path, input_data)
        
        # Handle --chat switch (same as stop.py)
        if args.chat and 'transcript_path' in input_data:
            transcript_path = input_data['transcript_path']
            if os.path.exists(transcript_path):
                # Read .jsonl file and convert to JSON array
                chat_data = []
                try:
                    with open(transcript_path, 'r') as f:
                        for line in f:
                            line = line.strip()
                            if line:
                                try:
                                    chat_data.append(json.loads(line))
                                except json.JSONDecodeError:
                                    pass  # Skip invalid lines
                    
                    # Write to logs/chat.jsonl using unified async logger
                    chat_file = LOGS_DIR / 'chat.jsonl'
                    for entry in chat_data:
                        log_hook_data_sync(chat_file, entry)
                except Exception:
                    pass  # Fail silently

        # Announce subagent completion via TTS
        announce_subagent_completion()

        sys.exit(0)

    except json.JSONDecodeError:
        # Handle JSON decode errors gracefully
        sys.exit(0)
    except Exception:
        # Handle any other errors gracefully
        sys.exit(0)


if __name__ == "__main__":
    main()