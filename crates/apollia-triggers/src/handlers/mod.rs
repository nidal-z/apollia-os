//! Handlers that consume trigger events and act on runtime components.
//!
//! Each handler watches an external resource (config file, directory, …)
//! and drives live updates without requiring a runtime restart.

pub mod config_watch;
