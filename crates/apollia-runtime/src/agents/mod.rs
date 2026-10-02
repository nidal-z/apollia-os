//! Community agent management, remote and local installation infrastructure.

use apollia_core::{AutonomyConfig, AutonomyLevel};

pub mod overrides;
pub mod registry_remote;
pub mod task;

/// The autonomy tier an agent runs at when neither its definition nor the
/// user names one: the `default_level` of the `[autonomy]` settings.
///
/// The runtime reads no `[autonomy]` section from `apollia.toml`, so this is
/// the built-in default of [`AutonomyConfig`].
pub fn global_default_autonomy() -> AutonomyLevel {
    AutonomyConfig::default().default_level
}
