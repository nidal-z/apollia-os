//! Logging structuré + métriques optionnelles.

pub mod logs;

#[cfg(feature = "metrics")]
pub mod metrics;
