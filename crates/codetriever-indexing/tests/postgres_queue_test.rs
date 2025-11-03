//! Integration test for PostgreSQL-backed persistent file queue
//!
//! Tests that files flow through the database queue correctly

#[path = "test_utils.rs"]
mod test_utils;

use codetriever_indexing::indexing::service::FileContent;
use test_utils::create_test_repository;
use uuid::Uuid;

/// Create a unique test tenant in the database
async fn create_test_tenant(
    repository: &std::sync::Arc<dyn codetriever_meta_data::traits::FileRepository>,
) -> Uuid {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let tenant_name = format!("test_tenant_{timestamp}");

    repository
        .create_tenant(&tenant_name)
        .await
        .expect("Failed to create tenant")
}

#[test]
fn test_postgres_queue_push_and_pop() {
    test_utils::get_test_runtime().block_on(async {
        let repository = create_test_repository().await;
        let tenant_id = create_test_tenant(&repository).await;

        // Create project branch first (required for jobs foreign key)
        let ctx = codetriever_meta_data::models::RepositoryContext {
            tenant_id,
            repository_id: "test_repo".to_string(),
            repository_url: None,
            branch: "main".to_string(),
            commit_sha: None,
            commit_message: None,
            commit_date: None,
            author: None,
            is_dirty: false,
            root_path: std::path::PathBuf::from("."),
        };
        repository
            .ensure_project_branch(&ctx)
            .await
            .expect("Failed to create project branch");

        // Create parent job (required for queue foreign key)
        let job = repository
            .create_indexing_job(&tenant_id, "test_repo", "main", None)
            .await
            .expect("Failed to create job");
        let job_id = job.job_id;

        // Create a test file
        let file = FileContent {
            path: "src/test.rs".to_string(),
            content: "fn test() {}".to_string(),
            hash: "test_hash".to_string(),
        };

        // Push to queue
        repository
            .enqueue_file(
                &job_id,
                &tenant_id,
                "test_repo",
                "main",
                &file.path,
                &file.content,
                &file.hash,
            )
            .await
            .expect("Failed to enqueue file");

        // Check queue depth
        let depth = repository
            .get_queue_depth(&job_id)
            .await
            .expect("Failed to get queue depth");
        assert_eq!(depth, 1, "Queue should have 1 file");

        // Pop from queue (global FIFO, no job_id filter)
        let result = repository
            .dequeue_file()
            .await
            .expect("Failed to dequeue file");

        assert!(result.is_some(), "Should dequeue a file");
        let dequeued = result.unwrap();
        assert_eq!(dequeued.file_path, file.path);
        assert_eq!(dequeued.file_content, file.content);
        assert_eq!(dequeued.content_hash, file.hash);

        // Queue should be empty now
        let depth = repository
            .get_queue_depth(&job_id)
            .await
            .expect("Failed to get queue depth");
        assert_eq!(depth, 0, "Queue should be empty after dequeue");

        // Pop again should return None
        let result = repository
            .dequeue_file()
            .await
            .expect("Failed to dequeue from empty queue");
        assert!(result.is_none(), "Empty queue should return None");
    })
}

#[test]
fn test_postgres_queue_concurrent_workers() {
    test_utils::get_test_runtime().block_on(async {
        let repository = create_test_repository().await;
        let tenant_id = create_test_tenant(&repository).await;

        // Create project branch first (required for jobs foreign key)
        let ctx = codetriever_meta_data::models::RepositoryContext {
            tenant_id,
            repository_id: "test_repo".to_string(),
            repository_url: None,
            branch: "main".to_string(),
            commit_sha: None,
            commit_message: None,
            commit_date: None,
            author: None,
            is_dirty: false,
            root_path: std::path::PathBuf::from("."),
        };
        repository
            .ensure_project_branch(&ctx)
            .await
            .expect("Failed to create project branch");

        // Create parent job (required for queue foreign key)
        let job = repository
            .create_indexing_job(&tenant_id, "test_repo", "main", None)
            .await
            .expect("Failed to create job");
        let job_id = job.job_id;

        // Push 10 files to queue
        for i in 0..10 {
            repository
                .enqueue_file(
                    &job_id,
                    &tenant_id,
                    "test_repo",
                    "main",
                    &format!("file_{i}.rs"),
                    &format!("content {i}"),
                    &format!("hash_{i}"),
                )
                .await
                .expect("Failed to enqueue");
        }

        // Spawn 3 concurrent workers trying to dequeue
        let mut handles = vec![];
        for worker_id in 0..3 {
            let repo = repository.clone();
            let handle = tokio::spawn(async move {
                let mut processed = vec![];
                loop {
                    match repo.dequeue_file().await {
                        Ok(Some(dequeued)) => {
                            processed.push(dequeued.file_path);
                        }
                        Ok(None) => break, // Queue empty
                        Err(e) => panic!("Worker {worker_id} failed: {e}"),
                    }
                }
                processed
            });
            handles.push(handle);
        }

        // Collect results from all workers
        let mut all_processed = vec![];
        for handle in handles {
            let files = handle.await.expect("Worker panicked");
            all_processed.extend(files);
        }

        // Should have processed all 10 files exactly once (no duplicates due to SKIP LOCKED)
        all_processed.sort();
        assert_eq!(all_processed.len(), 10, "Should process all 10 files");

        // Verify no duplicates
        let unique_count = all_processed
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len();
        assert_eq!(
            unique_count, 10,
            "No file should be processed twice (SKIP LOCKED prevents duplicates)"
        );
    })
}

#[test]
fn test_postgres_queue_crash_recovery() {
    test_utils::get_test_runtime().block_on(async {
        let repository = create_test_repository().await;
        let tenant_id = create_test_tenant(&repository).await;

        // Create project branch and job
        let ctx = codetriever_meta_data::models::RepositoryContext {
            tenant_id,
            repository_id: "test_repo".to_string(),
            repository_url: None,
            branch: "main".to_string(),
            commit_sha: None,
            commit_message: None,
            commit_date: None,
            author: None,
            is_dirty: false,
            root_path: std::path::PathBuf::from("."),
        };
        repository
            .ensure_project_branch(&ctx)
            .await
            .expect("Failed to create project branch");

        let job = repository
            .create_indexing_job(&tenant_id, "test_repo", "main", None)
            .await
            .expect("Failed to create job");
        let job_id = job.job_id;

        // Push 5 files to queue
        for i in 0..5 {
            repository
                .enqueue_file(
                    &job_id,
                    &tenant_id,
                    "test_repo",
                    "main",
                    &format!("file_{i}.rs"),
                    &format!("content {i}"),
                    &format!("hash_{i}"),
                )
                .await
                .expect("Failed to enqueue");
        }

        // Verify queue has 5 files
        let depth = repository
            .get_queue_depth(&job_id)
            .await
            .expect("Failed to get depth");
        assert_eq!(depth, 5, "Should have 5 queued files");

        // Process 2 files (simulate partial work before crash)
        repository.dequeue_file().await.expect("Should dequeue");
        repository.dequeue_file().await.expect("Should dequeue");

        // SIMULATE CRASH: Drop everything, create new repository instance
        drop(repository);

        // "Restart" - Create new repository connection (simulates app restart)
        let repository_after_crash = create_test_repository().await;

        // Verify queue still has 3 remaining files (crash recovery!)
        let depth_after_crash = repository_after_crash
            .get_queue_depth(&job_id)
            .await
            .expect("Failed to get depth after crash");

        assert_eq!(
            depth_after_crash, 3,
            "Queue should survive crash with 3 remaining files"
        );

        // Process remaining files
        for _ in 0..3 {
            let result = repository_after_crash.dequeue_file().await;
            assert!(
                result.is_ok() && result.unwrap().is_some(),
                "Should dequeue remaining files"
            );
        }

        // Queue should now be empty
        let final_depth = repository_after_crash
            .get_queue_depth(&job_id)
            .await
            .expect("Failed to get final depth");

        assert_eq!(
            final_depth, 0,
            "Queue should be empty after processing all files"
        );
    })
}
