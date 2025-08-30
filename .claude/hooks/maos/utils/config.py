#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.8"
# ///

import json
from pathlib import Path
from .path_utils import MAOS_HOOKS_DIR

# Constants for config file location
CONFIG_DIR_COMPONENTS = (".claude", "hooks", "maos")
CONFIG_FILENAME = "config.json"

# Cache for config path and loaded config
# Using a sentinel value to distinguish between "not cached" and "cached as None"
_CACHE_SENTINEL = object()
_config_path_cache = _CACHE_SENTINEL
_config_cache = _CACHE_SENTINEL

def get_config_dir():
    """Get the configuration directory path."""
    return Path.cwd().joinpath(*CONFIG_DIR_COMPONENTS)

def get_config_path():
    """Get the path to the Claude hooks configuration file (cached)."""
    global _config_path_cache
    
    # Return cached path if already resolved
    if _config_path_cache is not _CACHE_SENTINEL:
        return _config_path_cache
    
    import subprocess
    
    # Try to find git root first (most reliable for Claude Code)
    try:
        git_root = subprocess.check_output(
            ['git', 'rev-parse', '--show-toplevel'],
            stderr=subprocess.DEVNULL,
            text=True
        ).strip()
        config_path = Path(git_root).joinpath(*CONFIG_DIR_COMPONENTS, CONFIG_FILENAME)
        if config_path.exists():
            _config_path_cache = config_path
            return config_path
    except (subprocess.CalledProcessError, FileNotFoundError):
        pass
    
    # Look for config in .claude directory relative to current working directory
    config_path = Path.cwd().joinpath(*CONFIG_DIR_COMPONENTS, CONFIG_FILENAME)
    if config_path.exists():
        _config_path_cache = config_path
        return config_path
    
    # Fallback to MAOS hooks directory
    config_path = MAOS_HOOKS_DIR / CONFIG_FILENAME
    if config_path.exists():
        _config_path_cache = config_path
        return config_path
    
    # Cache None result too to avoid repeated searches
    _config_path_cache = None
    return None

def load_config(force_reload=False):
    """Load configuration from config.json with fallback to environment variables (cached).
    
    Args:
        force_reload: If True, bypass cache and reload from disk
    """
    global _config_cache
    
    # Return cached config if available and not forcing reload
    if _config_cache is not _CACHE_SENTINEL and not force_reload:
        return _config_cache
    
    # Default to cross-platform provider
    default_provider = "pyttsx3"
    default_macos_voice = "Alex"  # Alex is the default macOS system voice
    
    default_config = {
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
                    # Charlie
                    "voice_id": "IKne3meq5aSn9XLyUdCD",
                    "model": "eleven_turbo_v2_5",
                    "output_format": "mp3_44100_128",
                    "api_key": None  # Optional: falls back to ELEVENLABS_API_KEY env var
                },
                "openai": {
                    "model": "tts-1",
                    "voice": "alloy",
                    "api_key": None  # Optional: falls back to OPENAI_API_KEY env var
                }
            },
            "responses": {
                "enabled": False
            },
            "completion": {
                "enabled": False
            },
            "notifications": {
                "enabled": False
            }
        },
        "engineer": {
            "name": ""
        }
    }
    
    config_path = get_config_path()
    if config_path and config_path.exists():
        try:
            with open(config_path, 'r') as f:
                config = json.load(f)
                # Merge with defaults to handle missing keys
                merged_config = merge_configs(default_config, config)
                _config_cache = merged_config
                return merged_config
        except (json.JSONDecodeError, FileNotFoundError):
            pass
    
    # Return default config if no file found and cache it
    _config_cache = default_config
    return default_config

def merge_configs(default, user):
    """Recursively merge user config with default config."""
    result = default.copy()
    for key, value in user.items():
        if key in result and isinstance(result[key], dict) and isinstance(value, dict):
            result[key] = merge_configs(result[key], value)
        else:
            result[key] = value
    return result

def clear_config_cache():
    """Clear the cached config path and loaded config."""
    global _config_path_cache, _config_cache
    _config_path_cache = _CACHE_SENTINEL
    _config_cache = _CACHE_SENTINEL

def save_config(config):
    """Save configuration to config.json and clear cache."""
    global _config_cache
    
    config_path = get_config_path()
    if not config_path:
        # Create config in .claude directory
        config_dir = Path.cwd().joinpath(*CONFIG_DIR_COMPONENTS)
        config_dir.mkdir(parents=True, exist_ok=True)
        config_path = config_dir / CONFIG_FILENAME
    
    try:
        with open(config_path, 'w') as f:
            json.dump(config, f, indent=2)
        # Clear cache after successful save
        _config_cache = _CACHE_SENTINEL
        return True
    except (OSError, json.JSONDecodeError):
        return False

def get_tts_config():
    """Get TTS-specific configuration."""
    config = load_config()
    return config.get('tts', {})

def is_tts_enabled():
    """Check if TTS master switch is enabled."""
    return get_tts_config().get('enabled', True)

def is_response_tts_enabled():
    """Check if response TTS is enabled (master switch AND responses.enabled)."""
    tts_config = get_tts_config()
    master_enabled = tts_config.get('enabled', True)
    if not master_enabled:
        return False  # Master switch overrides everything
    responses_enabled = tts_config.get('responses', {}).get('enabled', False)
    return responses_enabled

def is_completion_tts_enabled():
    """Check if completion TTS is enabled (master switch AND completion.enabled)."""
    tts_config = get_tts_config()
    master_enabled = tts_config.get('enabled', True)
    if not master_enabled:
        return False  # Master switch overrides everything
    completion_enabled = tts_config.get('completion', {}).get('enabled', True)
    return completion_enabled

def is_notification_tts_enabled():
    """Check if notification TTS is enabled (master switch AND notifications.enabled)."""
    tts_config = get_tts_config()
    master_enabled = tts_config.get('enabled', True)
    if not master_enabled:
        return False  # Master switch overrides everything
    notifications_enabled = tts_config.get('notifications', {}).get('enabled', True)
    return notifications_enabled

def get_tts_provider():
    """Get the preferred TTS provider (pyttsx3, elevenlabs, macos) - applies globally."""
    return get_tts_config().get('provider', 'pyttsx3')

def get_api_key(provider):
    """Get API key for provider using cascading resolution: env vars → config.json.
    
    Args:
        provider: TTS provider name ('elevenlabs', 'openai')
        
    Returns:
        API key string or None if not found
    """
    import os  # Local import to avoid unused import warning
    
    # Environment variable names for each provider
    env_var_map = {
        'elevenlabs': 'ELEVENLABS_API_KEY',
        'openai': 'OPENAI_API_KEY'
    }
    
    # 1. Check environment variable first (highest priority)
    env_var = env_var_map.get(provider)
    if env_var:
        api_key = os.getenv(env_var)
        if api_key:
            return api_key.strip()
    
    # 2. Check config.json as fallback
    voices_config = get_tts_config().get('voices', {})
    provider_config = voices_config.get(provider, {})
    config_api_key = provider_config.get('api_key')
    
    if config_api_key and config_api_key.strip():
        return config_api_key.strip()
        
    return None

def get_active_tts_provider():
    """Get the active TTS provider based on config and API key availability.
    
    Respects user's configured provider but falls back gracefully if API keys unavailable.
    """
    provider = get_tts_provider()
    
    # For API-based providers, verify key availability
    if provider in ['elevenlabs', 'openai']:
        api_key = get_api_key(provider)
        if api_key:
            return provider
        else:
            # Fallback to pyttsx3 if API key not available
            return 'pyttsx3'
    
    # For local providers (macos, pyttsx3), no API key needed
    return provider

def get_text_length_limit():
    """Get the maximum text length for TTS processing."""
    return get_tts_config().get('text_length_limit', 2000)

def get_tts_timeout():
    """Get the timeout in seconds for TTS operations."""
    return get_tts_config().get('timeout', 120)

def get_voice_for_provider(provider):
    """Get the voice setting for a specific provider."""
    voices = get_tts_config().get('voices', {})
    return voices.get(provider, '')

def get_macos_config():
    """Get macOS TTS configuration."""
    voices = get_tts_config().get('voices', {})
    macos_config = voices.get('macos', {})
    
    return {
        'voice': macos_config.get('voice', 'Alex'),
        'rate': macos_config.get('rate', 190),
        'quality': macos_config.get('quality', 127)
    }

def get_elevenlabs_config():
    """Get ElevenLabs configuration with API key resolution.
    
    WARNING: This function returns the actual API key for use by TTS providers.
    Never print or log the return value of this function directly!
    """
    voices = get_tts_config().get('voices', {})
    elevenlabs_config = voices.get('elevenlabs', {})
    
    # Return config with cascading API key resolution
    return {
        'voice_id': elevenlabs_config.get('voice_id', 'FNMROvc7ZdHldafWFMqC'),
        'model': elevenlabs_config.get('model', 'eleven_turbo_v2_5'),
        'output_format': elevenlabs_config.get('output_format', 'mp3_44100_128'),
        'api_key': get_api_key('elevenlabs')  # Cascading resolution
    }

def get_openai_config():
    """Get OpenAI TTS configuration with API key resolution.
    
    WARNING: This function returns the actual API key for use by TTS providers.
    Never print or log the return value of this function directly!
    """
    voices = get_tts_config().get('voices', {})
    openai_config = voices.get('openai', {})
    
    # Return config with cascading API key resolution
    return {
        'model': openai_config.get('model', 'tts-1'),
        'voice': openai_config.get('voice', 'alloy'),
        'api_key': get_api_key('openai')  # Cascading resolution
    }

def mask_api_key(api_key):
    """Safely mask an API key for display purposes.
    
    Args:
        api_key: The API key to mask (can be None)
        
    Returns:
        Masked string safe for display/logging
    """
    if not api_key:
        return 'Not configured'
    return '*' * 8 + api_key[-4:] + ' (masked)'

def mask_sensitive_config(config, sensitive_keys=['api_key']):
    """Mask sensitive fields in a configuration dictionary.
    
    Args:
        config: Dictionary to mask
        sensitive_keys: List of keys to mask (default: ['api_key'])
        
    Returns:
        New dictionary with sensitive fields masked
    """
    safe_config = config.copy()
    for key in sensitive_keys:
        if key in safe_config:
            safe_config[key] = mask_api_key(safe_config[key])
    return safe_config

def get_elevenlabs_config_safe():
    """Get ElevenLabs configuration with API key masked for display."""
    return mask_sensitive_config(get_elevenlabs_config())

def get_openai_config_safe():
    """Get OpenAI configuration with API key masked for display."""
    return mask_sensitive_config(get_openai_config())

def get_engineer_name():
    """Get the engineer name from config."""
    config = load_config()
    return config.get('engineer', {}).get('name', '')

if __name__ == "__main__":
    # CLI for testing config
    import sys
    if len(sys.argv) > 1:
        if sys.argv[1] == "show":
            config = load_config()
            print(json.dumps(config, indent=2))
        elif sys.argv[1] == "tts":
            print(f"TTS enabled: {is_tts_enabled()}")
            print(f"TTS provider (config): {get_tts_provider()}")
            print(f"TTS provider (active): {get_active_tts_provider()}")
            print(f"macOS config: {get_macos_config()}")
            
            # Safe config display with API keys masked
            print(f"ElevenLabs config: {get_elevenlabs_config_safe()}")
            print(f"OpenAI config: {get_openai_config_safe()}")
            
            # Show API key resolution details
            print("\nAPI Key Resolution:")
            for provider in ['elevenlabs', 'openai']:
                api_key = get_api_key(provider)
                if api_key:
                    print(f"  {provider}: {mask_api_key(api_key)} (found)")
                else:
                    print(f"  {provider}: None (not configured)")
        elif sys.argv[1] == "keys":
            print("API Key Status:")
            for provider in ['elevenlabs', 'openai']:
                api_key = get_api_key(provider)
                status = mask_api_key(api_key)
                print(f"  {provider.capitalize()}: {status}")
    else:
        print("Usage: python config.py [show|tts|keys]")
        print("  show - Display full configuration")
        print("  tts  - Display TTS-specific settings and resolution")
        print("  keys - Display API key availability (masked)")