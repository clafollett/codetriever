#!/usr/bin/env python3
"""
Performance test script for optimized Claude Code hooks.
Tests TTS response times and measures improvements.
"""

import json
import time
import subprocess
from pathlib import Path

def test_hook_performance(hook_script, test_data):
    """Test individual hook performance."""
    print(f"\n🧪 Testing {hook_script.name}...")
    
    start_time = time.time()
    
    try:
        # Run hook with test data
        result = subprocess.run([
            "python3", str(hook_script)
        ], 
        input=json.dumps(test_data),
        text=True,
        capture_output=True,
        timeout=30
        )
        
        end_time = time.time()
        duration = (end_time - start_time) * 1000  # Convert to ms
        
        print(f"✅ {hook_script.name} completed in {duration:.2f}ms")
        print(f"   Return code: {result.returncode}")
        
        # Check for TTS timing info in stderr
        if result.stderr:
            for line in result.stderr.split('\n'):
                if 'TTS fired' in line:
                    print(f"   {line}")
        
        return duration
        
    except subprocess.TimeoutExpired:
        print(f"❌ {hook_script.name} timed out after 30 seconds")
        return 30000
    except Exception as e:
        print(f"❌ {hook_script.name} failed: {e}")
        return -1


def main():
    """Run performance tests on all hooks."""
    print("🚀 Claude Code Hook Performance Test")
    print("=" * 50)
    
    # Get hooks directory
    hooks_dir = Path(__file__).parent
    
    # Test data for stop hook
    stop_test_data = {
        "session_id": "test-session",
        "stop_hook_active": True,
        "transcript_path": "/dev/null",  # Won't exist, but hook should handle gracefully
        "completion_time": time.time(),
        "metadata": {"test": True}
    }
    
    # Test data for notification hook
    notification_test_data = {
        "message": "Test notification message",
        "timestamp": time.time(),
        "metadata": {"test": True}
    }
    
    # Test data for pre/post tool hooks
    tool_test_data = {
        "tool_name": "Read",
        "tool_input": {"file_path": "/dev/null"},
        "tool_response": {"result": "test"},
        "metadata": {"test": True}
    }
    
    # List of hooks to test
    hooks_to_test = [
        ("stop.py", stop_test_data, ["--chat"]),
        ("notification.py", notification_test_data, ["--notify"]),
        ("pre_tool_use.py", tool_test_data, []),
        ("post_tool_use.py", tool_test_data, [])
    ]
    
    results = {}
    
    for hook_name, test_data, args in hooks_to_test:
        hook_path = hooks_dir / hook_name
        if hook_path.exists():
            print(f"\n🔧 Testing {hook_name} with args: {args}")
            
            # Test without args first
            duration = test_hook_performance(hook_path, test_data)
            results[hook_name] = duration
            
            # Test with args if provided
            if args:
                print(f"   Testing with args: {' '.join(args)}")
                start_time = time.time()
                try:
                    cmd = ["python3", str(hook_path)] + args
                    result = subprocess.run(
                        cmd,
                        input=json.dumps(test_data),
                        text=True,
                        capture_output=True,
                        timeout=30
                    )
                    end_time = time.time()
                    duration_with_args = (end_time - start_time) * 1000
                    print(f"   ✅ With args: {duration_with_args:.2f}ms")
                    
                    # Check for TTS timing info in stderr
                    if result.stderr:
                        for line in result.stderr.split('\n'):
                            if 'TTS fired' in line:
                                print(f"      {line}")
                except Exception as e:
                    print(f"   ❌ With args failed: {e}")
        else:
            print(f"⚠️  Hook {hook_name} not found at {hook_path}")
    
    # Print summary
    print("\n" + "=" * 50)
    print("📊 PERFORMANCE SUMMARY")
    print("=" * 50)
    
    for hook_name, duration in results.items():
        if duration > 0:
            status = "🚀 EXCELLENT" if duration < 100 else "✅ GOOD" if duration < 500 else "⚠️  SLOW"
            print(f"{status:<12} {hook_name:<20} {duration:>8.2f}ms")
        else:
            print(f"❌ FAILED     {hook_name:<20}        --")
    
    print("\n🎯 PERFORMANCE TARGETS:")
    print("   🚀 Excellent: < 100ms (sub-second TTS)")  
    print("   ✅ Good:      < 500ms")
    print("   ⚠️  Slow:     >= 500ms")
    print("   ❌ Failed:    Hook crashed")
    
    # Check if we met our sub-second TTS target
    tts_hooks = ["stop.py", "notification.py"]
    tts_performance = [results.get(hook, -1) for hook in tts_hooks if results.get(hook, -1) > 0]
    
    if tts_performance and all(duration < 1000 for duration in tts_performance):
        print("\n🎉 SUCCESS: All TTS hooks fire in sub-second time!")
    elif any(duration < 100 for duration in tts_performance):
        print("\n🚀 EXCELLENT: TTS hooks optimized for lightning-fast response!")
    else:
        print("\n⚠️  WARNING: Some TTS hooks still too slow")


if __name__ == "__main__":
    main()