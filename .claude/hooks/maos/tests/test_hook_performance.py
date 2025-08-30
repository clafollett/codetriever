#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "python-dotenv",
# ]
# ///

import asyncio
import json
import subprocess
import sys
import time
from pathlib import Path
from typing import Dict, List
import tempfile

# Add utils to path for testing
sys.path.insert(0, str(Path(__file__).parent.parent.parent))
from maos.utils.async_logging import AsyncJSONLLogger, cleanup_async_systems

async def test_hook_performance(hook_name: str, test_data: Dict) -> Dict:
    """Test performance of a single hook."""
    hook_dir = Path(__file__).parent
    hook_script = hook_dir / f"{hook_name}.py"
    
    if not hook_script.exists():
        return {
            "hook": hook_name,
            "error": "Hook script not found",
            "time_ms": 0,
            "success": False
        }
    
    try:
        start_time = time.time()
        
        # Run hook with test data
        process = await asyncio.create_subprocess_exec(
            "uv", "run", str(hook_script),
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE
        )
        
        # Send test data as JSON
        test_json = json.dumps(test_data)
        stdout, stderr = await process.communicate(test_json.encode())
        
        end_time = time.time()
        execution_time = (end_time - start_time) * 1000  # Convert to ms
        
        return {
            "hook": hook_name,
            "time_ms": round(execution_time, 2),
            "exit_code": process.returncode,
            "stdout": stdout.decode() if stdout else "",
            "stderr": stderr.decode() if stderr else "",
            "success": process.returncode == 0
        }
        
    except Exception as e:
        return {
            "hook": hook_name,
            "error": str(e),
            "time_ms": 0,
            "success": False
        }

async def test_async_logging_performance() -> Dict:
    """Test performance of async JSONL logging."""
    with tempfile.TemporaryDirectory() as temp_dir:
        log_file = Path(temp_dir) / "test_performance.jsonl"
        logger = AsyncJSONLLogger(max_workers=2, batch_size=5)
        
        try:
            await logger.start()
            
            # Test logging 100 entries
            start_time = time.time()
            
            for i in range(100):
                test_entry = {
                    "test_id": i,
                    "message": f"Performance test entry {i}",
                    "data": {"nested": {"value": i * 2}}
                }
                await logger.log_async(log_file, test_entry)
            
            # Wait for completion
            await logger.stop()
            
            end_time = time.time()
            total_time = (end_time - start_time) * 1000
            
            # Verify entries were written
            if log_file.exists():
                with open(log_file, 'r') as f:
                    lines = f.readlines()
                entries_written = len(lines)
            else:
                entries_written = 0
            
            return {
                "test": "async_logging",
                "entries": 100,
                "entries_written": entries_written,
                "time_ms": round(total_time, 2),
                "avg_per_entry_ms": round(total_time / 100, 3),
                "success": entries_written == 100
            }
            
        except Exception as e:
            return {
                "test": "async_logging",
                "error": str(e),
                "success": False
            }
        finally:
            try:
                await logger.stop()
            except:
                pass

async def run_performance_tests() -> Dict:
    """Run comprehensive performance tests."""
    print("🧪 Starting Claude Code hook performance tests...")
    
    # Test data for different hook types
    test_data = {
        "stop_hook": {
            "event": "stop",
            "transcript_path": "/tmp/test_transcript.jsonl",
            "metadata": {"session_id": "test-123"}
        },
        "notification_hook": {
            "message": "Test notification", 
            "metadata": {"timestamp": time.time()}
        },
        "pre_tool_hook": {
            "tool_name": "Read",
            "tool_input": {"file_path": "/tmp/test.txt"},
            "metadata": {"hook_test": True}
        },
        "post_tool_hook": {
            "tool_name": "Edit",
            "tool_input": {"file_path": "/tmp/test.py"},
            "tool_response": {"success": True},
            "metadata": {"hook_test": True}
        }
    }
    
    # Test individual hooks
    hook_tests = []
    
    print("⏱️  Testing stop hook (TTS critical path)...")
    stop_result = await test_hook_performance("stop", test_data["stop_hook"])
    hook_tests.append(stop_result)
    
    print("⏱️  Testing notification hook...")
    notification_result = await test_hook_performance("notification", test_data["notification_hook"])
    hook_tests.append(notification_result)
    
    print("⏱️  Testing pre-tool hook...")
    pre_tool_result = await test_hook_performance("pre_tool_use", test_data["pre_tool_hook"])
    hook_tests.append(pre_tool_result)
    
    print("⏱️  Testing post-tool hook...")
    post_tool_result = await test_hook_performance("post_tool_use", test_data["post_tool_hook"])
    hook_tests.append(post_tool_result)
    
    # Test async logging
    print("⏱️  Testing async JSONL logging...")
    logging_result = await test_async_logging_performance()
    
    # Calculate summary statistics
    successful_tests = [t for t in hook_tests if t.get("success", False)]
    failed_tests = [t for t in hook_tests if not t.get("success", False)]
    
    if successful_tests:
        avg_time = sum(t["time_ms"] for t in successful_tests) / len(successful_tests)
        max_time = max(t["time_ms"] for t in successful_tests)
        min_time = min(t["time_ms"] for t in successful_tests)
    else:
        avg_time = max_time = min_time = 0
    
    return {
        "summary": {
            "total_hooks_tested": len(hook_tests),
            "successful_hooks": len(successful_tests),
            "failed_hooks": len(failed_tests),
            "avg_hook_time_ms": round(avg_time, 2),
            "max_hook_time_ms": round(max_time, 2),
            "min_hook_time_ms": round(min_time, 2),
            "tts_critical_time_ms": stop_result.get("time_ms", 0),
            "sub_second_performance": max_time < 1000 if successful_tests else False
        },
        "individual_results": hook_tests,
        "async_logging": logging_result
    }

def print_results(results: Dict) -> None:
    """Pretty print test results."""
    print("\n" + "="*60)
    print("🏁 CLAUDE CODE HOOK PERFORMANCE TEST RESULTS")
    print("="*60)
    
    summary = results["summary"]
    print(f"\n📊 SUMMARY:")
    print(f"   Total hooks tested: {summary['total_hooks_tested']}")
    print(f"   Successful: {summary['successful_hooks']} ✅")
    print(f"   Failed: {summary['failed_hooks']} {'❌' if summary['failed_hooks'] > 0 else '✅'}")
    print(f"   Average execution time: {summary['avg_hook_time_ms']}ms")
    print(f"   TTS critical path: {summary['tts_critical_time_ms']}ms")
    print(f"   Sub-second performance: {'✅ YES' if summary['sub_second_performance'] else '❌ NO'}")
    
    print(f"\n🎯 INDIVIDUAL HOOK RESULTS:")
    for result in results["individual_results"]:
        status = "✅" if result.get("success", False) else "❌"
        hook_name = result.get("hook", "unknown")
        time_ms = result.get("time_ms", 0)
        print(f"   {status} {hook_name}: {time_ms}ms")
        
        if not result.get("success", False) and result.get("error"):
            print(f"      Error: {result['error']}")
    
    # Async logging results
    logging = results["async_logging"]
    if logging.get("success", False):
        print(f"\n📝 ASYNC LOGGING PERFORMANCE:")
        print(f"   ✅ {logging['entries']} entries in {logging['time_ms']}ms")
        print(f"   Average per entry: {logging['avg_per_entry_ms']}ms")
    else:
        print(f"\n📝 ASYNC LOGGING: ❌ {logging.get('error', 'Failed')}")
    
    print("\n" + "="*60)
    
    # TTS-specific verdict
    if summary["tts_critical_time_ms"] < 100:
        print("🚀 TTS PERFORMANCE: EXCELLENT! Sub-100ms response time achieved.")
    elif summary["tts_critical_time_ms"] < 500:
        print("⚡ TTS PERFORMANCE: GOOD! Response time under 500ms.")
    else:
        print("🐌 TTS PERFORMANCE: NEEDS OPTIMIZATION! Response time over 500ms.")
    
    print("="*60 + "\n")

async def main():
    """Run the performance test suite."""
    try:
        results = await run_performance_tests()
        print_results(results)
        
        # Return appropriate exit code
        if results["summary"]["sub_second_performance"]:
            print("🎉 ALL TESTS PASSED! Hook system is optimized for high performance.")
            return 0
        else:
            print("⚠️  PERFORMANCE ISSUES DETECTED. See results above.")
            return 1
            
    except Exception as e:
        print(f"❌ Test suite failed: {e}")
        return 1
    finally:
        # Clean up async systems
        try:
            await cleanup_async_systems()
        except:
            pass

if __name__ == "__main__":
    import sys
    exit_code = asyncio.run(main())
    sys.exit(exit_code)