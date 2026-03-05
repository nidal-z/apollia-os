//! Apollia OS — shared types.
//!
//! `apollia-core` is the dependency foundation of the entire workspace.
//! All other crates depend on this one; this crate depends on nothing
//! else within the workspace.
//!
//! Types implemented in Sprint 0:
//! - `AgentManifest`, `AgentSkill` — STORY-002
//! - `AIPTask`, `AIPInput`, `AIPPart` — STORY-002
//! - `AIPResult`, `AIPError`, `AIPArtifact` — STORY-002
//! - `ProcessState`, `TaskStatus` — STORY-003
//! - `StepBudgetConfig`, `SandboxProfile` — STORY-004
