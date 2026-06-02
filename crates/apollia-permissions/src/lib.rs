#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Apollia OS three-layer permission engine.
//!
//! Evaluates every tool invocation before execution through three ordered layers:
//!
//! 1. `InjectionDetector` (layer 3, highest priority): blocks dangerous shell patterns.
//! 2. `SafeList` (layer 1): auto-approves explicitly configured commands.
//! 3. `PrefixRuleEngine` (layer 2): evaluates persisted prefix rules in SQLite.
//!
//! Every decision is recorded in `PermissionAuditLog` (SQLite, immutable).
//!
//! ## Usage
//!
//! ```rust,ignore
//! use apollia_permissions::{PermissionEngine, PermissionDecision};
//! use apollia_core::config::PermissionsConfig;
//!
//! let engine = PermissionEngine::new(&config, db_path)?;
//! let decision = engine.decide("bash_executor", &input, &manifest)?;
//! match decision {
//!     PermissionDecision::AutoAllowedSafeList => { /* execute */ }
//!     PermissionDecision::NeedsApproval => { /* emit PermissionRequired */ }
//!     PermissionDecision::AutoDeniedInjection { pattern } => { /* return PermissionDenied */ }
//!     _ => {}
//! }
//! ```

pub mod audit_log;
pub mod engine;
pub mod error;
pub mod injection_detector;
pub(crate) mod migrations;
pub mod prefix_rule_engine;
pub mod safe_list;

pub use audit_log::{PermissionAuditEntry, PermissionAuditLog};
pub use engine::{PermissionDecision, PermissionEngine, CONFIG_IMPORT_CREATOR};
pub use error::PermissionError;
pub use injection_detector::{InjectionDetector, StructuralInjectionDetector};
pub use prefix_rule_engine::{
    PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction, ScopeContext,
};
pub use safe_list::SafeList;
