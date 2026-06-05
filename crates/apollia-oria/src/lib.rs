#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Apollia OS ORIA Engine (Observer-Reasoner-Actor).
//!
//! The execution engine that drives agent reasoning:
//! - `Observer`: enriches incoming `AIPTask` into a `ContextBundle`
//! - `Reasoner`: calls the LLM and produces an `ExecutionPlan`
//! - `ActorLoop`: executes plan steps, calls tools via `ToolProxy`
//! - `StepBudget`: runtime-enforced budget, non-bypassable by agents
//! - `ResilienceLayer`: circuit breaker per tool
//!
//! ORIA operates in two modes classified automatically per task:
//! - Direct mode: for simple tasks (up to 4 tools, up to 15 steps)
//! - Orchestrated mode: full Reasoner + Actor loop for complex tasks

pub mod actor;
pub mod budget;
pub mod context_manager;
pub mod engine;
pub mod observer;
pub mod plan;
pub mod plan_cache;
pub mod plan_repository;
pub mod reasoner;
pub mod resilience;
pub mod tool_offload;
pub mod topo;
pub mod verification;

pub use resilience::{
    CircuitBreaker, CircuitBreakerSnapshot, CircuitState, ErrorClass, ResilienceError,
    ResilienceLayer,
};
