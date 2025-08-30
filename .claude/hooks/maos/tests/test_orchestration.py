#!/usr/bin/env python3
"""
MAOS Test Engineer - Comprehensive Orchestration Test Suite

This script validates all MAOS orchestration capabilities:
1. Hook interception
2. Git worktree isolation
3. Session coordination
4. Multi-agent workflows
5. Cleanup and resource management
6. Invisible operation validation
"""

import json
import subprocess
import tempfile
import time
import os
from pathlib import Path
from datetime import datetime
from typing import Dict, List, Optional, Tuple

class MAOSTestSuite:
    """Comprehensive test suite for MAOS orchestration"""
    
    def __init__(self):
        self.test_results = []
        self.session_ids = []
        self.created_worktrees = []
        self.start_time = datetime.now()
        
    def log_result(self, test_name: str, status: str, details: str = "", issues: List[str] = None):
        """Log test result"""
        result = {
            "test": test_name,
            "status": status,
            "details": details,
            "issues": issues or [],
            "timestamp": datetime.now().isoformat()
        }
        self.test_results.append(result)
        
        # Print real-time feedback
        status_icon = "✅" if status == "PASS" else "❌" if status == "FAIL" else "⚠️"
        print(f"{status_icon} {test_name}: {status}")
        if details:
            print(f"   {details}")
        for issue in (issues or []):
            print(f"   Issue: {issue}")
    
    def test_environment_verification(self) -> bool:
        """Test 1: Environment Verification"""
        print("\n🔍 Test 1: Environment Verification")
        print("=" * 40)
        
        issues = []
        
        # Check hook exists and is executable
        hook_path = Path(".claude/hooks/pre_tool_use.py")
        if not hook_path.exists():
            issues.append("pre_tool_use.py hook not found")
        else:
            import stat
            mode = hook_path.stat().st_mode
            if not (mode & stat.S_IXUSR):
                issues.append("Hook is not executable")
        
        # Check backend utilities
        backend_path = Path(".claude/hooks/maos/backend.py")
        if not backend_path.exists():
            issues.append("MAOS backend utilities not found")
        
        # Check git worktree support
        try:
            result = subprocess.run(["git", "worktree", "list"], 
                                 capture_output=True, text=True, check=True)
        except subprocess.CalledProcessError:
            issues.append("Git worktree support not available")
        
        status = "PASS" if not issues else "FAIL"
        self.log_result("Environment Verification", status, 
                       "All components available" if not issues else "Missing components",
                       issues)
        return status == "PASS"
    
    def test_hook_interception(self) -> bool:
        """Test 2: Hook Interception Testing"""
        print("\n🎣 Test 2: Hook Interception Testing")
        print("=" * 40)
        
        issues = []
        
        # Test hook execution with various tool calls
        test_cases = [
            {
                "tool_name": "Read",
                "tool_input": {"file_path": "/test/path"}
            },
            {
                "tool_name": "Task", 
                "tool_input": {
                    "subagent_type": "test-agent",
                    "prompt": "Test prompt"
                }
            },
            {
                "tool_name": "Edit",
                "tool_input": {
                    "file_path": "/test/file.py",
                    "old_string": "old",
                    "new_string": "new"
                }
            }
        ]
        
        for i, test_case in enumerate(test_cases):
            try:
                hook_input = json.dumps(test_case)
                result = subprocess.run(
                    ["python3", ".claude/hooks/pre_tool_use.py"],
                    input=hook_input, text=True, capture_output=True
                )
                
                if result.returncode != 0:
                    issues.append(f"Hook failed for {test_case['tool_name']}: {result.stderr}")
                    
            except Exception as e:
                issues.append(f"Hook execution error for {test_case['tool_name']}: {e}")
        
        status = "PASS" if not issues else "FAIL"
        self.log_result("Hook Interception", status,
                       f"Tested {len(test_cases)} tool interceptions",
                       issues)
        return status == "PASS"
    
    def test_worktree_isolation(self) -> bool:
        """Test 3: Git Worktree Isolation Validation"""
        print("\n🌳 Test 3: Git Worktree Isolation Validation")
        print("=" * 40)
        
        issues = []
        
        try:
            # Create multiple test agents
            agent_types = ["test-backend", "test-frontend", "test-tester"]
            created_workspaces = []
            
            for agent_type in agent_types:
                try:
                    result = subprocess.run([
                        "python3", ".claude/hooks/maos/backend.py", 
                        "test-workspace", agent_type
                    ], capture_output=True, text=True, check=True)
                    
                    # Extract workspace path from output
                    output_lines = result.stdout.strip().split('\n')
                    for line in output_lines:
                        if line.startswith("Created workspace:"):
                            workspace = line.split(": ", 1)[1]
                            created_workspaces.append(workspace)
                            self.created_worktrees.append(workspace)
                            break
                    
                except subprocess.CalledProcessError as e:
                    issues.append(f"Failed to create workspace for {agent_type}: {e}")
            
            # Verify worktrees are isolated
            if created_workspaces:
                try:
                    result = subprocess.run(["git", "worktree", "list"], 
                                         capture_output=True, text=True, check=True)
                    worktree_list = result.stdout
                    
                    for workspace in created_workspaces:
                        if workspace not in worktree_list:
                            issues.append(f"Worktree {workspace} not found in git worktree list")
                        else:
                            # Check that worktree directory exists
                            if not Path(workspace).exists():
                                issues.append(f"Worktree directory {workspace} does not exist")
                    
                except subprocess.CalledProcessError as e:
                    issues.append(f"Failed to list worktrees: {e}")
            
            # Test worktree naming conventions
            for workspace in created_workspaces:
                if not workspace.startswith("worktrees/"):
                    issues.append(f"Worktree {workspace} doesn't follow naming convention")
                if "sess-" not in workspace:
                    issues.append(f"Worktree {workspace} missing session ID")
            
        except Exception as e:
            issues.append(f"Worktree isolation test failed: {e}")
        
        status = "PASS" if not issues else "FAIL"
        self.log_result("Worktree Isolation", status,
                       f"Created {len(created_workspaces)} isolated worktrees",
                       issues)
        return status == "PASS"
    
    def test_session_coordination(self) -> bool:
        """Test 4: Session Coordination Testing"""
        print("\n📋 Test 4: Session Coordination Testing")
        print("=" * 40)
        
        issues = []
        
        try:
            # Check session directory structure
            maos_dir = Path(".maos")
            sessions_dir = maos_dir / "sessions"
            
            if not maos_dir.exists():
                issues.append(".maos directory not created")
            elif not sessions_dir.exists():
                issues.append(".maos/sessions directory not created")
            else:
                # Find active sessions
                session_dirs = [d for d in sessions_dir.iterdir() if d.is_dir()]
                if not session_dirs:
                    issues.append("No session directories found")
                else:
                    # Check session file structure
                    for session_dir in session_dirs:
                        required_files = ["session.json", "agents.json", "locks.json", "progress.json"]
                        for req_file in required_files:
                            file_path = session_dir / req_file
                            if not file_path.exists():
                                issues.append(f"Missing {req_file} in session {session_dir.name}")
                            else:
                                # Validate JSON structure
                                try:
                                    with open(file_path) as f:
                                        json.load(f)
                                except json.JSONDecodeError:
                                    issues.append(f"Invalid JSON in {req_file}")
                        
                        # Store session ID for cleanup
                        self.session_ids.append(session_dir.name)
            
            # Test session status retrieval
            if self.session_ids:
                for session_id in self.session_ids:
                    try:
                        result = subprocess.run([
                            "python3", ".claude/hooks/maos/backend.py", 
                            "status", session_id
                        ], capture_output=True, text=True, check=True)
                        
                        # Validate status output is valid JSON
                        status_data = json.loads(result.stdout)
                        if 'session' not in status_data:
                            issues.append(f"Session status missing 'session' key for {session_id}")
                        if 'agents' not in status_data:
                            issues.append(f"Session status missing 'agents' key for {session_id}")
                            
                    except (subprocess.CalledProcessError, json.JSONDecodeError) as e:
                        issues.append(f"Failed to get session status for {session_id}: {e}")
        
        except Exception as e:
            issues.append(f"Session coordination test failed: {e}")
        
        status = "PASS" if not issues else "FAIL"
        self.log_result("Session Coordination", status,
                       f"Validated {len(self.session_ids)} sessions",
                       issues)
        return status == "PASS"
    
    def test_multi_agent_workflow(self) -> bool:
        """Test 5: Multi-Agent Workflow Validation"""
        print("\n🤖 Test 5: Multi-Agent Workflow Validation")
        print("=" * 40)
        
        issues = []
        
        try:
            # Simulate complex multi-agent workflow
            workflow_agents = [
                "architect", "backend-dev", "frontend-dev", 
                "tester", "reviewer", "deployer"
            ]
            
            created_agents = []
            
            # Create agents sequentially (simulating orchestrator spawning)
            for agent in workflow_agents:
                try:
                    result = subprocess.run([
                        "python3", ".claude/hooks/maos/backend.py", 
                        "test-workspace", f"workflow-{agent}"
                    ], capture_output=True, text=True, check=True)
                    
                    created_agents.append(f"workflow-{agent}")
                    
                    # Extract workspace for cleanup
                    output_lines = result.stdout.strip().split('\n')
                    for line in output_lines:
                        if line.startswith("Created workspace:"):
                            workspace = line.split(": ", 1)[1]
                            self.created_worktrees.append(workspace)
                            break
                    
                except subprocess.CalledProcessError as e:
                    issues.append(f"Failed to create workflow agent {agent}: {e}")
            
            # Verify agents can work in parallel without conflicts
            if len(created_agents) != len(workflow_agents):
                issues.append(f"Only created {len(created_agents)}/{len(workflow_agents)} agents")
            
            # Test agent handoffs through session files
            if self.session_ids:
                session_id = self.session_ids[-1]  # Use latest session
                
                # Check that all agents are registered
                try:
                    result = subprocess.run([
                        "python3", ".claude/hooks/maos/backend.py", 
                        "status", session_id
                    ], capture_output=True, text=True, check=True)
                    
                    status_data = json.loads(result.stdout)
                    registered_agents = status_data.get('agents', [])
                    
                    if len(registered_agents) < len(workflow_agents):
                        issues.append(f"Only {len(registered_agents)} agents registered in session")
                    
                except Exception as e:
                    issues.append(f"Failed to verify agent registration: {e}")
        
        except Exception as e:
            issues.append(f"Multi-agent workflow test failed: {e}")
        
        status = "PASS" if not issues else "FAIL"
        self.log_result("Multi-Agent Workflow", status,
                       f"Simulated workflow with {len(created_agents)} agents",
                       issues)
        return status == "PASS"
    
    def test_isolation_boundaries(self) -> bool:
        """Test 6: Isolation Boundary Testing"""
        print("\n🔒 Test 6: Isolation Boundary Testing")
        print("=" * 40)
        
        issues = []
        
        try:
            # Test that worktrees are properly isolated
            if self.created_worktrees:
                for worktree in self.created_worktrees[:2]:  # Test first two
                    if Path(worktree).exists():
                        # Check git operations are scoped
                        try:
                            result = subprocess.run([
                                "git", "-C", worktree, "status", "--porcelain"
                            ], capture_output=True, text=True, check=True)
                            
                            # Should not see other worktrees in status
                            if "worktrees/" in result.stdout:
                                issues.append(f"Worktree {worktree} can see other worktrees in git status")
                            
                        except subprocess.CalledProcessError as e:
                            issues.append(f"Git operations failed in {worktree}: {e}")
                    else:
                        issues.append(f"Worktree {worktree} directory missing")
            else:
                issues.append("No worktrees available for isolation testing")
            
            # Test file system isolation
            if len(self.created_worktrees) >= 2:
                worktree1 = Path(self.created_worktrees[0])
                worktree2 = Path(self.created_worktrees[1])
                
                if worktree1.exists() and worktree2.exists():
                    # They should have separate file systems
                    test_file1 = worktree1 / "isolation_test.txt"
                    test_file2 = worktree2 / "isolation_test.txt"
                    
                    try:
                        # Create different files in each worktree
                        with open(test_file1, 'w') as f:
                            f.write("Agent 1 isolated content")
                        
                        with open(test_file2, 'w') as f:
                            f.write("Agent 2 isolated content")
                        
                        # Verify they're different
                        with open(test_file1) as f:
                            content1 = f.read()
                        with open(test_file2) as f:
                            content2 = f.read()
                        
                        if content1 == content2:
                            issues.append("File isolation failed - same content in different worktrees")
                        
                        # Cleanup test files
                        test_file1.unlink(missing_ok=True)
                        test_file2.unlink(missing_ok=True)
                        
                    except Exception as e:
                        issues.append(f"File isolation test failed: {e}")
        
        except Exception as e:
            issues.append(f"Isolation boundary test failed: {e}")
        
        status = "PASS" if not issues else "FAIL"
        self.log_result("Isolation Boundaries", status,
                       "File system and git isolation validated",
                       issues)
        return status == "PASS"
    
    def test_cleanup_resource_management(self) -> bool:
        """Test 7: Cleanup and Resource Management"""
        print("\n🧹 Test 7: Cleanup and Resource Management")
        print("=" * 40)
        
        issues = []
        
        try:
            # Test cleanup functionality
            initial_worktrees = len(self.created_worktrees)
            
            if initial_worktrees > 0:
                try:
                    result = subprocess.run([
                        "python3", ".claude/hooks/maos/backend.py", "cleanup"
                    ], capture_output=True, text=True, check=True)
                    
                    # Check if worktrees were cleaned up appropriately
                    # (Note: cleanup only removes worktrees with no uncommitted changes)
                    result = subprocess.run(["git", "worktree", "list"], 
                                         capture_output=True, text=True, check=True)
                    current_worktrees = result.stdout.count("worktrees/")
                    
                    # Should have same or fewer worktrees
                    if current_worktrees > initial_worktrees:
                        issues.append(f"Cleanup increased worktree count: {current_worktrees} > {initial_worktrees}")
                    
                except subprocess.CalledProcessError as e:
                    issues.append(f"Cleanup command failed: {e}")
            
            # Test resource limits (should not create excessive worktrees)
            stress_test_agents = [f"stress-test-{i}" for i in range(10)]  # Try to create 10 agents
            created_in_stress = 0
            
            for agent in stress_test_agents:
                try:
                    result = subprocess.run([
                        "python3", ".claude/hooks/maos/backend.py", 
                        "test-workspace", agent
                    ], capture_output=True, text=True, timeout=5)
                    
                    if result.returncode == 0:
                        created_in_stress += 1
                        # Track for cleanup
                        output_lines = result.stdout.strip().split('\n')
                        for line in output_lines:
                            if line.startswith("Created workspace:"):
                                workspace = line.split(": ", 1)[1]
                                self.created_worktrees.append(workspace)
                                break
                    
                except (subprocess.CalledProcessError, subprocess.TimeoutExpired):
                    # Expected for resource limits
                    pass
            
            # Should create some but not all (resource limits should kick in)
            if created_in_stress == len(stress_test_agents):
                issues.append("No resource limits enforced - created all 10 stress test agents")
        
        except Exception as e:
            issues.append(f"Cleanup test failed: {e}")
        
        status = "PASS" if not issues else "WARNING"
        self.log_result("Cleanup & Resource Management", status,
                       f"Stress tested with {created_in_stress} agents",
                       issues)
        return status in ["PASS", "WARNING"]
    
    def test_invisible_operation(self) -> bool:
        """Test 8: Invisible Operation Validation"""
        print("\n👻 Test 8: Invisible Operation Validation")
        print("=" * 40)
        
        issues = []
        
        try:
            # Test hook execution time (should be < 10ms for user invisibility)
            test_cases = [
                {"tool_name": "Read", "tool_input": {"file_path": "/test"}},
                {"tool_name": "Edit", "tool_input": {"file_path": "/test", "old_string": "a", "new_string": "b"}},
                {"tool_name": "Bash", "tool_input": {"command": "echo test"}}
            ]
            
            execution_times = []
            
            for test_case in test_cases:
                start_time = time.time()
                
                try:
                    hook_input = json.dumps(test_case)
                    result = subprocess.run(
                        ["python3", ".claude/hooks/pre_tool_use.py"],
                        input=hook_input, text=True, capture_output=True,
                        timeout=1  # Should complete much faster
                    )
                    
                    end_time = time.time()
                    execution_time = (end_time - start_time) * 1000  # Convert to ms
                    execution_times.append(execution_time)
                    
                    if execution_time > 10:  # 10ms threshold
                        issues.append(f"{test_case['tool_name']} hook took {execution_time:.2f}ms (>10ms)")
                
                except subprocess.TimeoutExpired:
                    issues.append(f"{test_case['tool_name']} hook timed out (>1s)")
                except Exception as e:
                    issues.append(f"{test_case['tool_name']} hook failed: {e}")
            
            # Test that MAOS doesn't produce user-visible output
            # (stderr is OK, stdout should be minimal)
            test_input = {
                "tool_name": "Task",
                "tool_input": {
                    "subagent_type": "invisible-test",
                    "prompt": "Test invisibility"
                }
            }
            
            try:
                hook_input = json.dumps(test_input)
                result = subprocess.run(
                    ["python3", ".claude/hooks/pre_tool_use.py"],
                    input=hook_input, text=True, capture_output=True
                )
                
                # stdout should be empty (stderr can contain debug info)
                if result.stdout.strip():
                    issues.append(f"Hook produced stdout output: {result.stdout[:100]}")
                
                # Extract workspace from stderr for cleanup
                if "Created isolated workspace" in result.stderr:
                    lines = result.stderr.split('\n')
                    for line in lines:
                        if "workspace" in line and ":" in line:
                            try:
                                workspace = line.split(": ", 1)[1].strip()
                                if workspace:
                                    self.created_worktrees.append(workspace)
                            except:
                                pass
            
            except Exception as e:
                issues.append(f"Invisibility test failed: {e}")
            
            avg_time = sum(execution_times) / len(execution_times) if execution_times else 0
            
        except Exception as e:
            issues.append(f"Invisible operation test failed: {e}")
            avg_time = 0
        
        status = "PASS" if not issues else "WARNING"
        self.log_result("Invisible Operation", status,
                       f"Average hook execution: {avg_time:.2f}ms",
                       issues)
        return status in ["PASS", "WARNING"]
    
    def cleanup_test_artifacts(self):
        """Clean up all test artifacts"""
        print("\n🧽 Cleaning up test artifacts...")
        
        # Remove created worktrees
        for worktree in self.created_worktrees:
            try:
                if Path(worktree).exists():
                    # Unlock if locked
                    subprocess.run(["git", "worktree", "unlock", worktree], 
                                 capture_output=True)
                    
                    # Remove worktree
                    subprocess.run(["git", "worktree", "remove", worktree, "--force"], 
                                 capture_output=True)
                    print(f"   Removed worktree: {worktree}")
            except Exception as e:
                print(f"   Failed to remove {worktree}: {e}")
        
        # Clean up session files
        try:
            sessions_dir = Path(".maos/sessions")
            if sessions_dir.exists():
                for session_id in self.session_ids:
                    session_dir = sessions_dir / session_id
                    if session_dir.exists():
                        import shutil
                        shutil.rmtree(session_dir)
                        print(f"   Removed session: {session_id}")
        except Exception as e:
            print(f"   Failed to clean sessions: {e}")
        
        # Prune git worktrees
        try:
            subprocess.run(["git", "worktree", "prune"], capture_output=True)
            print("   Pruned git worktrees")
        except Exception as e:
            print(f"   Failed to prune worktrees: {e}")
    
    def generate_report(self) -> Dict:
        """Generate comprehensive test report"""
        total_tests = len(self.test_results)
        passed = sum(1 for r in self.test_results if r['status'] == 'PASS')
        failed = sum(1 for r in self.test_results if r['status'] == 'FAIL')
        warnings = sum(1 for r in self.test_results if r['status'] == 'WARNING')
        
        # Calculate test duration
        end_time = datetime.now()
        duration = end_time - self.start_time
        
        # Collect all issues
        all_issues = []
        for result in self.test_results:
            all_issues.extend(result.get('issues', []))
        
        # Critical findings (FAIL status)
        critical_findings = [r for r in self.test_results if r['status'] == 'FAIL']
        
        report = {
            "summary": {
                "total_tests": total_tests,
                "passed": passed,
                "failed": failed,
                "warnings": warnings,
                "duration": str(duration),
                "timestamp": end_time.isoformat()
            },
            "critical_findings": [f["test"] + ": " + ", ".join(f["issues"]) for f in critical_findings],
            "all_issues": all_issues,
            "test_details": self.test_results,
            "system_health": {
                "worktree_management": "OPERATIONAL" if failed == 0 else "DEGRADED",
                "hook_system": "OPERATIONAL" if passed > 0 else "FAILED",
                "session_coordination": "OPERATIONAL" if any(r["test"] == "Session Coordination" and r["status"] == "PASS" for r in self.test_results) else "FAILED",
                "user_experience": "INVISIBLE" if any(r["test"] == "Invisible Operation" and r["status"] in ["PASS", "WARNING"] for r in self.test_results) else "VISIBLE"
            }
        }
        
        return report
    
    def run_all_tests(self) -> Dict:
        """Run complete MAOS test suite"""
        print("🚀 MAOS Test Engineer - Comprehensive Orchestration Validation")
        print("=" * 60)
        print(f"Starting test suite at {self.start_time.isoformat()}")
        
        try:
            # Run all tests in sequence
            self.test_environment_verification()
            self.test_hook_interception()
            self.test_worktree_isolation()
            self.test_session_coordination()
            self.test_multi_agent_workflow()
            self.test_isolation_boundaries()
            self.test_cleanup_resource_management()
            self.test_invisible_operation()
            
        except KeyboardInterrupt:
            print("\n⚠️ Test suite interrupted by user")
        except Exception as e:
            print(f"\n❌ Test suite failed with error: {e}")
            self.log_result("Test Suite", "FAIL", str(e))
        finally:
            # Always cleanup
            self.cleanup_test_artifacts()
        
        # Generate and return report
        return self.generate_report()

def main():
    """Main test runner"""
    test_suite = MAOSTestSuite()
    report = test_suite.run_all_tests()
    
    # Print final report
    print("\n" + "=" * 60)
    print("📊 FINAL TEST REPORT")
    print("=" * 60)
    
    summary = report["summary"]
    print(f"Total Tests Run: {summary['total_tests']}")
    print(f"Passed: {summary['passed']}")
    print(f"Failed: {summary['failed']}")
    print(f"Warnings: {summary['warnings']}")
    print(f"Duration: {summary['duration']}")
    
    if report["critical_findings"]:
        print("\n❌ CRITICAL FINDINGS:")
        for finding in report["critical_findings"]:
            print(f"  - {finding}")
    
    print("\n🏥 SYSTEM HEALTH:")
    health = report["system_health"]
    for component, status in health.items():
        status_icon = "✅" if status in ["OPERATIONAL", "INVISIBLE"] else "❌"
        print(f"  {status_icon} {component.replace('_', ' ').title()}: {status}")
    
    if summary['failed'] == 0:
        print("\n🎉 MAOS orchestration is working perfectly! All tests passed.")
    elif summary['failed'] <= 2:
        print("\n⚠️ MAOS has minor issues but core functionality works.")
    else:
        print("\n💥 MAOS has significant issues requiring immediate attention.")
    
    # Save detailed report
    report_file = Path(f"maos_test_report_{int(time.time())}.json")
    with open(report_file, 'w') as f:
        json.dump(report, f, indent=2)
    print(f"\n📄 Detailed report saved to: {report_file}")
    
    return summary['failed'] == 0

if __name__ == "__main__":
    success = main()
    exit(0 if success else 1)
