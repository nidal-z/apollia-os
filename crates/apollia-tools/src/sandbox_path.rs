//! Sandbox path validation for file operations.
//!
//! Ensures all file paths remain within the agent's designated sandbox directory,
//! preventing path traversal attacks and unauthorized filesystem access.

use std::path::{Component, Path, PathBuf};
use thiserror::Error;

/// Error type for sandbox path validation.
#[derive(Debug, Error)]
pub enum SandboxPathError {
    /// Path attempts to escape the sandbox root.
    #[error("sandbox violation: path '{path}' escapes the sandbox root")]
    SandboxViolation { path: String },

    /// Failed to initialize sandbox root directory.
    #[error("I/O error creating sandbox root '{path}': {cause}")]
    InitFailed { path: String, cause: String },
}

/// Validated sandbox root directory.
///
/// Created once per tool instance. All path operations validate against this root.
/// The sandbox root is canonicalized at creation time, and all resolved paths
/// are checked to ensure they remain within the sandbox boundary.
#[derive(Debug, Clone)]
pub struct SandboxRoot {
    /// Canonicalized absolute path to the sandbox root directory.
    canonical: PathBuf,
}

impl SandboxRoot {
    /// Create and canonicalize the sandbox root directory.
    ///
    /// Creates the directory (and parents) if it doesn't exist.
    /// Canonicalizes the path to resolve symlinks and normalize.
    ///
    /// # Errors
    ///
    /// Returns `SandboxPathError::InitFailed` if the directory cannot be created
    /// or canonicalized.
    pub fn new(path: PathBuf) -> Result<Self, SandboxPathError> {
        std::fs::create_dir_all(&path).map_err(|e| SandboxPathError::InitFailed {
            path: path.display().to_string(),
            cause: e.to_string(),
        })?;

        let canonical = path
            .canonicalize()
            .map_err(|e| SandboxPathError::InitFailed {
                path: path.display().to_string(),
                cause: e.to_string(),
            })?;

        Ok(Self { canonical })
    }

    /// Resolve a relative path within the sandbox.
    ///
    /// Validates that the path remains within the sandbox boundary after normalization.
    ///
    /// # Rejections
    ///
    /// - Absolute paths (e.g. "/etc/passwd")
    /// - Path traversal attempts (e.g. "../../etc/passwd")
    /// - Any path that would resolve outside the sandbox root
    ///
    /// # Errors
    ///
    /// Returns `SandboxPathError::SandboxViolation` if the path is invalid or
    /// attempts to escape the sandbox.
    pub fn resolve(&self, relative_path: &str) -> Result<PathBuf, SandboxPathError> {
        let path = Path::new(relative_path);

        if path.is_absolute() {
            return Err(SandboxPathError::SandboxViolation {
                path: relative_path.to_string(),
            });
        }

        let normalized = normalize_path(path);

        // Check if normalized path contains any remaining ParentDir components
        for component in normalized.components() {
            if matches!(component, Component::ParentDir) {
                return Err(SandboxPathError::SandboxViolation {
                    path: relative_path.to_string(),
                });
            }
        }

        let resolved = self.canonical.join(&normalized);

        if !resolved.starts_with(&self.canonical) {
            return Err(SandboxPathError::SandboxViolation {
                path: relative_path.to_string(),
            });
        }

        Ok(resolved)
    }

    /// Return a reference to the canonical sandbox root path.
    pub fn path(&self) -> &Path {
        &self.canonical
    }
}

/// Normalize a path by resolving `.` and `..` components without touching the filesystem.
///
/// This function processes path components:
/// - `Component::Normal` — added to result
/// - `Component::CurDir` (`.`) — skipped
/// - `Component::ParentDir` (`..`) — removes last normal component if present, or adds `..` if none
/// - `Component::RootDir`, `Component::Prefix` — preserved as-is
///
/// The normalized path may still contain `..` components at the start if there are more
/// parent directory references than can be resolved against normal components.
fn normalize_path(path: &Path) -> PathBuf {
    let mut components: Vec<Component> = Vec::new();

    for component in path.components() {
        match component {
            Component::CurDir => {
                // Skip "." components
            }
            Component::ParentDir => {
                // Try to pop the last normal component
                if !components.is_empty() && matches!(components.last(), Some(Component::Normal(_)))
                {
                    components.pop();
                } else {
                    // Cannot resolve, keep the ParentDir
                    components.push(Component::ParentDir);
                }
            }
            Component::Normal(_) | Component::RootDir | Component::Prefix(_) => {
                components.push(component);
            }
        }
    }

    components.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn sandbox_root_new_creates_directory() {
        // GIVEN: a non-existent temp directory path
        let temp_dir = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));

        // WHEN: creating a SandboxRoot
        let result = SandboxRoot::new(temp_dir.clone());

        // THEN: the directory is created and canonicalized
        assert!(result.is_ok());
        assert!(temp_dir.exists());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn resolve_normal_path() {
        // GIVEN: a valid SandboxRoot with a temp directory
        let temp_dir = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(temp_dir.clone()).expect("Failed to create sandbox");

        // WHEN: resolving a normal relative path
        let result = sandbox.resolve("src/main.rs");

        // THEN: the path is resolved correctly
        assert!(result.is_ok());
        let resolved = result.expect("Resolution failed");
        assert!(resolved.starts_with(sandbox.path()));
        assert!(resolved.ends_with("src/main.rs"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn resolve_rejects_parent_traversal() {
        // GIVEN: a valid SandboxRoot
        let temp_dir = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(temp_dir.clone()).expect("Failed to create sandbox");

        // WHEN: resolving a path with parent traversal
        let result = sandbox.resolve("../escape");

        // THEN: SandboxViolation error is returned
        assert!(result.is_err());
        match result.err() {
            Some(SandboxPathError::SandboxViolation { .. }) => {}
            _ => panic!("Expected SandboxViolation error"),
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn resolve_rejects_double_traversal() {
        // GIVEN: a valid SandboxRoot
        let temp_dir = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(temp_dir.clone()).expect("Failed to create sandbox");

        // WHEN: resolving a path with nested parent traversal
        let result = sandbox.resolve("a/../../escape");

        // THEN: SandboxViolation error is returned
        assert!(result.is_err());
        match result.err() {
            Some(SandboxPathError::SandboxViolation { .. }) => {}
            _ => panic!("Expected SandboxViolation error"),
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn resolve_rejects_absolute_path() {
        // GIVEN: a valid SandboxRoot
        let temp_dir = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(temp_dir.clone()).expect("Failed to create sandbox");

        // WHEN: resolving an absolute path
        let result = sandbox.resolve("/etc/passwd");

        // THEN: SandboxViolation error is returned
        assert!(result.is_err());
        match result.err() {
            Some(SandboxPathError::SandboxViolation { .. }) => {}
            _ => panic!("Expected SandboxViolation error"),
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn resolve_handles_dot_component() {
        // GIVEN: a valid SandboxRoot
        let temp_dir = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(temp_dir.clone()).expect("Failed to create sandbox");

        // WHEN: resolving a path with leading dot component
        let result = sandbox.resolve("./src/main.rs");

        // THEN: the path is resolved correctly
        assert!(result.is_ok());
        let resolved = result.expect("Resolution failed");
        assert!(resolved.starts_with(sandbox.path()));
        assert!(resolved.ends_with("src/main.rs"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn resolve_handles_dot_in_middle() {
        // GIVEN: a valid SandboxRoot
        let temp_dir = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(temp_dir.clone()).expect("Failed to create sandbox");

        // WHEN: resolving a path with dot in the middle
        let result = sandbox.resolve("src/./main.rs");

        // THEN: the path is resolved correctly
        assert!(result.is_ok());
        let resolved = result.expect("Resolution failed");
        assert!(resolved.starts_with(sandbox.path()));
        assert!(resolved.ends_with("src/main.rs"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn normalize_path_removes_dot() {
        // GIVEN: a path with dot components
        let path = Path::new("./src/./main.rs");

        // WHEN: normalizing the path
        let normalized = normalize_path(path);

        // THEN: dot components are removed
        assert_eq!(normalized, PathBuf::from("src/main.rs"));
    }

    #[test]
    fn normalize_path_resolves_parent() {
        // GIVEN: a path with parent directory component
        let path = Path::new("src/../lib.rs");

        // WHEN: normalizing the path
        let normalized = normalize_path(path);

        // THEN: parent directory is resolved
        assert_eq!(normalized, PathBuf::from("lib.rs"));
    }

    #[test]
    fn normalize_path_handles_multiple_parents() {
        // GIVEN: a path with multiple parent directory components
        let path = Path::new("a/b/../../c");

        // WHEN: normalizing the path
        let normalized = normalize_path(path);

        // THEN: all parent directories are resolved
        assert_eq!(normalized, PathBuf::from("c"));
    }

    #[test]
    fn normalize_path_allows_escape_attempt() {
        // GIVEN: a path that attempts to escape (more .. than depth)
        let path = Path::new("a/../../escape");

        // WHEN: normalizing the path
        let normalized = normalize_path(path);

        // THEN: normalization completes (validation happens at resolve level)
        assert_eq!(normalized, PathBuf::from("../escape"));
    }

    #[test]
    fn sandbox_root_path_returns_canonical() {
        // GIVEN: a valid SandboxRoot
        let temp_dir = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(temp_dir.clone()).expect("Failed to create sandbox");

        // WHEN: calling path()
        let path = sandbox.path();

        // THEN: the canonical path is returned
        assert!(path.is_absolute());
        assert!(path.exists());

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
