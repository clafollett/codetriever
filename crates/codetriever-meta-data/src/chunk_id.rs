//! Deterministic chunk ID generation

use sha2::{Digest, Sha256};
use uuid::{Uuid, uuid};

/// Namespace UUID for Codetriever chunk IDs (randomly generated once)
/// This ensures our UUIDs don't collide with other systems
const CODETRIEVER_NAMESPACE: Uuid = uuid!("a8f5c3e2-7b9d-4f2a-9e1c-3d5a7b9f1e3c");

/// Generate a deterministic chunk ID based on byte ranges
///
/// The ID is stable for a given combination of:
/// - Repository ID
/// - Branch
/// - File path
/// - Generation
/// - Byte range (start and end)
///
/// Using byte ranges instead of chunk index ensures stability
/// even if the tokenizer or chunking algorithm changes
///
/// Returns a UUID v5 which is deterministic based on the input
pub fn generate_chunk_id(
    repository_id: &str,
    branch: &str,
    file_path: &str,
    generation: i64,
    byte_start: usize,
    byte_end: usize,
) -> Uuid {
    // Create a unique string from all components
    let data = format!("{repository_id}:{branch}:{file_path}:{generation}:{byte_start}:{byte_end}");

    // Generate UUID v5 (deterministic) from namespace and data
    Uuid::new_v5(&CODETRIEVER_NAMESPACE, data.as_bytes())
}

/// Generate a content hash for a file
pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_id_deterministic() {
        let id1 = generate_chunk_id("github.com/user/repo", "main", "src/main.rs", 1, 0, 100);
        let id2 = generate_chunk_id("github.com/user/repo", "main", "src/main.rs", 1, 0, 100);

        assert_eq!(id1, id2, "Same inputs should produce same chunk ID");

        // Verify it's a valid UUID
        assert_eq!(id1.get_version(), Some(uuid::Version::Sha1));
    }

    #[test]
    fn test_chunk_id_unique() {
        let id1 = generate_chunk_id("github.com/user/repo", "main", "src/main.rs", 1, 0, 100);

        let id2 = generate_chunk_id(
            "github.com/user/repo",
            "main",
            "src/main.rs",
            1,
            100, // Different byte start
            200,
        );

        let id3 = generate_chunk_id(
            "github.com/user/repo",
            "main",
            "src/main.rs",
            2, // Different generation
            0,
            100,
        );

        let id4 = generate_chunk_id(
            "github.com/user/repo",
            "feature", // Different branch
            "src/main.rs",
            1,
            0,
            100,
        );

        assert_ne!(id1, id2, "Different byte range should produce different ID");
        assert_ne!(id1, id3, "Different generation should produce different ID");
        assert_ne!(id1, id4, "Different branch should produce different ID");
    }

    #[test]
    fn test_content_hash() {
        let content = "fn main() {\n    println!(\"Hello, world!\");\n}";
        let hash1 = hash_content(content);
        let hash2 = hash_content(content);

        assert_eq!(hash1, hash2, "Same content should produce same hash");

        let different = "fn main() {}";
        let hash3 = hash_content(different);

        assert_ne!(
            hash1, hash3,
            "Different content should produce different hash"
        );
    }
}
