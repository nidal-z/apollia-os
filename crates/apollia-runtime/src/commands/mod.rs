//! Custom slash command registry for the Apollia REPL.
//!
//! Exposes [`CommandRegistry`] and [`CustomCommand`] — the primary types for
//! loading and dispatching operator-defined prompts from `.apollia/commands/`.

pub mod registry;

pub use registry::{CommandRegistry, CustomCommand};
