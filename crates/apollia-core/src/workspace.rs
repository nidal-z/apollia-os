//! [`WorkspaceProvider`] trait and types for collecting situational context.
//!
//! A **workspace** is the set of information about the current environment that
//! an agent receives before it starts working. This information is collected by
//! independent [`WorkspaceProvider`]s, aggregated into a [`WorkspaceSnapshot`],
//! then injected into the LLM system prompt.
//!
//! ## Key distinction from agent memory
//! - **Workspace context**: a snapshot of the environment collected by the runtime.
//! - **Memory**: accumulated by the agent over time, at its own initiative.

use std::path::Path;
use std::time::Instant;

// ─────────────────────────────────────────────────────────────────────────────
// Main trait
// ─────────────────────────────────────────────────────────────────────────────

/// Provides a slice of situational context before an agent starts.
///
/// Each implementation represents an independent information source: git state,
/// a rules file, the directory tree, a custom script, a Python module, etc.
/// Providers are orchestrated in parallel by [`WorkspaceSnapshot`].
///
/// ## Fail-silent
/// The [`collect`](WorkspaceProvider::collect) method must never propagate an
/// error: return [`WorkspaceSlice::with_error`] instead. The agent keeps going
/// even if a provider fails or exceeds its timeout.
#[async_trait::async_trait]
pub trait WorkspaceProvider: Send + Sync {
    /// Unique provider identifier, used as a cache key and as the source of sections.
    fn name(&self) -> &str;

    /// Collects a context slice for the `cwd` directory.
    ///
    /// Always fail-silent: return [`WorkspaceSlice::with_error`] rather than
    /// propagating an error.
    async fn collect(&self, cwd: &Path) -> WorkspaceSlice;

    /// Short description for logs and the CLI.
    fn description(&self) -> &str {
        ""
    }

    /// Display priority in the system prompt (lower value is shown first).
    fn priority(&self) -> u8 {
        50
    }

    /// Refresh interval in seconds.
    ///
    /// `None` means the runtime global TTL applies.
    fn refresh_secs(&self) -> Option<u64> {
        None
    }

    /// Returns `true` if this provider is applicable in `cwd`.
    ///
    /// A provider returning `false` is not called: no overhead, no error.
    fn is_applicable(&self, cwd: &Path) -> bool {
        let _ = cwd;
        true
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WorkspaceSlice - what a provider produces
// ─────────────────────────────────────────────────────────────────────────────

/// Ephemeral context slice produced by a [`WorkspaceProvider`].
///
/// Injected into the agent system prompt at session start. Not persisted:
/// recollected on every task start (modulo the TTL cache).
#[derive(Clone)]
pub struct WorkspaceSlice {
    /// Identifier of the source provider.
    pub source: String,
    /// Text sections to inject into the prompt.
    pub sections: Vec<WorkspaceSection>,
    /// Non-fatal errors: timeout, unreachable source, partial parse error.
    pub errors: Vec<String>,
    /// Collection timestamp for the TTL cache.
    pub collected_at: Instant,
}

/// Context section injected under a `<context name="title">` tag.
#[derive(Clone)]
pub struct WorkspaceSection {
    /// Title shown in the `<context name="...">` tag.
    pub title: String,
    /// Text content of the section.
    pub content: String,
    /// Name of the provider that produced this section.
    pub source: String,
}

impl WorkspaceSlice {
    /// Builds a slice with a single content section.
    pub fn single(source: &str, title: &str, content: impl Into<String>) -> Self {
        let content = content.into();
        Self {
            source: source.to_owned(),
            sections: vec![WorkspaceSection {
                title: title.to_owned(),
                content,
                source: source.to_owned(),
            }],
            errors: vec![],
            collected_at: Instant::now(),
        }
    }

    /// Builds an empty slice with no content to inject.
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

    /// Builds an empty slice carrying a non-fatal error.
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

    /// Returns `true` if the slice contains no section.
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// WorkspaceSnapshot - aggregation of all slices
// ─────────────────────────────────────────────────────────────────────────────

/// Complete workspace snapshot, aggregating every slice produced by the active
/// [`WorkspaceProvider`]s.
///
/// Created by the `ProjectRuntime` after parallel collection. Injected into the
/// LLM system prompt and exposed to the Python agent via `ctx.workspace`.
#[derive(Clone, Default)]
pub struct WorkspaceSnapshot {
    /// Slices produced by each provider, in priority order.
    pub slices: Vec<WorkspaceSlice>,
}

impl WorkspaceSnapshot {
    /// Builds a snapshot from a list of slices.
    pub fn new(slices: Vec<WorkspaceSlice>) -> Self {
        Self { slices }
    }

    /// Formats the snapshot for injection into an LLM system prompt.
    ///
    /// Each section is wrapped in a `<context name="title">` tag. Empty slices
    /// (no section) are omitted. Order follows provider priority (already sorted
    /// at collection time).
    pub fn format_for_prompt(&self) -> String {
        let blocks: Vec<String> = self
            .slices
            .iter()
            .flat_map(|s| &s.sections)
            .map(|s| format!("<context name=\"{}\">\n{}\n</context>", s.title, s.content))
            .collect();
        blocks.join("\n\n")
    }

    /// Returns `true` if the snapshot contains no section.
    pub fn is_empty(&self) -> bool {
        self.slices.iter().all(|s| s.sections.is_empty())
    }
}
