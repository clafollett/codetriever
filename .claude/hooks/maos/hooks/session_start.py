#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "python-dotenv",
# ]
# ///

import json
import sys
from pathlib import Path
from datetime import datetime

try:
    from dotenv import load_dotenv
    load_dotenv()
except ImportError:
    pass  # dotenv is optional

# Add path resolution for proper imports
sys.path.insert(0, str(Path(__file__).parent.parent))
from utils.path_utils import LOGS_DIR, MAOS_DIR
from utils.async_logging import log_hook_data_sync

def main():
    """Handle session start event (new or resumed session)"""
    try:
        # Read JSON input from stdin
        input_data = json.load(sys.stdin)
        
        # Get session ID
        session_id = input_data.get('session_id', 'unknown')
        is_resumed = input_data.get('is_resumed', False)
        
        # Ensure directories exist
        LOGS_DIR.mkdir(parents=True, exist_ok=True)
        MAOS_DIR.mkdir(parents=True, exist_ok=True)
        
        # Create session directory
        session_dir = MAOS_DIR / 'sessions' / session_id
        session_dir.mkdir(parents=True, exist_ok=True)
        
        # Log session start with enhanced data
        log_data = {
            'timestamp': datetime.now().isoformat(),
            'event_type': 'session_start',
            'is_resumed': is_resumed,
            **input_data
        }
        
        log_path = LOGS_DIR / "session_start.jsonl"
        log_hook_data_sync(log_path, log_data)
        
        if is_resumed:
            print(f"Session {session_id} resumed", file=sys.stderr)
        else:
            print(f"Session {session_id} initialized", file=sys.stderr)
        
        sys.exit(0)
        
    except json.JSONDecodeError:
        sys.exit(0)  # Graceful exit on bad JSON
    except Exception:
        sys.exit(0)  # Graceful exit on any error

if __name__ == "__main__":
    main()