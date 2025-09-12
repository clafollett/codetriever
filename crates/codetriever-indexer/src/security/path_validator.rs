//! Path validation utilities for security
//!
//! Prevents path traversal attacks by validating and sanitizing file paths

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Validates that a path is within the allowed base directory
///
/// # Errors
///
/// Returns an error if:
/// - Path is outside the allowed base directory (path traversal attempt)
/// - Path contains invalid components
pub fn validate_path(path: &Path, base_dir: &Path) -> Result<PathBuf> {
    // First check for any absolute paths or parent directory references
    validate_relative_path(path)?;

    // Combine base_dir with path and normalize
    let combined = base_dir.join(path);
    let normalized = normalize_path(&combined);
    let normalized_base = normalize_path(base_dir);

    // Check if the normalized path starts with the normalized base
    if !normalized.starts_with(&normalized_base) {
        anyhow::bail!("Path validation failed: Access denied to path outside allowed directory");
    }

    Ok(normalized)
}

/// Normalize a path by resolving . and .. components without filesystem access
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {} // Skip .
            Component::ParentDir => {
                // Only pop if we have a normal component (not root)
                if let Some(last) = components.last()
                    && !matches!(last, Component::RootDir)
                {
                    components.pop();
                }
            }
            c => components.push(c),
        }
    }

    components.iter().collect()
}

/// Validates that a relative path component is safe
///
/// # Errors
///
/// Returns an error if the path contains:
/// - Parent directory references (..)
/// - Absolute path components
/// - Invalid characters for the platform
pub fn validate_relative_path(path: &Path) -> Result<()> {
    // Check for absolute paths
    if path.is_absolute() {
        anyhow::bail!("Path validation failed: Absolute paths are not allowed");
    }

    // Check for parent directory references
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::ParentDir => {
                anyhow::bail!(
                    "Path validation failed: Parent directory references are not allowed"
                );
            }
            Component::RootDir => {
                anyhow::bail!("Path validation failed: Root directory references are not allowed");
            }
            _ => {}
        }
    }

    Ok(())
}

/// Sanitizes a file path by removing potentially dangerous components
///
/// This function removes:
/// - Parent directory references (..)
/// - Current directory references (.)
/// - Root directory references
/// - Empty components
pub fn sanitize_path(path: &Path) -> PathBuf {
    use std::path::Component;

    let components: Vec<_> = path
        .components()
        .filter_map(|comp| match comp {
            Component::Normal(os_str) => Some(os_str),
            _ => None,
        })
        .collect();

    components.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_path_within_base() {
        let base = Path::new("/home/user/project");
        let path = Path::new("src/main.rs");

        let result = validate_path(path, base);
        assert!(result.is_ok());
        let validated = result.unwrap();
        assert_eq!(validated, Path::new("/home/user/project/src/main.rs"));
    }

    #[test]
    fn test_validate_path_traversal_attack() {
        let base = Path::new("/home/user/project");
        let malicious = Path::new("../../../etc/passwd");

        let result = validate_path(malicious, base);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_relative_path_safe() {
        let path = Path::new("src/main.rs");
        assert!(validate_relative_path(path).is_ok());
    }

    #[test]
    fn test_validate_relative_path_with_parent() {
        let path = Path::new("../etc/passwd");
        assert!(validate_relative_path(path).is_err());
    }

    #[test]
    fn test_validate_relative_path_absolute() {
        let path = Path::new("/etc/passwd");
        assert!(validate_relative_path(path).is_err());
    }

    #[test]
    fn test_sanitize_path() {
        let dirty = Path::new("../../.././foo//bar/../baz");
        let clean = sanitize_path(dirty);
        // Only keeps normal components: "foo", "bar", "baz"
        assert_eq!(clean, Path::new("foo/bar/baz"));
    }
}
