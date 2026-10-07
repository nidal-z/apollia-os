#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Apollia OS: Agent Interface Protocol (AIP) bridge.
//!
//! The PyO3-based bridge between the Rust runtime and the Python a package
//! brings: its tools and its workflows. An agent is data, read from the
//! manifest of its package; no Python object stands for an agent.
//!
//! Components:
//! - `package_tools`: loads the modules of an installed package and runs its
//!   tools and workflows.
//! - `cancel`: stops a running Python call when its run is cancelled.
//! - `context`: `RuntimeContext`, the `ctx` a tool or a workflow receives.
//!
//! Nested ctx surfaces (additive, non-breaking):
//! - `a2a`: nested `ctx.a2a` surface (consolidates flat A2A methods).
//! - `events`: `ctx.events` typed event emission.
//! - `datasources`: `ctx.datasources` YAML runtime access.
//! - `templates`: `ctx.templates` Jinja2 runtime rendering.
//! - `secrets`: `ctx.secrets` read-only credentials gating.
//! - `budget`: `ctx.budget` step budget view (successor of `step_budget`).

pub mod a2a;
pub use apollia_tools::{apo_archive, apollia_manifest};
pub mod budget;
pub(crate) mod cancel;
#[allow(clippy::useless_conversion)]
pub mod context;
pub mod datasources;
pub mod events;
#[allow(clippy::useless_conversion)]
pub mod llm;
pub mod mail;
#[allow(clippy::useless_conversion)]
pub mod memory;
pub mod notify;
pub mod package_tools;
#[allow(clippy::useless_conversion)]
pub mod profile;
pub mod python_provider;
pub mod runtime_pin;
pub mod secrets;
pub mod stt;
pub mod templates;

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
