#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.8"
# ///

import sys
import subprocess
from pathlib import Path

# Bootstrap path resolution to find our utils
sys.path.insert(0, str(Path(__file__).parent.parent))  # Get to maos directory
from utils.path_utils import setup_maos_imports
setup_maos_imports()

from utils.text_utils import clean_text_for_speech
from utils.config import get_macos_config, get_tts_timeout
from tts.control import get_tts_manager

def speak_with_macos(text, voice=None, use_process_manager=True):
    """Speak text using native macOS TTS with specified voice.
    
    Args:
        text: Text to speak
        voice: Voice to use (None for config default)
        use_process_manager: Whether to use process manager for interruption (default: True)
        
    Returns:
        bool: True if successful, False otherwise
    """
    
    try:
        # Use provided voice or get from config system
        if voice is None:
            macos_config = get_macos_config()
            voice = macos_config['voice']
        
        # Clean text for speech
        clean_text = clean_text_for_speech(text)
        
        if not clean_text:
            print("❌ No speakable content found", file=sys.stderr)
            return False
        
        print(f"🎙️  {voice} speaking: {clean_text[:100]}...")
        
        # Build say command
        command = ["say", "-v", voice, clean_text]
        
        if use_process_manager:
            # Use process manager for interruptible TTS
            tts_manager = get_tts_manager()
            process = tts_manager.start_tts_process(command)
            
            if process is None:
                return False
                
            # Wait for completion with configured timeout
            timeout = get_tts_timeout()
            success = tts_manager.wait_for_completion(process, timeout=timeout)
            
            if success:
                print(f"✅ {voice} has spoken!")
            return success
            
        else:
            # Legacy synchronous mode (for compatibility)
            result = subprocess.run(
                command,
                capture_output=True,
                text=True
            )
            
            if result.returncode == 0:
                print(f"✅ {voice} has spoken!")
                return True
            else:
                print(f"❌ Error: {result.stderr}", file=sys.stderr)
                return False
        
    except Exception as e:
        print(f"❌ Error: {e}", file=sys.stderr)
        return False

def get_available_voices():
    """Get list of available macOS voices."""
    try:
        result = subprocess.run(["say", "-v", "?"], capture_output=True, text=True)
        if result.returncode == 0:
            voices = []
            for line in result.stdout.strip().split('\n'):
                if line.strip():
                    # Extract voice name (first part before spaces)
                    voice_name = line.split()[0]
                    voices.append(voice_name)
            return voices
        return []
    except Exception:
        return []

def main():
    """Command line interface for macOS TTS."""
    if len(sys.argv) > 1:
        # Check if first arg is a voice name
        available_voices = get_available_voices()
        
        if sys.argv[1] in available_voices and len(sys.argv) > 2:
            # First arg is voice, rest is text
            voice = sys.argv[1]
            text = " ".join(sys.argv[2:])
        else:
            # All args are text, use default voice
            voice = "Lee (Premium)"
            text = " ".join(sys.argv[1:])
        
        success = speak_with_macos(text, voice, use_process_manager=True)
        sys.exit(0 if success else 1)
    else:
        print("Usage: ./macos_tts.py 'text to speak'")
        print("   or: ./macos_tts.py 'VoiceName' 'text to speak'")
        print(f"Available voices: {', '.join(get_available_voices()[:5])}...")
        sys.exit(1)

if __name__ == "__main__":
    main()