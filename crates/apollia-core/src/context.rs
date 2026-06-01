//! [`ContextProvider`] trait and types for collecting situational context.
//!
//! Key distinction from agent memory:
//! - **Context**: snapshot of the current environment collected by the runtime.
//! - **Memory**: accumulated by the agent over time, at its own initiative.

use std::path::Path;
use std::time::Instant;

/// Provides a situational context snapshot before an agent starts.
///
/// Memory is supplied at the agent's initiative; context is collected by the
/// runtime before the first step, independently of memory.
#[async_trait::async_trait]
pub trait ContextProvider: Send + Sync {
    /// Unique provider identifier, used as the cache key and section source.
    fn name(&self) -> &str;

    /// Collects a context snapshot for the `cwd` directory.
    ///
    /// Always fail-silent: return [`ContextSnapshot::with_error`] rather than
    /// propagating an error. The agent continues even if this provider fails.
    async fn collect(&self, cwd: &Path) -> ContextSnapshot;

    /// Short description for logs and the CLI.
    fn description(&self) -> &str {
        ""
    }

    /// Display priority in the system prompt (lower value shown first).
    fn priority(&self) -> u8 {
        50
    }

    /// Refresh interval in seconds.
    ///
    /// `None` means the assembler's global TTL applies.
    fn refresh_secs(&self) -> Option<u64> {
        None
    }

    /// Returns `true` if this provider is applicable in `cwd`.
    ///
    /// A provider returning `false` is never called: no overhead, no error.
    fn is_applicable(&self, cwd: &Path) -> bool {
        let _ = cwd;
        true
    }
}

/// Ephemeral snapshot produced by a [`ContextProvider`].
///
/// Injected into the agent's system prompt at session start. Not persisted:
/// recollected on every task start (subject to the cache TTL).
#[derive(Clone)]
pub struct ContextSnapshot {
    /// Identifier of the source provider.
    pub source: String,
    /// Text sections to inject into the prompt.
    pub sections: Vec<ContextSection>,
    /// Non-fatal errors: timeout, unreachable source, partial parse error.
    pub errors: Vec<String>,
    /// Collection timestamp for the cache TTL.
    pub collected_at: Instant,
}

/// Context section injected under a `<context name="title">` tag.
#[derive(Clone)]
pub struct ContextSection {
    /// Title shown in the `<context name="...">` tag.
    pub title: String,
    /// Text content of the section.
    pub content: String,
    /// Name of the provider that produced this section.
    pub source: String,
}

impl ContextSnapshot {
    /// Builds a snapshot with a single content section.
    pub fn single(source: &str, title: &str, content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            source: source.to_owned(),
            sections: vec![ContextSection {
                title: title.to_owned(),
                content,
                source: source.to_owned(),
            }],
            errors: vec![],
            collected_at: Instant::now(),
        }
    }

    /// Builds an empty snapshot: nothing to inject.
    ///
    /// Returned when the provider is not applicable or the source is empty.
    pub fn empty(source: &str) -> Self {
        Self {
            source: source.to_owned(),
            sections: vec![],
            errors: vec![],
            collected_at: Instant::now(),
        }
    }

    /// Builds an empty snapshot carrying a non-fatal error.
    ///
    /// The error is traced but does not prevent the agent from running.
    pub fn with_error(source: &str, error: String) -> Self {
        Self {
            source: source.to_owned(),
            sections: vec![],
            errors: vec![error],
            collected_at: Instant::now(),
        }
    }

    /// Returns `true` if the snapshot contains no sections.
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }
}
