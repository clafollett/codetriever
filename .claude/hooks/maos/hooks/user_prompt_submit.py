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
from pathlib import Path

try:
    from dotenv import load_dotenv
    load_dotenv()
except ImportError:
    pass  # dotenv is optional


# Add path resolution for proper imports
sys.path.insert(0, str(Path(__file__).parent.parent))
from utils.path_utils import PROJECT_ROOT, LOGS_DIR
from utils.async_logging import log_hook_data_sync


def log_user_prompt(session_id, input_data):
    """Log user prompt using unified async logger."""
    from datetime import datetime
    try:
        log_file = LOGS_DIR / 'user_prompt_submit.jsonl'
        
        # Enhance Claude Code's input with our timestamp
        log_data = {
            'timestamp': datetime.now().isoformat(),
            **input_data,  # Preserve all Claude Code fields as-is
        }
        
        log_hook_data_sync(log_file, log_data)
    except Exception:
        # Silent failure for logging - don't block user prompt processing
        pass


def validate_prompt(prompt):
    """
    Validate the user prompt for security or policy violations.
    Returns tuple (is_valid, reason).
    """
    # Example validation rules (customize as needed)
    blocked_patterns = [
        # Add any patterns you want to block
        # Example: ('rm -rf /', 'Dangerous command detected'),
    ]
    
    prompt_lower = prompt.lower()
    
    for pattern, reason in blocked_patterns:
        if pattern.lower() in prompt_lower:
            return False, reason
    
    return True, None


def main():
    try:
        # Parse command line arguments
        parser = argparse.ArgumentParser()
        parser.add_argument('--validate', action='store_true', 
                          help='Enable prompt validation')
        parser.add_argument('--log-only', action='store_true',
                          help='Only log prompts, no validation or blocking')
        args = parser.parse_args()
        
        # Read JSON input from stdin
        input_data = json.loads(sys.stdin.read())
        
        # Validate Claude Code provided required fields
        if 'session_id' not in input_data:
            print(f"❌ FATAL: Claude Code did not provide session_id!", file=sys.stderr)
            print(f"Available keys: {list(input_data.keys())}", file=sys.stderr)
            sys.exit(1)
        
        # Extract fields we need
        session_id = input_data['session_id']
        prompt = input_data.get('user_input', '')  # UserPromptSubmit uses 'user_input' field
        
        # Log the user prompt with enhanced data
        log_user_prompt(session_id, input_data)
        
        # Validate prompt if requested and not in log-only mode
        if args.validate and not args.log_only:
            is_valid, reason = validate_prompt(prompt)
            if not is_valid:
                # Exit code 2 blocks the prompt with error message
                print(f"Prompt blocked: {reason}", file=sys.stderr)
                sys.exit(2)
        
        # Add context information (optional)
        # You can print additional context that will be added to the prompt
        # Example: print(f"Current time: {datetime.now()}")
        
        # Success - prompt will be processed
        sys.exit(0)
        
    except json.JSONDecodeError:
        # Handle JSON decode errors gracefully
        sys.exit(0)
    except Exception:
        # Handle any other errors gracefully
        sys.exit(0)


if __name__ == '__main__':
    main()