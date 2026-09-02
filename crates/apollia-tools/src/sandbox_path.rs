//! Sandbox path validation for file operations.
//!
//! Ensures all file paths remain within the agent's designated sandbox directory,
//! preventing path traversal attacks and unauthorized filesystem access.

use std::path::{Component, Path, PathBuf};
use thiserror::Error;

/// Error type for sandbox path validation.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SandboxPathError {
    /// Path attempts to escape the sandbox root.
    #[error(
        "path '{path}' is outside every trusted root; add it to [filesystem] \
         trusted_paths in apollia.toml to reach it"
    )]
    SandboxViolation { path: String },

    /// Failed to initialize sandbox root directory.
    #[error("I/O error creating sandbox root '{path}': {cause}")]
    InitFailed { path: String, cause: String },
}

/// What a file tool is allowed to reach: one anchor, and any number of
/// additional roots.
///
/// The anchor is where a relative path lands, so there is exactly one and it is
/// created if missing. The additional roots only widen what an absolute path may
/// reach; they are never created, because naming `/Volumes/work` in a setting is
/// a statement about a disk that may not be mounted, not a request to make one.
///
/// `From<PathBuf>` and `From<Vec<PathBuf>>` mean a caller with a single root
/// writes what it always wrote. An empty vector is rejected at construction
/// rather than silently treated as no confinement.
#[derive(Debug, Clone)]
pub struct SandboxSpec {
    anchor: PathBuf,
    extra: Vec<PathBuf>,
}

impl From<PathBuf> for SandboxSpec {
    fn from(anchor: PathBuf) -> Self {
        Self {
            anchor,
            extra: Vec::new(),
        }
    }
}

impl From<&Path> for SandboxSpec {
    fn from(anchor: &Path) -> Self {
        Self::from(anchor.to_path_buf())
    }
}

impl From<Vec<PathBuf>> for SandboxSpec {
    /// The first entry is the anchor; an empty vector yields an empty anchor,
    /// which [`SandboxRoot::new`] then refuses.
    fn from(mut roots: Vec<PathBuf>) -> Self {
        if roots.is_empty() {
            return Self {
                anchor: PathBuf::new(),
                extra: Vec::new(),
            };
        }
        let anchor = roots.remove(0);
        Self {
            anchor,
            extra: roots,
        }
    }
}

/// Validated sandbox roots.
///
/// Created once per tool instance. All path operations validate against these
/// roots. Each is canonicalized at creation time. Every resolved path has its
/// existing prefix canonicalized and re-checked against them, so a symlink is
/// followed only while it stays inside one root and any symlink escaping all of
/// them is rejected.
#[derive(Debug, Clone)]
pub struct SandboxRoot {
    /// Canonicalized absolute path of the anchor: where a relative path lands.
    /// Always the first entry of `roots`.
    canonical: PathBuf,
    /// Every root a resolved path may sit under, canonicalized where the
    /// directory exists.
    roots: Vec<PathBuf>,
}

impl SandboxRoot {
    /// Create and canonicalize the sandbox roots.
    ///
    /// The anchor is created (with parents) if it does not exist, then
    /// canonicalized to resolve symlinks and normalize. Additional roots are
    /// canonicalized when they exist and kept as written when they do not: a
    /// root that is not there yet cannot be reached, and comparing an
    /// uncanonicalized form against a canonicalized real path fails closed
    /// (macOS `/var` against `/private/var`) rather than opening a hole.
    ///
    /// Per-request paths are resolved and symlink-checked separately in
    /// [`SandboxRoot::resolve`].
    ///
    /// # Errors
    ///
    /// Returns `SandboxPathError::InitFailed` if the anchor cannot be created
    /// or canonicalized, including the case of an empty root list.
    pub fn new(spec: impl Into<SandboxSpec>) -> Result<Self, SandboxPathError> {
        let SandboxSpec { anchor, extra } = spec.into();

        if anchor.as_os_str().is_empty() {
            return Err(SandboxPathError::InitFailed {
                path: String::new(),
                cause: "no sandbox root given".to_string(),
            });
        }

        std::fs::create_dir_all(&anchor).map_err(|e| SandboxPathError::InitFailed {
            path: anchor.display().to_string(),
            cause: e.to_string(),
        })?;

        let canonical = anchor
            .canonicalize()
            .map_err(|e| SandboxPathError::InitFailed {
                path: anchor.display().to_string(),
                cause: e.to_string(),
            })?;

        let mut roots = vec![canonical.clone()];
        for root in extra {
            if root.as_os_str().is_empty() {
                continue;
            }
            roots.push(root.canonicalize().unwrap_or(root));
        }

        Ok(Self { canonical, roots })
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
    /// Absolute paths are accepted if their canonical form stays under one of
    /// the roots (e.g. `/Users/alice/docs` with root `/Users/alice`).
    /// The comparison happens after canonicalizing the path's existing
    /// prefix, so an uncanonicalized alias of an in-root path is accepted
    /// (macOS `/var/...` for a root stored as `/private/var/...`, Windows
    /// `C:\...` for a root stored in verbatim `\\?\C:\...` form). Absolute
    /// paths outside the root (e.g. `/etc/passwd`) are rejected.
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

        // An absolute input (RootDir, or a Windows drive Prefix even without a
        // RootDir) must not be joined onto the root: `Path::join` would replace
        // the root, and a lexical `starts_with` against the canonical root is
        // meaningless for a path the caller spelled in uncanonicalized form
        // (macOS `/var` vs `/private/var`, Windows verbatim `\\?\C:\`). It goes
        // as-is to the canonicalize-then-compare check below, which is the only
        // comparison valid across those aliases and stays fail-closed.
        let is_absolute = normalized.is_absolute()
            || matches!(normalized.components().next(), Some(Component::Prefix(_)));
        let resolved = if is_absolute {
            normalized
        } else {
            self.canonical.join(&normalized)
        };

        self.contain_within_root(&resolved, relative_path)
    }

    /// Canonicalize the existing portion of `resolved` and re-check confinement.
    ///
    /// Walks up to the longest existing ancestor, canonicalizes it (resolving
    /// every symlink), and verifies the real path still starts with one of the
    /// roots. The not-yet-existing tail is re-appended so callers that create
    /// files (`file_write`) keep working. Any failure to confirm confinement is
    /// treated as an escape (fail-closed).
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

        if !self.roots.iter().any(|root| real_prefix.starts_with(root)) {
            return Err(violation());
        }

        let mut full = real_prefix;
        for name in tail.into_iter().rev() {
            full.push(name);
        }
        Ok(full)
    }

    /// Return the anchor: the canonical root a relative path lands under.
    pub fn path(&self) -> &Path {
        &self.canonical
    }

    /// Return every root a resolved path may sit under, anchor first.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
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

    /// Two throwaway directories, both created, plus a third path that is never
    /// created. Returned in that order.
    fn two_roots_and_a_ghost() -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let anchor = base.join("anchor");
        let second = base.join("second");
        std::fs::create_dir_all(&anchor).expect("anchor");
        std::fs::create_dir_all(&second).expect("second");
        (anchor, second, base.join("never-created"))
    }

    #[test]
    fn resolve_accepts_an_absolute_path_under_a_second_root() {
        // GIVEN two roots, the second being the kind of path an operator names
        // in `[filesystem] trusted_paths` (a mounted volume, /opt, a work disk)
        let (anchor, second, _) = two_roots_and_a_ghost();
        std::fs::write(second.join("report.csv"), b"x").expect("write");
        let sandbox =
            SandboxRoot::new(vec![anchor.clone(), second.clone()]).expect("two-root sandbox");

        // WHEN an absolute path under the second root is resolved
        let resolved = sandbox
            .resolve(&second.join("report.csv").display().to_string())
            .expect("a path under a trusted root resolves");

        // THEN it is accepted. A single root is what made an agent whose work
        // sits outside the home directory unusable.
        assert!(resolved.ends_with("report.csv"));

        let _ = std::fs::remove_dir_all(anchor.parent().expect("base"));
    }

    #[test]
    fn resolve_still_rejects_a_path_under_no_root() {
        // GIVEN the same two roots
        let (anchor, second, _) = two_roots_and_a_ghost();
        let sandbox = SandboxRoot::new(vec![anchor.clone(), second]).expect("two-root sandbox");

        // WHEN a path under neither is resolved
        let result = sandbox.resolve("/etc/passwd");

        // THEN it is refused. Widening to a list is not the same as opening up.
        assert!(matches!(
            result,
            Err(SandboxPathError::SandboxViolation { .. })
        ));

        let _ = std::fs::remove_dir_all(anchor.parent().expect("base"));
    }

    #[test]
    fn a_relative_path_lands_under_the_anchor_not_a_later_root() {
        // GIVEN two roots
        let (anchor, second, _) = two_roots_and_a_ghost();
        let sandbox =
            SandboxRoot::new(vec![anchor.clone(), second.clone()]).expect("two-root sandbox");

        // WHEN a relative path is resolved
        let resolved = sandbox.resolve("notes.md").expect("relative resolves");

        // THEN it lands under the first root. The anchor is a single directory
        // by construction: a relative path with several candidate roots would
        // otherwise have no defined meaning.
        let canonical_anchor = anchor.canonicalize().expect("canonical anchor");
        assert!(
            resolved.starts_with(&canonical_anchor),
            "{resolved:?} should sit under {canonical_anchor:?}"
        );

        let _ = std::fs::remove_dir_all(anchor.parent().expect("base"));
    }

    #[test]
    fn an_empty_root_list_is_refused_at_construction() {
        // GIVEN no root at all
        // WHEN a sandbox is built from it
        let result = SandboxRoot::new(Vec::<PathBuf>::new());

        // THEN construction fails for the stated reason rather than yielding a
        // sandbox that confines nothing: an empty anchor is a prefix of every
        // path on the machine. Asserting the cause matters, because creating an
        // empty directory also fails, which would make this pass with the guard
        // removed.
        match result {
            Err(SandboxPathError::InitFailed { cause, .. }) => {
                assert_eq!(cause, "no sandbox root given", "wrong reason: {cause}");
            }
            other => panic!("expected InitFailed, got {other:?}"),
        }
    }

    #[test]
    fn an_extra_root_that_does_not_exist_opens_nothing() {
        // GIVEN an anchor plus a trusted root that is not there, the shape of an
        // unmounted volume named in the configuration
        let (anchor, _, ghost) = two_roots_and_a_ghost();
        let sandbox = SandboxRoot::new(vec![anchor.clone(), ghost.clone()]).expect("sandbox");

        // WHEN a path outside the anchor is resolved
        let result = sandbox.resolve("/etc/passwd");

        // THEN it is still refused, and the missing root did not become a
        // wildcard on its way through canonicalization.
        assert!(matches!(
            result,
            Err(SandboxPathError::SandboxViolation { .. })
        ));

        let _ = std::fs::remove_dir_all(anchor.parent().expect("base"));
    }

    #[test]
    fn a_symlink_from_the_anchor_into_a_second_root_is_followed() {
        // GIVEN a symlink inside the anchor pointing at the second root, which
        // the operator has declared trusted
        let (anchor, second, _) = two_roots_and_a_ghost();
        std::fs::write(second.join("data.txt"), b"x").expect("write");
        let link = anchor.join("shortcut");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&second, &link).expect("symlink");
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(&second, &link).expect("symlink");
        let sandbox = SandboxRoot::new(vec![anchor.clone(), second]).expect("sandbox");

        // WHEN the link is walked
        let resolved = sandbox.resolve("shortcut/data.txt");

        // THEN it resolves: the real target sits under a declared root. The
        // symlink check refuses an escape from every root, not from the anchor.
        assert!(resolved.is_ok(), "{resolved:?}");

        let _ = std::fs::remove_dir_all(anchor.parent().expect("base"));
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
    fn resolve_accepts_uncanonicalized_absolute_path_under_root() {
        // GIVEN: a sandbox created from the raw temp path. On macOS the raw
        // path is `/var/folders/...` while the stored canonical root is
        // `/private/var/...`, which is exactly the aliasing that used to be
        // rejected. A file exists inside the sandbox.
        let raw_root = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(raw_root.clone()).expect("Failed to create sandbox");
        std::fs::create_dir_all(raw_root.join("sub")).expect("create sub dir");
        std::fs::write(raw_root.join("sub/f.txt"), b"inside").expect("seed file");

        // WHEN: resolving the raw (uncanonicalized) absolute path of that file
        let raw_input = raw_root.join("sub/f.txt");
        let result = sandbox.resolve(raw_input.to_str().expect("valid utf-8"));

        // THEN: the path is accepted and lands under the canonical root
        let resolved = result.expect("in-sandbox absolute path must resolve");
        assert!(resolved.starts_with(sandbox.path()));
        assert!(resolved.ends_with("sub/f.txt"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&raw_root);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_accepts_absolute_path_via_symlinked_root() {
        // GIVEN: a real directory R and a symlink L -> R, with the sandbox
        // created from L (canonical root is therefore R). This reproduces the
        // /var -> /private/var class on hosts where temp_dir is already canonical.
        let real = std::env::temp_dir().join(format!("apollia-real-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&real).expect("create real root");
        let link = std::env::temp_dir().join(format!("apollia-link-{}", uuid::Uuid::new_v4()));
        std::os::unix::fs::symlink(&real, &link).expect("create root symlink");
        let sandbox = SandboxRoot::new(link.clone()).expect("Failed to create sandbox");
        std::fs::write(real.join("f.txt"), b"inside").expect("seed file");

        // WHEN: resolving the absolute path spelled through the symlinked alias
        let via_link = link.join("f.txt");
        let result = sandbox.resolve(via_link.to_str().expect("valid utf-8"));

        // THEN: the alias is accepted and resolves to the real in-root file
        let resolved = result.expect("aliased in-sandbox path must resolve");
        let canonical_real = real.canonicalize().expect("canonicalize real root");
        assert!(resolved.starts_with(&canonical_real));
        assert!(resolved.ends_with("f.txt"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&real);
        let _ = std::fs::remove_file(&link);
    }

    #[test]
    fn resolve_accepts_uncanonicalized_absolute_path_to_nonexistent_leaf() {
        // GIVEN: a sandbox created from the raw temp path, and a target whose
        // tail components do not exist yet (the fresh `file_write` case)
        let raw_root = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(raw_root.clone()).expect("Failed to create sandbox");

        // WHEN: resolving the raw absolute path of the not-yet-created target
        let raw_input = raw_root.join("new/dir/file.txt");
        let result = sandbox.resolve(raw_input.to_str().expect("valid utf-8"));

        // THEN: the existing prefix is canonicalized and the tail re-appended
        let resolved = result.expect("nonexistent in-sandbox target must resolve");
        assert!(resolved.starts_with(sandbox.path()));
        assert!(resolved.ends_with("new/dir/file.txt"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&raw_root);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_rejects_absolute_symlink_escape() {
        // GIVEN: a symlink inside the sandbox pointing outside of it
        let raw_root = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(raw_root.clone()).expect("Failed to create sandbox");
        let outside = std::env::temp_dir().join(format!("apollia-out-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&outside).expect("create outside dir");
        std::fs::write(outside.join("secret.txt"), b"top secret").expect("seed outside file");
        std::os::unix::fs::symlink(&outside, raw_root.join("link"))
            .expect("create escaping symlink");

        // WHEN: resolving the escaping target through its raw absolute path
        let raw_input = raw_root.join("link/secret.txt");
        let result = sandbox.resolve(raw_input.to_str().expect("valid utf-8"));

        // THEN: the escape is rejected (fail-closed is preserved for absolutes)
        assert!(
            matches!(result, Err(SandboxPathError::SandboxViolation { .. })),
            "absolute path through an escaping symlink must be rejected, got {result:?}"
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&raw_root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[cfg(windows)]
    #[test]
    fn resolve_accepts_windows_drive_absolute_path_under_root() {
        // GIVEN: a sandbox whose canonical root is in verbatim form (\\?\C:\...)
        let raw_root = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(raw_root.clone()).expect("Failed to create sandbox");
        std::fs::write(raw_root.join("f.txt"), b"inside").expect("seed file");

        // WHEN: resolving the plain C:\... spelling (no verbatim prefix), as an
        // agent would write it
        let canonical_str = sandbox.path().display().to_string();
        let plain = canonical_str
            .strip_prefix(r"\\?\")
            .unwrap_or(&canonical_str)
            .to_string();
        let result = sandbox.resolve(&format!(r"{plain}\f.txt"));

        // THEN: the plain form is accepted
        let resolved = result.expect("plain drive-absolute in-sandbox path must resolve");
        assert!(resolved.starts_with(sandbox.path()));

        // Cleanup
        let _ = std::fs::remove_dir_all(&raw_root);
    }

    #[cfg(windows)]
    #[test]
    fn resolve_rejects_windows_absolute_path_outside_root() {
        // GIVEN: a sandbox in the temp directory
        let raw_root = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(raw_root.clone()).expect("Failed to create sandbox");

        // WHEN: resolving a system path outside the root
        let result = sandbox.resolve(r"C:\Windows\System32\config");

        // THEN: it is rejected
        assert!(matches!(
            result,
            Err(SandboxPathError::SandboxViolation { .. })
        ));

        // Cleanup
        let _ = std::fs::remove_dir_all(&raw_root);
    }

    #[cfg(windows)]
    #[test]
    fn resolve_never_escapes_on_drive_relative_path() {
        // GIVEN: a sandbox in the temp directory
        let raw_root = std::env::temp_dir().join(format!("apollia-test-{}", uuid::Uuid::new_v4()));
        let sandbox = SandboxRoot::new(raw_root.clone()).expect("Failed to create sandbox");

        // WHEN: resolving a drive-relative path (Prefix without RootDir)
        let result = sandbox.resolve(r"C:sub\f.txt");

        // THEN: it either resolves inside the root or is rejected; it never
        // lands outside the canonical root
        match result {
            Ok(resolved) => assert!(resolved.starts_with(sandbox.path())),
            Err(SandboxPathError::SandboxViolation { .. }) => {}
            Err(other) => panic!("unexpected error kind: {other:?}"),
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&raw_root);
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
