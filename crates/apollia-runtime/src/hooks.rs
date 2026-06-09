//! Lifecycle hooks: declarative interception points in the agent execution loop.
//!
//! A hook handler is an external `command` or `http` endpoint, declared in the
//! `[hooks]` section of `apollia.toml`, that the runtime invokes at defined
//! lifecycle events (see [`apollia_core::HookEventKind`]).
//!
//! - [`registry`]: the immutable [`HookRegistry`] built once from
//!   [`apollia_core::HooksConfig`] at startup and shared read-only with the
//!   execution loop.

pub mod registry;

pub use registry::{HookHandlerSummary, HookRegistry, ResolvedHandler};
