//! `web_search` tool — privacy-preserving web search with pluggable backends.
//!
//! # Architecture
//!
//! This module exposes a single [`WebSearch`] tool whose work is delegated to a
//! list of [`backend::SearchBackend`] implementations. At tool-invocation time
//! the user may pin a specific backend via the `backend` input field;
//! otherwise the first available backend in the list (Brave > DuckDuckGo) is
//! used.
//!
//! # Security posture
//!
//! `web_search` does **not** honour the agent-wide `http_allowlist`: its
//! network target is the *backend host*, which is an implementation detail
//! rather than an agent-supplied URL. Enabling the tool at all (via
//! `apollia.toml -> [tools].web_search = true`) is the user's opt-in to
//! network egress for search queries.

pub mod backend;

pub use backend::{SafeSearch, SearchBackendError, SearchQuery, SearchResult, TimeRange};
