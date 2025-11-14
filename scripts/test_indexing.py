#!/usr/bin/env python3
"""
Manual indexing test script - index Rust files from codetriever-indexing crate
"""

import json
import subprocess
import sys
from pathlib import Path

# Config
API_URL = "http://localhost:8080"
TARGET_DIR = Path(__file__).parent.parent / "crates" / "codetriever-indexing"

def get_git_info():
    """Extract git commit context from current repo"""
    result = subprocess.run(
        ["git", "log", "-1", "--format=%H%n%s%n%aI%n%an <%ae>"],
        capture_output=True,
        text=True,
        check=True
    )
    lines = result.stdout.strip().split("\n")

    repo_url = subprocess.run(
        ["git", "remote", "get-url", "origin"],
        capture_output=True,
        text=True,
        check=True
    ).stdout.strip()

    return {
        "repository_url": repo_url,
        "commit_sha": lines[0],
        "commit_message": lines[1],
        "commit_date": lines[2],
        "author": lines[3]
    }

def collect_rust_files(base_dir):
    """Collect all .rs files from target directory"""
    rust_files = sorted(base_dir.rglob("*.rs"))

    files = []
    for rs_file in rust_files:
        try:
            content = rs_file.read_text()
            # Make path relative to repo root
            rel_path = rs_file.relative_to(Path(__file__).parent.parent)
            files.append({
                "path": str(rel_path),
                "content": content
            })
            print(f"  ✓ {rel_path} ({len(content)} bytes)")
        except Exception as e:
            print(f"  ✗ {rs_file}: {e}", file=sys.stderr)

    return files

def send_index_request(payload):
    """Send POST /index request"""
    print(f"\n🚀 Sending POST /index to {API_URL}/index...")

    result = subprocess.run(
        [
            "curl",
            "-X", "POST",
            "-H", "Content-Type: application/json",
            "-d", json.dumps(payload),
            f"{API_URL}/index",
            "-w", "\n%{http_code}",
            "-s"
        ],
        capture_output=True,
        text=True
    )

    # Split response body and status code
    output = result.stdout.strip()
    lines = output.split("\n")
    status_code = lines[-1]
    response_body = "\n".join(lines[:-1])

    print(f"\n📡 HTTP {status_code}")
    print(f"Response: {response_body}")

    return response_body, status_code

def ensure_tenant_exists(tenant_id, tenant_name):
    """Ensure tenant exists in database, create if needed"""
    print(f"\n🔐 Checking tenant {tenant_id}...")

    # Check if tenant exists
    check_result = subprocess.run(
        ["just", "db-query", f"SELECT tenant_id FROM tenants WHERE tenant_id = '{tenant_id}';"],
        capture_output=True,
        text=True,
        cwd=Path(__file__).parent.parent
    )

    if tenant_id in check_result.stdout:
        print(f"  ✓ Tenant exists")
        return True

    # Create tenant
    print(f"  ⚠️  Tenant not found, creating...")
    create_result = subprocess.run(
        ["just", "db-query", f"INSERT INTO tenants (tenant_id, name) VALUES ('{tenant_id}', '{tenant_name}') RETURNING tenant_id, name;"],
        capture_output=True,
        text=True,
        cwd=Path(__file__).parent.parent
    )

    if create_result.returncode == 0:
        print(f"  ✅ Tenant created: {tenant_name}")
        return True
    else:
        print(f"  ❌ Failed to create tenant: {create_result.stderr}", file=sys.stderr)
        return False

def poll_job_until_complete(job_id, max_wait_seconds=60, poll_interval=3):
    """Poll job status until completion or timeout"""
    import time

    print(f"\n⏳ Polling job status every {poll_interval}s (max {max_wait_seconds}s)...")

    start_time = time.time()
    last_files_processed = 0
    poll_count = 0

    while (time.time() - start_time) < max_wait_seconds:
        result = subprocess.run(
            ["curl", "-s", f"{API_URL}/index/jobs/{job_id}"],
            capture_output=True,
            text=True
        )
        poll_count += 1

        try:
            status = json.loads(result.stdout)
            job_status = status.get("status", "unknown")
            files_processed = status.get("files_processed", 0)
            chunks_created = status.get("chunks_created", 0)

            # Show progress if changed
            if files_processed != last_files_processed:
                print(f"  📊 Status: {job_status} | Files: {files_processed} | Chunks: {chunks_created}")
                last_files_processed = files_processed

            if job_status == "completed":
                elapsed = time.time() - start_time
                print(f"\n✅ Job completed in {elapsed:.1f}s ({poll_count} status checks)")
                print(f"   Files processed: {files_processed}")
                print(f"   Chunks created: {chunks_created}")
                return status
            elif job_status == "failed":
                print(f"\n❌ Job failed!")
                print(f"   Error: {status.get('error_message', 'Unknown error')}")
                return None

        except json.JSONDecodeError:
            print(f"  ⚠️  Invalid JSON response: {result.stdout}")

        time.sleep(poll_interval)

    print(f"\n⏱️  Timeout waiting for job completion ({poll_count} status checks)")
    return None

def search_semantic(query, tenant_id="13e6e848-1183-4f2d-aa5a-6d5b69d0cb47", repository_id="codetriever", branch="main", limit=3):
    """Perform semantic search"""
    payload = {
        "tenant_id": tenant_id,
        "repository_id": repository_id,
        "branch": branch,
        "query": query,
        "limit": limit
    }

    result = subprocess.run(
        [
            "curl",
            "-X", "POST",
            "-H", "Content-Type: application/json",
            "-d", json.dumps(payload),
            f"{API_URL}/search",
            "-s"
        ],
        capture_output=True,
        text=True
    )

    try:
        response = json.loads(result.stdout)
        # Check for error in response
        if "error" in response:
            print(f"  ⚠️  Search error: {response.get('message', 'Unknown error')}", file=sys.stderr)
        return response
    except json.JSONDecodeError:
        print(f"  ⚠️  Invalid search response: {result.stdout}", file=sys.stderr)
        return None

def run_search_tests():
    """Run semantic search tests - both positive and negative cases"""
    print("\n" + "="*50)
    print("🔍 SEMANTIC SEARCH TESTS")
    print("="*50)

    # Positive test cases - should find results (natural language questions/commands)
    positive_tests = [
        "Where is the Qdrant database connection created?",
        "Show me how files are parsed into code chunks",
        "How does the PostgreSQL chunk queue work?",
        "Find the worker pool that processes indexing jobs",
        "Where are embeddings generated for semantic search?",
        "Show me the code that handles unchanged file detection"
    ]

    # Negative test cases - should NOT find high-confidence results
    negative_tests = [
        "How do I deploy a Kubernetes pod with this YAML?",
        "Show me React hooks for managing component state",
        "Where is the TensorFlow neural network training code?",
        "Find the Swift code for iOS mobile app navigation"
    ]

    print("\n✅ POSITIVE TESTS (should find results):")
    print("-" * 50)

    for query in positive_tests:
        print(f"\n🔎 Query: \"{query}\"")
        response = search_semantic(query, limit=2)

        if response and "matches" in response:
            matches = response.get("matches", [])
            print(f"   Found {len(matches)} matches")

            for i, match in enumerate(matches, 1):
                file_path = match.get("path", match.get("file", "unknown"))
                score = match.get("similarity", 0.0)
                lines = match.get("lines", {})
                start = lines.get("start", "?")
                end = lines.get("end", "?")
                print(f"   {i}. {file_path}:{start}-{end} (score: {score:.3f})")
        else:
            print(f"   ❌ Search failed or returned no matches")

    print("\n\n❌ NEGATIVE TESTS (should NOT find results):")
    print("-" * 50)

    for query in negative_tests:
        print(f"\n🔎 Query: \"{query}\"")
        response = search_semantic(query, limit=2)

        if response and "matches" in response:
            matches = response.get("matches", [])
            # Filter matches by similarity threshold (< 0.5 = noise)
            high_confidence_matches = [m for m in matches if m.get("similarity", 0.0) >= 0.5]

            if len(high_confidence_matches) == 0:
                print(f"   ✅ No high-confidence matches (found {len(matches)} low-confidence noise)")
            else:
                print(f"   ⚠️  Unexpectedly found {len(high_confidence_matches)} high-confidence matches:")
                for i, match in enumerate(high_confidence_matches, 1):
                    file_path = match.get("path", match.get("file", "unknown"))
                    score = match.get("similarity", 0.0)
                    print(f"   {i}. {file_path} (score: {score:.3f})")
        else:
            print(f"   ❌ Search failed")

def main():
    print("🎯 Codetriever Indexing Test")
    print("=" * 50)

    # Get git context
    print("\n📋 Extracting git commit context...")
    commit_context = get_git_info()
    print(f"  Commit: {commit_context['commit_sha'][:8]}")
    print(f"  Message: {commit_context['commit_message']}")
    print(f"  Author: {commit_context['author']}")

    # Collect files
    print(f"\n📂 Collecting Rust files from {TARGET_DIR}...")
    files = collect_rust_files(TARGET_DIR)
    print(f"\n✅ Found {len(files)} Rust files")

    if not files:
        print("❌ No files to index!", file=sys.stderr)
        sys.exit(1)

    # Ensure tenant exists
    tenant_id = "13e6e848-1183-4f2d-aa5a-6d5b69d0cb47"
    if not ensure_tenant_exists(tenant_id, "test-tenant"):
        print("❌ Failed to ensure tenant exists", file=sys.stderr)
        sys.exit(1)

    # Build payload
    payload = {
        "tenant_id": tenant_id,
        "project_id": "codetriever",
        "commit_context": commit_context,
        "files": files
    }

    # Save for inspection
    payload_file = "/tmp/index_request.json"
    with open(payload_file, "w") as f:
        json.dump(payload, f, indent=2)
    print(f"\n💾 Saved payload to {payload_file}")

    # Send request
    response_body, status_code = send_index_request(payload)

    # Parse and handle response
    try:
        response = json.loads(response_body)

        if status_code.startswith("2"):  # 2xx success
            if "job_id" in response:
                job_id = response["job_id"]
                files_queued = response.get("files_queued", 0)
                print(f"\n✅ Job created successfully!")
                print(f"   Job ID: {job_id}")
                print(f"   Files queued: {files_queued}")

                # Poll until completion
                job_result = poll_job_until_complete(job_id, max_wait_seconds=60)

                if not job_result:
                    print("\n❌ Job did not complete successfully")
                    sys.exit(1)

                # Run search tests
                run_search_tests()

                print("\n" + "="*50)
                print("🎉 ALL TESTS COMPLETE!")
                print("="*50)
            else:
                print(f"\n✅ Success! Response: {json.dumps(response, indent=2)}")
        else:
            print(f"\n❌ Request failed with status {status_code}")
            print(f"   Response: {json.dumps(response, indent=2)}")
            sys.exit(1)

    except json.JSONDecodeError:
        print(f"\n⚠️  Could not parse response as JSON")
        print(f"   Raw: {response_body}")
        sys.exit(1)

if __name__ == "__main__":
    main()
