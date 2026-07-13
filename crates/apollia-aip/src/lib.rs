#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Apollia OS: Agent Interface Protocol (AIP) bridge.
//!
//! The PyO3-based bridge enabling Python agents to run inside the Rust runtime.
//! Implements duck-typing validation: any Python object with `manifest()` and
//! `async run()` is AIP-compatible.
//!
//! Components:
//! - `loader`: loads a Python module and validates AIP duck-typing.
//! - `bridge`: async Rust to Python calls via pyo3-async-runtimes.
//! - `context`: `RuntimeContext` injected into agent `run()` calls.
//!
//! Nested ctx surfaces (additive, non-breaking):
//! - `a2a`: nested `ctx.a2a` surface (consolidates flat A2A methods).
//! - `events`: `ctx.events` typed event emission.
//! - `datasources`: `ctx.datasources` YAML runtime access.
//! - `templates`: `ctx.templates` Jinja2 runtime rendering.
//! - `secrets`: `ctx.secrets` read-only credentials gating.
//! - `budget`: `ctx.budget` step budget view (successor of `step_budget`).

pub mod a2a;
pub mod bridge;
pub mod budget;
#[allow(clippy::useless_conversion)]
pub mod context;
pub mod datasources;
pub mod events;
#[allow(clippy::useless_conversion)]
pub mod llm;
pub mod loader;
pub mod mail;
#[allow(clippy::useless_conversion)]
pub mod memory;
pub mod notify;
pub mod package_loader;
#[allow(clippy::useless_conversion)]
pub mod profile;
pub mod python_provider;
pub mod runtime_pin;
pub mod secrets;
pub mod stt;
pub mod templates;
pub mod validator;

// Public re-exports, consumed by apollia-runtime when wiring
// RuntimeContext at task start. Keeping these flat at the crate root
// makes the bridge surface easy to discover.
pub use a2a::A2AInterface;
pub use budget::BudgetView;
pub use datasources::DatasourcesInterface;
pub use events::EventsInterface;
pub use mail::MailInterface;
pub use runtime_pin::pin_async_runtime;
pub use secrets::SecretsInterface;
pub use templates::TemplatesInterface;
