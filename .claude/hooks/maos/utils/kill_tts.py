#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.8"
# ///

"""
TTS Kill Switch - Emergency stop for all TTS processes

Usage:
    python kill_tts.py          # Kill all TTS processes  
    python kill_tts.py --quiet  # Kill quietly without output
    ./kill_tts.py               # Same as python kill_tts.py
    
You can also create an alias for quick access:
    alias stoptts='cd /path/to/maos && python .claude/hooks/utils/kill_tts.py --quiet'
"""

import argparse
import sys
from pathlib import Path

# Add path resolution for proper imports
maos_dir = Path(__file__).parent.parent
sys.path.insert(0, str(maos_dir))
from tts.control import emergency_stop_tts


def main():
    """Main entry point for TTS kill switch."""
    parser = argparse.ArgumentParser(
        description="Emergency stop for all MAOS TTS processes",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )
    
    parser.add_argument(
        '--quiet', '-q',
        action='store_true',
        help='Run quietly without status output'
    )
    
    parser.add_argument(
        '--verbose', '-v',
        action='store_true',
        help='Show detailed output'
    )
    
    args = parser.parse_args()
    
    # Set up output level
    if args.quiet:
        # Redirect stdout to suppress print statements from tts_control
        import io
        import contextlib
        f = io.StringIO()
        with contextlib.redirect_stdout(f):
            results = emergency_stop_tts()
    else:
        if not args.verbose:
            print("🛑 Stopping all TTS processes...")
        
        results = emergency_stop_tts()
        
        if not args.verbose:
            # Show summary
            total_killed = results.get("active_killed", 0)
            system_cleaned = results.get("system_killed", False)
            
            if total_killed > 0 or system_cleaned:
                print(f"✅ TTS stopped ({total_killed} processes killed)")
            else:
                print("ℹ️  No active TTS processes found")
    
    # Exit code based on results
    if results.get("active_killed", 0) > 0 or results.get("system_killed", False):
        sys.exit(0)  # Success - something was killed
    else:
        sys.exit(1)  # Nothing to kill (not really an error, but distinguishable)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\n🚫 Kill operation cancelled")
        sys.exit(130)
    except Exception as e:
        print(f"❌ Error stopping TTS: {e}")
        sys.exit(1)