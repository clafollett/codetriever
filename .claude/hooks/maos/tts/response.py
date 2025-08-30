#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.8"
# dependencies = [
#     "elevenlabs",
#     "python-dotenv",
# ]
# ///

import os
import sys
import subprocess
from pathlib import Path

from dotenv import load_dotenv

# Bootstrap path resolution to find our utils
sys.path.insert(0, str(Path(__file__).parent.parent))  # Get to maos directory
from utils.path_utils import setup_maos_imports
setup_maos_imports()

# Constants for fallback values
DEFAULT_VOICE_ID = "FNMROvc7ZdHldafWFMqC"
DEFAULT_MODEL_ID = "eleven_turbo_v2_5"
DEFAULT_OUTPUT_FORMAT = "mp3_44100_128"

# Master config system - single source of truth (used by both main and fallback)
def _get_master_config():
    """Master configuration object - single source of truth for all defaults."""
    import platform
    
    is_macos = platform.system() == "Darwin"
    default_provider = "macos" if is_macos else "pyttsx3"
    default_macos_voice = "Alex"  # Alex is the default macOS system voice
    
    return {
        "tts": {
            "enabled": True,
            "provider": default_provider,
            "text_length_limit": 2000,
            "timeout": 120,
            "voices": {
                "macos": {
                    "voice": default_macos_voice,
                    "rate": 190,
                    "quality": 127
                },
                "pyttsx3": {
                    "voice": "default",
                    "rate": 190,
                    "volume": 0.9
                },
                "elevenlabs": {
                    "voice_id": "IKne3meq5aSn9XLyUdCD",
                    "model": "eleven_turbo_v2_5",
                    "output_format": "mp3_44100_128"
                }
            },
            "responses": {
                "enabled": True
            },
            "completion": {
                "enabled": False
            },
            "notifications": {
                "enabled": True
            }
        },
        "engineer": {
            "name": ""
        }
    }

def _load_config_fallback():
    """Load config using centralized config management."""
    # Use the centralized config loading from utils/config.py
    # This avoids duplicating the path resolution logic
    try:
        from utils.config import load_config
        return load_config()
    except ImportError:
        # Fallback if config module not available
        return _get_master_config()

# Fallback function implementations using master config
def _get_active_tts_provider_fallback():
    config = _load_config_fallback()
    return config['tts']['provider']

def _get_elevenlabs_config_fallback():
    config = _load_config_fallback()
    return config['tts']['voices']['elevenlabs']

def _get_macos_config_fallback():
    config = _load_config_fallback()
    return config['tts']['voices']['macos']

def _get_tts_timeout_fallback():
    config = _load_config_fallback()
    return config['tts']['timeout']

def _clean_text_for_speech_fallback(text):
    config = _load_config_fallback()
    limit = config['tts']['text_length_limit']
    return text[:limit] if len(text) > limit else text

try:
    from utils.config import get_active_tts_provider, get_elevenlabs_config, get_macos_config, get_tts_timeout
    from utils.text_utils import clean_text_for_speech
except ImportError as e:
    if "config" in str(e):
        print(f"❌ Config module import error: {e}", file=sys.stderr)
        print("Using fallback configuration", file=sys.stderr)
        # Assign fallback functions - they use the same master config logic!
        get_active_tts_provider = _get_active_tts_provider_fallback
        get_elevenlabs_config = _get_elevenlabs_config_fallback
        get_macos_config = _get_macos_config_fallback
        get_tts_timeout = _get_tts_timeout_fallback
        clean_text_for_speech = _clean_text_for_speech_fallback
    else:
        raise

def speak_with_native_macos(text):
    """Speak text using native macOS TTS with enhanced quality settings."""
    # Get timeout outside try block to ensure it's available in except blocks
    timeout = get_tts_timeout()
    
    try:
        # Get macOS config with quality settings
        macos_config = get_macos_config()
        voice = macos_config['voice']
        rate = macos_config['rate']
        quality = macos_config['quality']
        
        # Clean text for speech
        clean_text = clean_text_for_speech(text)
        
        if not clean_text:
            return False
        
        print(f"🎙️  {voice} speaking (rate:{rate}, quality:{quality}): {clean_text[:100]}...")
        
        # Use macOS say command with quality settings
        result = subprocess.run([
            "say", 
            "-v", voice,
            "-r", str(rate),
            "--quality", str(quality),
            clean_text
        ], 
        capture_output=True,
        text=True,
        timeout=timeout
        )
        
        if result.returncode == 0:
            print(f"✅ {voice} has spoken!")
            return True
        else:
            error_msg = result.stderr.strip() if result.stderr else "Unknown subprocess error"
            print(f"❌ macOS TTS subprocess error: {error_msg}", file=sys.stderr)
            return False
        
    except subprocess.TimeoutExpired:
        print(f"❌ macOS TTS timeout after {timeout} seconds", file=sys.stderr)
        return False
    except FileNotFoundError:
        print("❌ macOS 'say' command not found", file=sys.stderr)
        return False
    except OSError as e:
        print(f"❌ macOS TTS OS error: {e}", file=sys.stderr)
        return False
    except Exception as e:
        print(f"❌ Unexpected macOS TTS error: {e}", file=sys.stderr)
        return False

def speak_response(text):
    """Speak text using configured TTS provider with consolidated logic."""
    load_dotenv()
    
    # Determine active TTS provider based on configuration and environment
    provider = get_active_tts_provider()
    
    if provider == 'macos':
        return speak_with_native_macos(text)
    
    # Handle ElevenLabs provider (get_active_tts_provider already checked API key availability)
    try:
        from elevenlabs.client import ElevenLabs
        from elevenlabs import play
        
        # Initialize client
        api_key = os.getenv('ELEVENLABS_API_KEY')
        elevenlabs = ElevenLabs(api_key=api_key)
        
        # Clean text for speech
        clean_text = clean_text_for_speech(text)
        
        if not clean_text:
            print("❌ No speakable content found", file=sys.stderr)
            return False
        
        # Get ElevenLabs settings from config
        elevenlabs_config = get_elevenlabs_config()
        
        print(f"🎙️  ElevenLabs speaking: {clean_text[:100]}...")
        
        # Generate and play audio using config settings
        audio = elevenlabs.text_to_speech.convert(
            text=clean_text,
            voice_id=elevenlabs_config['voice_id'],
            model_id=elevenlabs_config['model'],
            output_format=elevenlabs_config['output_format'],
        )
        
        play(audio)
        print("✅ ElevenLabs TTS completed!")
        return True
        
    except ImportError as e:
        if "elevenlabs" in str(e):
            print(f"❌ ElevenLabs package not available: {e}", file=sys.stderr)
            print("🔄 Falling back to macOS TTS", file=sys.stderr)
            return speak_with_native_macos(text)
        else:
            # Re-raise if it's not an ElevenLabs import issue
            raise
    except Exception as e:
        print(f"❌ ElevenLabs TTS error: {e}", file=sys.stderr)
        print("🔄 Falling back to macOS TTS", file=sys.stderr)
        return speak_with_native_macos(text)

def main():
    """Command line interface for response TTS."""
    if len(sys.argv) > 1:
        text = " ".join(sys.argv[1:])
        success = speak_response(text)
        sys.exit(0 if success else 1)
    else:
        print("Usage: ./response_tts.py 'text to speak'")
        sys.exit(1)

if __name__ == "__main__":
    main()