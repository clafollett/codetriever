//! Deterministic chunk ID generation

use sha2::{Digest, Sha256};

/// Generate a deterministic chunk ID based on all identifying components
///
/// The ID is stable for a given combination of:
/// - Repository ID
/// - Branch
/// - File path
/// - Generation
/// - Chunk index within the file
pub fn generate_chunk_id(
    repository_id: &str,
    branch: &str,
    file_path: &str,
    generation: i64,
    chunk_index: u32,
) -> String {
    let mut hasher = Sha256::new();

    // Add each component with a separator
    hasher.update(repository_id.as_bytes());
    hasher.update(b":");
    hasher.update(branch.as_bytes());
    hasher.update(b":");
    hasher.update(file_path.as_bytes());
    hasher.update(b":");
    hasher.update(generation.to_le_bytes());
    hasher.update(b":");
    hasher.update(chunk_index.to_le_bytes());

    // Return hex-encoded hash
    format!("{:x}", hasher.finalize())
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
        let id1 = generate_chunk_id("github.com/user/repo", "main", "src/main.rs", 1, 0);

        let id2 = generate_chunk_id("github.com/user/repo", "main", "src/main.rs", 1, 0);

        assert_eq!(id1, id2, "Same inputs should produce same chunk ID");
    }

    #[test]
    fn test_chunk_id_unique() {
        let id1 = generate_chunk_id("github.com/user/repo", "main", "src/main.rs", 1, 0);

        let id2 = generate_chunk_id(
            "github.com/user/repo",
            "main",
            "src/main.rs",
            1,
            1, // Different chunk index
        );

        let id3 = generate_chunk_id(
            "github.com/user/repo",
            "main",
            "src/main.rs",
            2, // Different generation
            0,
        );

        let id4 = generate_chunk_id(
            "github.com/user/repo",
            "feature", // Different branch
            "src/main.rs",
            1,
            0,
        );

        assert_ne!(
            id1, id2,
            "Different chunk index should produce different ID"
        );
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
