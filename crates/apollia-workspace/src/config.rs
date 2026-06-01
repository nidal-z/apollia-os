//! Configuration for the `ProjectRuntime` (provider orchestrator).
//!
//! Only orchestration parameters live here: global timeout and cache TTL.
//! Each provider carries its own configuration in its own module.

use serde::Deserialize;

/// Configuration for the `ProjectRuntime`.
///
/// Controls orchestration behavior only: the global collection timeout
/// and the per-directory TTL cache lifetime.
#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    /// Global timeout applied to the whole collection, in seconds.
    ///
    /// If the timeout is exceeded, collection returns an empty [`WorkspaceSnapshot`].
    /// Default: 2.
    #[serde(default = "default_collect_timeout")]
    pub collect_timeout_secs: u64,

    /// Cache lifetime, in seconds.
    ///
    /// A second call to `collect()` within this window returns the cached result
    /// without re-running the providers.
    /// Default: 30.
    #[serde(default = "default_ttl")]
    pub context_ttl_secs: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            collect_timeout_secs: default_collect_timeout(),
            context_ttl_secs: default_ttl(),
        }
    }
}

fn default_collect_timeout() -> u64 {
    2
}

fn default_ttl() -> u64 {
    30
}

// ─────────────────────────────────────────────────────────────────────────────
// Native provider config
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for the [`GitProvider`](crate::providers::GitProvider).
#[derive(Debug, Clone, Deserialize)]
pub struct GitProviderConfig {
    /// Maximum number of lines kept from the `git status --short` output.
    /// Default: 200.
    #[serde(default = "default_git_status_lines")]
    pub git_status_max_lines: usize,
}

impl Default for GitProviderConfig {
    fn default() -> Self {
        Self {
            git_status_max_lines: default_git_status_lines(),
        }
    }
}

fn default_git_status_lines() -> usize {
    200
}

/// Configuration for the [`RulesProvider`](crate::providers::RulesProvider).
#[derive(Debug, Clone, Deserialize)]
pub struct RulesProviderConfig {
    /// Name of the rules file to look for (walks up the directory hierarchy).
    /// Default: "APOLLIA.md".
    #[serde(default = "default_rules_file")]
    pub file_name: String,

    /// Maximum size of the content injected into the prompt, in bytes.
    /// Default: 8192.
    #[serde(default = "default_rules_max_bytes")]
    pub max_bytes: usize,

    /// Maximum number of parent directories to walk up.
    /// Default: 5.
    #[serde(default = "default_search_depth")]
    pub search_depth: usize,
}

impl Default for RulesProviderConfig {
    fn default() -> Self {
        Self {
            file_name: default_rules_file(),
            max_bytes: default_rules_max_bytes(),
            search_depth: default_search_depth(),
        }
    }
}

fn default_rules_file() -> String {
    "APOLLIA.md".to_owned()
}

fn default_rules_max_bytes() -> usize {
    8192
}

fn default_search_depth() -> usize {
    5
}

/// Configuration for the [`StyleProvider`](crate::providers::StyleProvider).
#[derive(Debug, Clone, Deserialize)]
pub struct StyleProviderConfig {
    /// Number of source files to sample for style detection.
    /// Default: 7.
    #[serde(default = "default_style_sample_count")]
    pub sample_count: usize,

    /// Timeout in milliseconds for the style-detection LLM call.
    /// Default: 1000.
    #[serde(default = "default_style_timeout_ms")]
    pub timeout_ms: u64,

    /// Number of lines read per file when sampling (head + tail).
    /// Default: 200.
    #[serde(default = "default_style_sample_lines")]
    pub sample_lines_per_file: usize,
}

impl Default for StyleProviderConfig {
    fn default() -> Self {
        Self {
            sample_count: default_style_sample_count(),
            timeout_ms: default_style_timeout_ms(),
            sample_lines_per_file: default_style_sample_lines(),
        }
    }
}

fn default_style_sample_count() -> usize {
    7
}

fn default_style_timeout_ms() -> u64 {
    1000
}

fn default_style_sample_lines() -> usize {
    200
}
