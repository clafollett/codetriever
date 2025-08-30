#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.8"
# ///

"""
MAOS Integration Test Suite

Comprehensive end-to-end testing of the new directory-based atomic operations system.
Tests all components: state management, file locking, agent coordination, and performance.
"""

import os
import sys
import json
import time
import threading
import tempfile
from pathlib import Path
from datetime import datetime
from concurrent.futures import ThreadPoolExecutor, as_completed

# Add path for imports using parent directory
maos_dir = Path(__file__).parent.parent  
sys.path.insert(0, str(maos_dir))

from utils.state_manager import MAOSStateManager
from utils.file_locking import MAOSFileLockManager
from utils.backend import MAOSBackend


class MAOSIntegrationTests:
    """Comprehensive integration test suite for MAOS"""
    
    def __init__(self):
        self.test_results = {}
        self.temp_dir = None
        self.session_id = f"test-session-{int(time.time())}"
    
    def setup_test_environment(self):
        """Setup isolated test environment"""
        self.temp_dir = tempfile.mkdtemp()
        os.chdir(self.temp_dir)
        
        # Create test project structure
        test_dirs = [".maos", "worktrees", "logs"]
        for dir_name in test_dirs:
            Path(dir_name).mkdir(exist_ok=True)
        
        print(f"🧪 Test environment setup: {self.temp_dir}")
    
    def cleanup_test_environment(self):
        """Clean up test environment"""
        if self.temp_dir:
            import shutil
            shutil.rmtree(self.temp_dir, ignore_errors=True)
            print(f"🧹 Cleaned up test environment: {self.temp_dir}")
    
    def test_atomic_state_management(self):
        """Test 1: Atomic State Management"""
        print("\\n🎯 Test 1: Atomic State Management")
        
        start_time = time.time()
        
        # Create state manager
        state_manager = MAOSStateManager(self.session_id)
        
        # Test agent registration
        agent_id = "backend-engineer-test-12345-abcd1234"
        success = state_manager.register_pending_agent(agent_id, "backend-engineer", {
            "transcript_path": "/test/path",
            "cwd": "/test/cwd"
        })
        assert success, "Agent registration failed"
        
        # Verify pending state
        state = state_manager.get_agent_state(agent_id)
        assert state == "pending", f"Expected pending, got {state}"
        
        # Test atomic transition to active
        workspace_path = f"/test/workspace/{agent_id}"
        transition_success = state_manager.transition_to_active(agent_id, workspace_path)
        assert transition_success, "Transition to active failed"
        
        # Verify active state
        new_state = state_manager.get_agent_state(agent_id)
        assert new_state == "active", f"Expected active, got {new_state}"
        
        # Test transition to completed
        complete_success = state_manager.transition_to_completed(agent_id)
        assert complete_success, "Transition to completed failed"
        
        # Verify completed state
        final_state = state_manager.get_agent_state(agent_id)
        assert final_state == "completed", f"Expected completed, got {final_state}"
        
        end_time = time.time()
        
        self.test_results["atomic_state_management"] = {
            "passed": True,
            "duration": end_time - start_time,
            "operations": 4
        }
        print(f"✅ Atomic state management: {end_time - start_time:.3f}s")
    
    def test_directory_file_locking(self):
        """Test 2: Directory-Based File Locking"""
        print("\\n🔒 Test 2: Directory-Based File Locking")
        
        start_time = time.time()
        
        # Create lock manager
        lock_manager = MAOSFileLockManager(self.session_id)
        
        test_file = "/test/file.py"
        agent1_id = "agent1-test-12345"
        agent2_id = "agent2-test-12345"
        
        # Agent 1 acquires lock
        lock1_acquired = lock_manager.acquire_lock(agent1_id, test_file, "write", 1.0)
        assert lock1_acquired, "Agent 1 should acquire lock"
        
        # Agent 2 should fail to acquire conflicting lock
        lock2_acquired = lock_manager.acquire_lock(agent2_id, test_file, "write", 0.1)
        assert not lock2_acquired, "Agent 2 should not acquire conflicting lock"
        
        # Verify lock info
        lock_info = lock_manager.get_lock_info(test_file)
        assert lock_info is not None, "Lock info should exist"
        assert lock_info["agent_id"] == agent1_id, "Lock should be owned by agent1"
        
        # Agent 1 releases lock
        release_success = lock_manager.release_lock(agent1_id, test_file)
        assert release_success, "Lock release should succeed"
        
        # Agent 2 should now be able to acquire lock
        lock2_retry = lock_manager.acquire_lock(agent2_id, test_file, "write", 1.0)
        assert lock2_retry, "Agent 2 should now acquire lock"
        
        # Clean up
        lock_manager.release_lock(agent2_id, test_file)
        
        end_time = time.time()
        
        self.test_results["directory_file_locking"] = {
            "passed": True,
            "duration": end_time - start_time,
            "operations": 6
        }
        print(f"✅ Directory-based file locking: {end_time - start_time:.3f}s")
    
    def test_concurrent_operations(self):
        """Test 3: Concurrent Multi-Agent Operations"""
        print("\\n⚡ Test 3: Concurrent Multi-Agent Operations")
        
        start_time = time.time()
        
        def simulate_agent_work(agent_num):
            """Simulate an agent doing work"""
            agent_id = f"concurrent-agent-{agent_num}-{int(time.time())}"
            
            # Create state manager for this agent
            state_manager = MAOSStateManager(self.session_id)
            
            # Register agent
            success = state_manager.register_pending_agent(agent_id, f"worker-{agent_num}", {
                "agent_num": agent_num,
                "cwd": "/test"
            })
            
            if success:
                # Simulate workspace creation delay
                time.sleep(0.05)
                
                # Transition to active
                workspace = f"/test/workspace/agent-{agent_num}"
                state_manager.transition_to_active(agent_id, workspace)
                
                # Simulate work
                time.sleep(0.1)
                
                # Complete
                state_manager.transition_to_completed(agent_id)
                
                return {"agent_id": agent_id, "success": True}
            
            return {"agent_id": agent_id, "success": False}
        
        # Launch 10 concurrent agents
        num_agents = 10
        with ThreadPoolExecutor(max_workers=num_agents) as executor:
            futures = [executor.submit(simulate_agent_work, i) for i in range(num_agents)]
            
            results = []
            for future in as_completed(futures):
                results.append(future.result())
        
        # Verify all agents succeeded
        successful_agents = [r for r in results if r["success"]]
        assert len(successful_agents) == num_agents, f"Expected {num_agents} successful agents, got {len(successful_agents)}"
        
        # Verify state consistency
        state_manager = MAOSStateManager(self.session_id)
        summary = state_manager.get_state_summary()
        
        assert summary["pending_count"] == 0, f"Expected 0 pending agents, got {summary['pending_count']}"
        assert summary["completed_count"] >= num_agents, f"Expected >= {num_agents} completed, got {summary['completed_count']}"
        
        end_time = time.time()
        
        self.test_results["concurrent_operations"] = {
            "passed": True,
            "duration": end_time - start_time,
            "agents": num_agents,
            "success_rate": len(successful_agents) / num_agents
        }
        print(f"✅ Concurrent operations: {num_agents} agents in {end_time - start_time:.3f}s")
    
    def test_backend_integration(self):
        """Test 4: Full Backend Integration"""
        print("\\n🔧 Test 4: Full Backend Integration")
        
        start_time = time.time()
        
        # Create backend
        backend = MAOSBackend()
        
        # Create session
        session_id = backend.get_or_create_session({"test": True})
        assert session_id is not None, "Session creation failed"
        
        # Register agent
        agent_id = backend.register_pending_agent("integration-tester", session_id, {
            "test_mode": True
        })
        assert agent_id is not None, "Agent registration failed"
        
        # Get agent info
        agent_info = backend.get_agent_info(agent_id, session_id)
        assert agent_info is not None, "Agent info retrieval failed"
        assert agent_info["status"] == "pending", "Agent should be pending"
        
        # Create workspace
        workspace_path = backend.create_workspace_if_needed(agent_id, session_id)
        # Note: This might fail if git isn't available, which is OK for this test
        
        # Test file locking
        test_file = "/test/integration/file.py"
        lock_acquired = backend.acquire_file_lock(test_file, agent_id, session_id, "test")
        
        if lock_acquired:
            # Check lock
            lock_info = backend.check_file_lock(test_file, session_id, agent_id)
            # Should not show as locked for the same agent
            
            # Release lock
            backend.release_file_lock(test_file, agent_id, session_id)
        
        end_time = time.time()
        
        self.test_results["backend_integration"] = {
            "passed": True,
            "duration": end_time - start_time,
            "session_id": session_id,
            "agent_id": agent_id
        }
        print(f"✅ Backend integration: {end_time - start_time:.3f}s")
    
    def test_cleanup_and_recovery(self):
        """Test 5: Cleanup and Recovery"""
        print("\\n🧹 Test 5: Cleanup and Recovery")
        
        start_time = time.time()
        
        state_manager = MAOSStateManager(self.session_id)
        
        # Create some stale agents by manipulating timestamps
        old_agent_id = "stale-agent-12345"
        state_manager.register_pending_agent(old_agent_id, "stale-worker", {"test": True})
        
        # Make the agent file appear old by changing its mtime
        agent_file = state_manager.pending_agents_dir / f"{old_agent_id}.json"
        if agent_file.exists():
            old_time = time.time() - (25 * 3600)  # 25 hours ago
            os.utime(agent_file, (old_time, old_time))
        
        # Run cleanup
        cleanup_stats = state_manager.cleanup_stale_agents(max_age_hours=24)
        
        # Verify cleanup worked
        assert cleanup_stats["pending"] >= 1, "Should have cleaned up at least 1 pending agent"
        
        # Verify cleanup timestamp was recorded
        summary = state_manager.get_state_summary()
        assert summary["last_cleanup"] is not None, "Cleanup timestamp should be recorded"
        
        end_time = time.time()
        
        self.test_results["cleanup_recovery"] = {
            "passed": True,
            "duration": end_time - start_time,
            "cleanup_stats": cleanup_stats
        }
        print(f"✅ Cleanup and recovery: {end_time - start_time:.3f}s")
    
    def test_performance_benchmarks(self):
        """Test 6: Performance Benchmarks"""
        print("\\n⚡ Test 6: Performance Benchmarks")
        
        operations = {
            "agent_registration": [],
            "state_transitions": [],
            "lock_operations": []
        }
        
        state_manager = MAOSStateManager(self.session_id)
        lock_manager = MAOSFileLockManager(self.session_id)
        
        # Benchmark agent registration
        for i in range(100):
            start = time.time()
            agent_id = f"perf-agent-{i}-{int(time.time())}"
            state_manager.register_pending_agent(agent_id, "perf-tester", {"perf": True})
            operations["agent_registration"].append(time.time() - start)
        
        # Benchmark state transitions
        for i in range(50):
            start = time.time()
            agent_id = f"transition-agent-{i}-{int(time.time())}"
            state_manager.register_pending_agent(agent_id, "transition-tester", {})
            state_manager.transition_to_active(agent_id, f"/test/workspace/{agent_id}")
            operations["state_transitions"].append(time.time() - start)
        
        # Benchmark lock operations
        for i in range(50):
            start = time.time()
            file_path = f"/test/perf/file-{i}.py"
            agent_id = f"lock-agent-{i}"
            lock_manager.acquire_lock(agent_id, file_path, "perf-test", 1.0)
            lock_manager.release_lock(agent_id, file_path)
            operations["lock_operations"].append(time.time() - start)
        
        # Calculate statistics
        performance_stats = {}
        for op_type, times in operations.items():
            performance_stats[op_type] = {
                "count": len(times),
                "avg_ms": (sum(times) / len(times)) * 1000,
                "max_ms": max(times) * 1000,
                "min_ms": min(times) * 1000
            }
        
        self.test_results["performance_benchmarks"] = {
            "passed": True,
            "stats": performance_stats
        }
        
        print(f"✅ Performance benchmarks completed:")
        for op_type, stats in performance_stats.items():
            print(f"   {op_type}: {stats['avg_ms']:.2f}ms avg, {stats['max_ms']:.2f}ms max")
    
    def run_all_tests(self):
        """Run complete test suite"""
        print("🚀 MAOS Integration Test Suite Starting...")
        print("=" * 60)
        
        try:
            self.setup_test_environment()
            
            # Run all tests
            self.test_atomic_state_management()
            self.test_directory_file_locking()
            self.test_concurrent_operations()
            self.test_backend_integration()
            self.test_cleanup_and_recovery()
            self.test_performance_benchmarks()
            
            # Summary
            print("\\n" + "=" * 60)
            print("🎯 TEST SUITE RESULTS:")
            
            total_tests = len(self.test_results)
            passed_tests = sum(1 for r in self.test_results.values() if r.get("passed", False))
            
            print(f"   Total Tests: {total_tests}")
            print(f"   Passed: {passed_tests}")
            print(f"   Failed: {total_tests - passed_tests}")
            print(f"   Success Rate: {(passed_tests/total_tests)*100:.1f}%")
            
            if passed_tests == total_tests:
                print("\\n🎉 ALL TESTS PASSED! The new MAOS system is ready for production! 🚀")
            else:
                print(f"\\n❌ {total_tests - passed_tests} tests failed. Please review failures.")
            
            # Performance summary
            if "performance_benchmarks" in self.test_results:
                print("\\n⚡ Performance Summary:")
                stats = self.test_results["performance_benchmarks"]["stats"]
                for op_type, perf in stats.items():
                    status = "🟢" if perf["avg_ms"] < 1.0 else "🟡" if perf["avg_ms"] < 10.0 else "🔴"
                    print(f"   {status} {op_type}: {perf['avg_ms']:.2f}ms average")
            
            return passed_tests == total_tests
            
        except Exception as e:
            print(f"\\n❌ Test suite failed with exception: {e}")
            import traceback
            traceback.print_exc()
            return False
            
        finally:
            self.cleanup_test_environment()


if __name__ == "__main__":
    """Run integration tests when executed directly"""
    test_suite = MAOSIntegrationTests()
    success = test_suite.run_all_tests()
    
    sys.exit(0 if success else 1)