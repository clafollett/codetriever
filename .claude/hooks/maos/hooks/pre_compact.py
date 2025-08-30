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

try:
    from dotenv import load_dotenv
    load_dotenv()
except ImportError:
    pass  # dotenv is optional

# Add path resolution for proper imports
sys.path.insert(0, str(Path(__file__).parent.parent))
from utils.path_utils import LOGS_DIR
from utils.async_logging import log_hook_data_sync

def main():
    """Handle pre-compact hook event (before conversation compaction)"""
    try:
        # Read JSON input from stdin
        input_data = json.load(sys.stdin)
        
        # Validate Claude Code provided required fields
        if 'session_id' not in input_data:
            print(f"❌ WARNING: Claude Code did not provide session_id!", file=sys.stderr)
        
        # Log the pre-compact event
        log_path = LOGS_DIR / "pre_compact.jsonl"
        log_hook_data_sync(log_path, input_data)
        
        # Pre-compact might be a good time to:
        # - Clean up old worktrees
        # - Archive session data
        # - Free up resources
        print("Pre-compact hook: Ready for conversation compaction", file=sys.stderr)
        
        sys.exit(0)
        
    except json.JSONDecodeError:
        sys.exit(0)  # Graceful exit on bad JSON
    except Exception:
        sys.exit(0)  # Graceful exit on any error

if __name__ == "__main__":
    main()