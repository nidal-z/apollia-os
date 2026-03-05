//! Apollia OS — ORIA Engine (Observer-Reasoner-Actor).
//!
//! The execution engine that drives agent reasoning:
//! - `Observer` — enriches incoming `AIPTask` into a `ContextBundle` (STORY-029)
//! - `Reasoner` — calls the LLM and produces an `ExecutionPlan` (STORY-043)
//! - `ActorLoop` — executes plan steps, calls tools via `ToolProxy` (STORY-030)
//! - `StepBudget` — runtime-enforced budget, non-bypassable by agents (STORY-030)
//! - `ResilienceLayer` — circuit breaker per tool (STORY-041)
//!
//! ORIA operates in two modes classified automatically per task:
//! - Direct mode: for simple tasks (≤ 4 tools, ≤ 15 steps)
//! - Orchestrated mode: full Reasoner + Actor loop for complex tasks
