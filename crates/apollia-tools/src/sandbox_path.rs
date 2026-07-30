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
/// The sandbox root is canonicalized at creation time. Every resolved path has
/// its existing prefix canonicalized and re-checked against the root, so a
/// symlink is followed only while it stays inside the sandbox and any symlink
/// escaping the root is rejected.
#[derive(Debug, Clone)]
pub struct SandboxRoot {
    /// Canonicalized absolute path to the sandbox root directory.
    canonical: PathBuf,
}

impl SandboxRoot {
    /// Create and canonicalize the sandbox root directory.
    ///
    /// Creates the directory (and parents) if it doesn't exist.
    /// Canonicalizes the root path itself to resolve symlinks and normalize.
    /// Per-request paths are resolved and symlink-checked separately in
    /// [`SandboxRoot::resolve`].
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
    /// Validates that the path remains within the sandbox boundary after
    /// lexical normalization, then canonicalizes the resolved target's existing
    /// prefix and re-checks it against the root. A symlink is therefore
    /// followed only while its real target stays under the root; a symlink
    /// pointing outside is rejected. When the target does not exist yet
    /// (e.g. a fresh `file_write`), the longest existing ancestor is
    /// canonicalized and the not-yet-created tail is re-appended.
    ///
    /// Absolute paths are accepted if they point under the canonical root
    /// (e.g. `/Users/alice/docs` with root `/Users/alice`). Absolute paths
    /// outside the root (e.g. `/etc/passwd`) are rejected.
    ///
    /// # Rejections
    ///
    /// - Path traversal attempts (e.g. "../../etc/passwd")
    /// - Any path that would resolve outside the sandbox root
    /// - Symlinks whose real target escapes the sandbox root
    ///
    /// # Errors
    ///
    /// Returns `SandboxPathError::SandboxViolation` if the path is invalid or
    /// attempts to escape the sandbox.
    pub fn resolve(&self, relative_path: &str) -> Result<PathBuf, SandboxPathError> {
        // Expand a leading `~` (or `~/`) to $HOME. Without this, `Path::new("~")`
        // treats `~` as a literal directory name and the file ends up under
        // `<sandbox>/~/...`, so agents looking for `~/Documents/foo.md` find nothing.
        // The shell expands tilde, but file_write/file_read receive the raw
        // string. Any tilde elsewhere in the path is left alone (only the
        // leading shorthand is shell-portable).
        let expanded = expand_leading_tilde(relative_path);
        let path = Path::new(expanded.as_ref());

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

        self.contain_within_root(&resolved, relative_path)
    }

    /// Canonicalize the existing portion of `resolved` and re-check confinement.
    ///
    /// Walks up to the longest existing ancestor, canonicalizes it (resolving
    /// every symlink), and verifies the real path still starts with the
    /// canonical root. The not-yet-existing tail is re-appended so callers that
    /// create files (`file_write`) keep working. Any failure to confirm
    /// confinement is treated as an escape (fail-closed).
    fn contain_within_root(
        &self,
        resolved: &Path,
        original: &str,
    ) -> Result<PathBuf, SandboxPathError> {
        let violation = || SandboxPathError::SandboxViolation {
            path: original.to_string(),
        };

        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        let mut probe = resolved.to_path_buf();

        let real_prefix = loop {
            match probe.canonicalize() {
                Ok(canonical) => break canonical,
                Err(_) => {
                    let name = probe.file_name().ok_or_else(violation)?.to_os_string();
                    tail.push(name);
                    if !probe.pop() {
                        return Err(violation());
                    }
                }
            }
        };

        if !real_prefix.starts_with(&self.canonical) {
            return Err(violation());
        }

        let mut full = real_prefix;
        for name in tail.into_iter().rev() {
            full.push(name);
        }
        Ok(full)
    }

    /// Return a reference to the canonical sandbox root path.
    pub fn path(&self) -> &Path {
        &self.canonical
    }
}

/// Normalize a path by resolving `.` and `..` components without touching the filesystem.
///
/// This function processes path components:
/// - `Component::Normal`: added to result
/// - `Component::CurDir` (`.`): skipped
/// - `Component::ParentDir` (`..`): removes last normal component if present, or adds `..` if none
/// - `Component::RootDir`, `Component::Prefix`: preserved as-is
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

/// Expand a leading `~` shorthand to `$HOME`.
///
/// Only the *leading* tilde is expanded: `~/x` and `~` map to `$HOME/x` and
/// `$HOME` respectively. A bare `~user` form is *not* supported (no passwd
/// lookup) and is left unchanged so the sandbox check rejects it cleanly.
/// If `$HOME` is unset the input is returned untouched, which makes the
/// existing sandbox boundary check the authoritative gate.
fn expand_leading_tilde(input: &str) -> std::borrow::Cow<'_, str> {
    if input != "~" && !input.starts_with("~/") {
        return std::borrow::Cow::Borrowed(input);
    }
    let Some(home) = apollia_core::paths::home_dir() else {
        return std::borrow::Cow::Borrowed(input);
    };
    let home_str = home.to_string_lossy();
    if input == "~" {
        return std::borrow::Cow::Owned(home_str.into_owned());
    }
    // input.starts_with("~/"): strip the `~` and join with HOME.
    let rest = &input[2..];
    let mut buf = String::with_capacity(home_str.len() + 1 + rest.len());
    buf.push_str(&home_str);
    if !home_str.ends_with('/') {
        buf.push('/');
    }
    buf.push_str(rest);
    std::borrow::Cow::Owned(buf)
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
    fn expand_leading_tilde_resolves_to_home() {
        // GIVEN: HOME is set
        let home = apollia_core::paths::home_string().expect("a test host has a home directory");

        // WHEN/THEN: bare `~` and `~/...` map to HOME
        assert_eq!(expand_leading_tilde("~").as_ref(), home);
        let expanded = expand_leading_tilde("~/Documents/file.md");
        assert_eq!(
            expanded.as_ref(),
            format!("{}/Documents/file.md", home.trim_end_matches('/'))
        );
        // AND a tilde mid-path is left alone (only leading shorthand is shell-portable)
        assert_eq!(expand_leading_tilde("foo/~/bar").as_ref(), "foo/~/bar");
        // AND `~user` is unchanged (no passwd lookup; sandbox check rejects later)
        assert_eq!(expand_leading_tilde("~root/x").as_ref(), "~root/x");
    }

    #[test]
    fn resolve_expands_tilde_when_target_is_under_root() {
        // GIVEN: a sandbox rooted at $HOME
        let home = apollia_core::paths::home_string().expect("a test host has a home directory");
        let sandbox = SandboxRoot::new(PathBuf::from(&home)).expect("sandbox under HOME");

        // WHEN: resolving `~/Documents/foo.md`
        let resolved = sandbox
            .resolve("~/Documents/foo.md")
            .expect("tilde path should resolve under sandbox");

        // THEN: it lands under HOME directly, no literal `~` directory
        assert!(resolved.starts_with(sandbox.path()));
        assert!(resolved.ends_with("Documents/foo.md"));
        assert!(
            !resolved.to_string_lossy().contains("/~/"),
            "resolved path must not contain a literal '~' segment, got {resolved:?}"
        );
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
    fn resolve_accepts_absolute_path_under_root() {
        // GIVEN: a valid SandboxRoot
        let temp_dir = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(temp_dir.clone()).expect("Failed to create sandbox");
        let canonical = sandbox.path().to_path_buf();

        // WHEN: resolving an absolute path that points inside the sandbox root
        let inside = canonical.join("src/main.rs");
        let result = sandbox.resolve(inside.to_str().expect("valid utf-8"));

        // THEN: the path is accepted and normalized under the canonical root
        assert!(result.is_ok(), "expected Ok, got {:?}", result);
        let resolved = result.expect("Resolution failed");
        assert!(resolved.starts_with(&canonical));
        assert!(resolved.ends_with("src/main.rs"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn resolve_rejects_absolute_path_outside_root() {
        // GIVEN: a valid SandboxRoot in a temp directory
        let temp_dir = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(temp_dir.clone()).expect("Failed to create sandbox");

        // WHEN: resolving an absolute path that lies outside the sandbox root
        let result = sandbox.resolve("/etc/passwd");

        // THEN: SandboxViolation error is returned (via the starts_with check)
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

    #[cfg(unix)]
    #[test]
    fn resolve_rejects_symlink_escaping_root() {
        // GIVEN: a sandbox containing a symlink that points outside the root
        let temp_dir = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(temp_dir.clone()).expect("Failed to create sandbox");
        let outside = std::env::temp_dir().join(format!("apollia-out-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&outside).expect("create outside dir");
        std::fs::write(outside.join("secret.txt"), b"top secret").expect("seed outside file");
        std::os::unix::fs::symlink(&outside, sandbox.path().join("link"))
            .expect("create escaping symlink");

        // WHEN: resolving a path that traverses the escaping symlink
        let via_symlink = sandbox.resolve("link/secret.txt");
        let bare_symlink = sandbox.resolve("link");

        // THEN: both are rejected as sandbox violations
        assert!(
            matches!(via_symlink, Err(SandboxPathError::SandboxViolation { .. })),
            "escaping symlink target must be rejected, got {via_symlink:?}"
        );
        assert!(
            matches!(bare_symlink, Err(SandboxPathError::SandboxViolation { .. })),
            "escaping symlink itself must be rejected, got {bare_symlink:?}"
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_allows_internal_symlink() {
        // GIVEN: a sandbox with a real subdir and a symlink to it, both inside root
        let temp_dir = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(temp_dir.clone()).expect("Failed to create sandbox");
        std::fs::create_dir_all(sandbox.path().join("real")).expect("create real dir");
        std::fs::write(sandbox.path().join("real/f.txt"), b"inside").expect("seed inside file");
        std::os::unix::fs::symlink(sandbox.path().join("real"), sandbox.path().join("link"))
            .expect("create internal symlink");

        // WHEN: resolving a path that traverses the internal symlink
        let resolved = sandbox
            .resolve("link/f.txt")
            .expect("internal symlink must resolve");

        // THEN: it lands on the real file under the root, with the symlink resolved away
        assert!(resolved.starts_with(sandbox.path()));
        assert!(resolved.ends_with("real/f.txt"));
        assert!(
            !resolved.to_string_lossy().contains("/link/"),
            "resolved path must not contain the symlink segment, got {resolved:?}"
        );

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
