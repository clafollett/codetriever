//! Git repository detection and normalization

use anyhow::{Context, Result};
use chrono::{TimeZone, Utc};
use git2::{Repository, Status};
use std::path::{Path, PathBuf};

use crate::models::RepositoryContext;

/// Repository identity tuple: (`repository_id`, `optional_url`)
type RepositoryIdentity = (String, Option<String>);

impl RepositoryContext {
    /// Detect repository context from a given path
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path is not within a git repository
    /// - Repository working directory cannot be determined
    /// - Repository HEAD reference cannot be accessed
    /// - Git object access fails due to corruption or permissions
    /// - Repository status check fails
    pub fn detect(path: &Path) -> Result<Self> {
        // Find the repository root
        let repo = Repository::discover(path)
            .context("Not a git repository - codetriever requires git initialization")?;

        let root_path = repo
            .workdir()
            .context("Could not get repository working directory")?
            .to_path_buf();

        // Get current branch
        let head = repo.head().context("Could not get repository HEAD")?;

        let branch = if head.is_branch() {
            head.shorthand().unwrap_or("HEAD").to_string()
        } else {
            // Detached HEAD state
            "HEAD".to_string()
        };

        // Get commit information
        let (commit_sha, commit_message, commit_date, author) =
            head.peel_to_commit()
                .map_or((None, None, None, None), |commit| {
                    let sha = commit.id().to_string();
                    let message = commit.message().map(std::string::ToString::to_string);

                    let timestamp = commit.time();
                    let commit_date = Utc.timestamp_opt(timestamp.seconds(), 0).single();

                    let author = commit.author().name().map(std::string::ToString::to_string);

                    (Some(sha), message, commit_date, author)
                });

        // Check if working directory is dirty
        let is_dirty = repo
            .statuses(None)?
            .iter()
            .any(|s| s.status() != Status::CURRENT);

        // Get repository ID from remote or fallback
        let (repository_id, repository_url) = Self::get_repository_identity(&repo);

        Ok(Self {
            repository_id,
            repository_url,
            branch,
            commit_sha,
            commit_message,
            commit_date,
            author,
            is_dirty,
            root_path,
        })
    }

    /// Get repository identity from Git remote or generate fallback
    fn get_repository_identity(repo: &Repository) -> RepositoryIdentity {
        // Try to get origin remote
        if let Ok(origin) = repo.find_remote("origin")
            && let Some(url) = origin.url()
        {
            let normalized = Self::normalize_git_url(url);
            return (normalized, Some(url.to_string()));
        }

        // Try upstream remote as fallback
        if let Ok(upstream) = repo.find_remote("upstream")
            && let Some(url) = upstream.url()
        {
            let normalized = Self::normalize_git_url(url);
            return (normalized, Some(url.to_string()));
        }

        // Fallback to directory name + username
        let dir_name = repo
            .workdir()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        let user = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "local".to_string());

        let id = format!("local/{user}/{dir_name}");
        (id, None)
    }

    /// Normalize various Git URL formats to a consistent ID
    pub fn normalize_git_url(url: &str) -> String {
        let mut normalized = url.to_lowercase();

        // First remove any authentication info (username:password@)
        // Look for protocol:// then user:pass@ pattern
        if (normalized.starts_with("https://") || normalized.starts_with("http://"))
            && let Some(proto_end) = normalized.find("://")
        {
            // Safe: we found "://" at proto_end, so proto_end + 3 is guaranteed in bounds
            #[allow(clippy::arithmetic_side_effects)]
            let after_proto = &normalized[proto_end + 3..];
            if let Some(at_pos) = after_proto.find('@') {
                // Check if there's a : before @ (indicating auth)
                if after_proto[..at_pos].contains(':') {
                    // Reconstruct without auth
                    // Safe: we found '@' at at_pos, and "://" at proto_end, so both additions are in bounds
                    #[allow(clippy::arithmetic_side_effects)]
                    {
                        normalized = format!(
                            "{}{}",
                            &normalized[..proto_end + 3],
                            &after_proto[at_pos + 1..]
                        );
                    }
                }
            }
        }

        // Remove protocol prefixes
        normalized = normalized
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("git://")
            .trim_start_matches("ssh://")
            .trim_start_matches("git@")
            .to_string();

        // Convert SSH format to path format (git@github.com:user/repo -> github.com/user/repo)
        if let Some(colon_pos) = normalized.find(':')
            && !normalized[..colon_pos].contains('/')
        {
            // This is likely SSH format
            normalized.replace_range(colon_pos..=colon_pos, "/");
        }

        // Remove .git suffix
        normalized = normalized.trim_end_matches(".git").to_string();

        normalized
    }

    /// Convert an absolute path to a relative path from repository root
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The absolute path is not within the repository root directory
    pub fn relativize_path(&self, absolute: &Path) -> Result<String> {
        let relative = absolute.strip_prefix(&self.root_path).with_context(|| {
            format!(
                "Path {} is not within repository root {}",
                absolute.display(),
                self.root_path.display()
            )
        })?;

        // Convert to forward slashes for consistency
        Ok(relative.to_string_lossy().replace('\\', "/"))
    }

    /// Convert a relative path to absolute within the repository
    pub fn absolutize_path(&self, relative: &str) -> PathBuf {
        self.root_path.join(relative)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_git_urls() {
        let cases = vec![
            ("https://github.com/user/repo.git", "github.com/user/repo"),
            ("git@github.com:user/repo.git", "github.com/user/repo"),
            ("ssh://git@github.com/user/repo.git", "github.com/user/repo"),
            (
                "https://gitlab.com/org/project.git",
                "gitlab.com/org/project",
            ),
            ("git@bitbucket.org:team/repo.git", "bitbucket.org/team/repo"),
            ("HTTP://GITHUB.COM/USER/REPO", "github.com/user/repo"),
            (
                "https://user:pass@github.com/user/repo.git",
                "github.com/user/repo",
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(
                RepositoryContext::normalize_git_url(input),
                expected,
                "Failed for input: {input}"
            );
        }
    }
}
