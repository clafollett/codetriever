#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "python-dotenv",
# ]
# ///

import json
import sys
import re
import os
import asyncio
import time
import threading
from pathlib import Path
from typing import Dict, Optional
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime

try:
    from dotenv import load_dotenv
    load_dotenv()
except ImportError:
    pass  # dotenv is optional

# Add path resolution for proper imports
sys.path.insert(0, str(Path(__file__).parent.parent))
from utils.path_utils import PROJECT_ROOT, LOGS_DIR
from utils.async_logging import log_hook_data, log_hook_data_sync, get_task_manager

# Import MAOS handler
try:
    from handlers.pre_tool_handler import handle_maos_pre_tool
except ImportError:
    # Fallback if MAOS not available
    handle_maos_pre_tool = None

def is_dangerous_rm_command(command):
    """
    Comprehensive detection of dangerous rm commands.
    Properly distinguishes between flags and filenames to avoid false positives.
    """
    # Split into tokens to properly identify flags vs filenames
    tokens = command.split()
    
    if not tokens or tokens[0].lower() != 'rm':
        return False
    
    # Remove 'rm' to focus on arguments
    args = tokens[1:]
    
    # Track flags found
    has_recursive = False
    has_force = False
    
    # Check for -- which stops flag processing
    double_dash_index = len(args)
    for i, arg in enumerate(args):
        if arg == '--':
            double_dash_index = i
            break
    
    # Process arguments before --
    for i, arg in enumerate(args[:double_dash_index]):
        arg_lower = arg.lower()
        
        # Long options
        if arg_lower == '--recursive':
            has_recursive = True
        elif arg_lower == '--force':
            has_force = True
        # Short options (must start with single dash and have letters)
        elif arg.startswith('-') and len(arg) > 1 and arg[1] != '-':
            # Check each character in the flag
            for char in arg[1:]:
                if char in 'rR':
                    has_recursive = True
                elif char == 'f':
                    has_force = True
    
    # Check for dangerous combinations
    if has_recursive and has_force:
        return True
    
    # Check for just recursive with dangerous paths
    if has_recursive:
        # Get all non-flag arguments (potential paths)
        paths = []
        for i, arg in enumerate(args):
            # Skip flags and arguments before --
            if i < double_dash_index and arg.startswith('-') and arg != '-':
                continue
            # After --, everything is a path
            if i > double_dash_index:
                paths.append(arg)
            # Before --, non-flag args are paths
            elif not arg.startswith('-') or arg == '-':
                paths.append(arg)
        
        # Check for dangerous paths
        dangerous_patterns = [
            r'^/$',          # Exactly root
            r'^/\*',         # Root with wildcard
            r'^~/?$',        # Home directory
            r'^\$HOME',      # HOME variable
            r'^\.\./?',      # Parent directory
            r'^\*$',         # Just wildcard
            r'^\.$',         # Current directory
        ]
        
        for path in paths:
            for pattern in dangerous_patterns:
                if re.match(pattern, path):
                    return True
    
    return False

def is_env_file_access(tool_name, tool_input):
    """
    Check if any tool is trying to access .env files containing sensitive data.
    """
    if tool_name in ['Read', 'Edit', 'MultiEdit', 'Write', 'Bash']:
        # Check file paths for file-based tools
        if tool_name in ['Read', 'Edit', 'MultiEdit', 'Write']:
            file_path = tool_input.get('file_path', '')
            # Block .env files but allow .env.sample and stack.env
            if '.env' in file_path and not file_path.endswith('.env.sample') and not file_path.endswith('stack.env'):
                return True
        
        # Check bash commands for .env file access
        elif tool_name == 'Bash':
            command = tool_input.get('command', '')
            # Pattern to detect .env file access (but allow .env.sample and stack.env)
            env_patterns = [
                r'(?<!stack)\.env\b(?!\.sample)',  # .env but not .env.sample or stack.env
                r'cat\s+.*(?<!stack)\.env\b(?!\.sample)',  # cat .env
                r'echo\s+.*>\s*(?<!stack)\.env\b(?!\.sample)',  # echo > .env
                r'touch\s+.*(?<!stack)\.env\b(?!\.sample)',  # touch .env
                r'cp\s+.*(?<!stack)\.env\b(?!\.sample)',  # cp .env
                r'mv\s+.*(?<!stack)\.env\b(?!\.sample)',  # mv .env
            ]
            
            for pattern in env_patterns:
                if re.search(pattern, command):
                    return True
    
    return False

async def run_maos_background(tool_name: str, tool_input: Dict, hook_metadata: Dict) -> None:
    """Run MAOS orchestration in background without blocking."""
    if not handle_maos_pre_tool:
        return
    
    def maos_task():
        try:
            handle_maos_pre_tool(tool_name, tool_input, hook_metadata)
        except Exception as e:
            print(f"⚠️  MAOS processing error (background): {e}", file=sys.stderr)
    
    # Run MAOS in background task manager
    task_manager = get_task_manager()
    await task_manager.run_background_task(maos_task, timeout=10.0)


async def main_async():
    """ASYNC main function - security checks first, everything else in background."""
    try:
        start_time = time.time()
        
        # Read JSON input from stdin
        input_data = json.load(sys.stdin)
        
        # Validate Claude Code provided required fields
        if 'session_id' not in input_data:
            print(f"❌ FATAL: Claude Code did not provide session_id!", file=sys.stderr)
            print(f"Available keys: {list(input_data.keys())}", file=sys.stderr)
            sys.exit(1)
        
        # Extract fields we need for MAOS processing
        tool_name = input_data.get('tool_name', '')
        tool_input = input_data.get('tool_input', {})
        hook_metadata = input_data.get('metadata', {})
        
        # 🚨 CRITICAL SECURITY CHECKS FIRST (these can block operations)
        # These must run synchronously to block dangerous operations
        
        # Check for .env file access (blocks access to sensitive environment files)
        if is_env_file_access(tool_name, tool_input):
            print("BLOCKED: Access to .env files containing sensitive data is prohibited", file=sys.stderr)
            print("Use .env.sample for template files instead", file=sys.stderr)
            sys.exit(2)  # Exit code 2 blocks tool call and shows error to Claude
        
        # Check for dangerous rm -rf commands
        if tool_name == 'Bash':
            command = tool_input.get('command', '')
            
            # Block rm -rf commands with comprehensive pattern matching
            if is_dangerous_rm_command(command):
                print("BLOCKED: Dangerous rm command detected and prevented", file=sys.stderr)
                sys.exit(2)  # Exit code 2 blocks tool call and shows error to Claude
        
        security_time = time.time() - start_time
        print(f"🔒 Security checks completed in {security_time*1000:.2f}ms", file=sys.stderr)
        
        # 🚀 EVERYTHING ELSE RUNS IN BACKGROUND (non-blocking)
        
        # Start background tasks in parallel
        background_tasks = []
        
        # MAOS orchestration in background
        if handle_maos_pre_tool:
            maos_task = asyncio.create_task(run_maos_background(tool_name, tool_input, hook_metadata))
            background_tasks.append(maos_task)
        
        # Enhance Claude Code's input with our timestamp and MAOS metadata
        log_data = {
            'timestamp': datetime.now().isoformat(),
            **input_data,  # Preserve all Claude Code fields as-is
            # Add any MAOS-specific metadata here if needed
        }
        
        # Async logging (JSONL format for true append-only)
        log_path = LOGS_DIR / 'pre_tool_use.jsonl'
        logging_task = asyncio.create_task(log_hook_data(log_path, log_data))
        
        # Add immediate fallback sync logging to ensure file is always created
        try:
            log_hook_data_sync(log_path, log_data)
        except Exception:
            pass  # Silent failure
        background_tasks.append(logging_task)
        
        # Give background tasks a moment to start, but don't wait for completion
        if background_tasks:
            # Wait just long enough to ensure tasks are started (non-blocking)
            try:
                await asyncio.wait_for(
                    asyncio.gather(*background_tasks, return_exceptions=True),
                    timeout=0.1  # Very short timeout - just to start tasks
                )
            except asyncio.TimeoutError:
                # This is expected - we don't want to block
                pass
        
        total_time = time.time() - start_time
        print(f"⚡ Pre-tool hook completed in {total_time*1000:.2f}ms (background tasks running)", file=sys.stderr)
        
        sys.exit(0)
        
    except json.JSONDecodeError:
        # Gracefully handle JSON decode errors
        sys.exit(0)
    except Exception as e:
        # Handle any other errors gracefully - don't block operations
        print(f"⚠️  Pre-tool hook error (non-blocking): {e}", file=sys.stderr)
        sys.exit(0)


def main():
    """Clean async-only main function."""
    try:
        asyncio.run(main_async())
    except Exception as e:
        print(f"⚠️  Pre-tool hook error: {e}", file=sys.stderr)
        sys.exit(0)

if __name__ == '__main__':
    main()