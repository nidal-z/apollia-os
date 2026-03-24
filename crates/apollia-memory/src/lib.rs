//! Apollia OS — Memory Engine.
//!
//! Sovereign, local-first memory persistence via SQLite + FTS5.
//! One `.db` file per agent namespace: `~/.apollia/memory/<namespace>.db`
//!
//! Components (Sprint 3):
//! - `MemoryStore` — SQLite schema + versioned migrations.
//! - `EpisodicMemory` — event record with TTL and importance scoring.
//! - `SemanticMemory` — key/value with confidence and TTL.
//! - `ProceduralMemory` — trigger→steps patterns with success tracking.
//! - FTS5 full-text search with `unicode61` tokenizer for French.
//! - `MemoryManager` — namespace isolation and cross-namespace access control.
//!
//! The `unicode61` tokenizer is mandatory (ADR-009): "réunion" must match "reunion".

pub mod episodic;
pub mod manager;
pub mod procedural;
pub mod search;
pub mod semantic;
pub mod store;
