//! Apollia OS — Memory Engine.
//!
//! Sovereign, local-first memory persistence via SQLite + FTS5.
//! One `.db` file per agent namespace: `~/.apollia/memory/<namespace>.db`
//!
//! Components (Sprint 3):
//! - `MemoryStore` — SQLite schema + versioned migrations (STORY-017)
//! - `EpisodicMemory` — event record with TTL and importance scoring (STORY-018)
//! - `SemanticMemory` — key/value with confidence and TTL (STORY-019)
//! - `ProceduralMemory` — trigger→steps patterns with success tracking (STORY-022)
//! - FTS5 full-text search with `unicode61` tokenizer for French (STORY-020)
//! - `MemoryManager` — namespace isolation and cross-namespace access control (STORY-021)
//!
//! The `unicode61` tokenizer is mandatory (ADR-009): "réunion" must match "reunion".

pub mod episodic;
pub mod semantic;
pub mod store;
