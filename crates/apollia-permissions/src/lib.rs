#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Apollia OS tool permission rules.
//!
//! ## What actually gates a tool call
//!
//! Read this before assuming a layer protects you. Two of the pieces in this
//! crate are **not** in the execution path of the shipped runtime.
//!
//! Live, consulted on every chat tool invocation:
//!
//! - [`PrefixRuleEngine`]: persisted per-scope rules in SQLite (session,
//!   project, global), the mechanism behind "always allow `git status`".
//! - [`executor_guard`]: the invariant that `bash_executor` and
//!   `python_executor` are never blanket-authorised by tool name, and that a
//!   prefix rule only ever matches a single simple command, so an authorised
//!   `git status` cannot carry `; rm -rf /`.
//! - [`PermissionAuditLog`]: every decision recorded in SQLite.
//!
//! Present but **not wired**:
//!
//! - [`PermissionEngine`], the aggregate below, together with [`SafeList`] and
//!   [`InjectionDetector`]. `ToolDispatcher` holds an `Option<PermissionEngine>`
//!   that no production caller ever populates (see
//!   `apollia_tools::executor::ToolDispatcher::with_permission_engine`, which
//!   has no callers), so `SafeList` and `InjectionDetector` never run and
//!   `PermissionDecision::AutoDeniedInjection` is unreachable in the shipped
//!   binary. They are kept because they are tested and useful to an embedder
//!   that opts in, not because they are protecting the desktop app today.
//!
//! Practical consequence: the anti-chaining protection people usually attribute
//! to [`InjectionDetector`] is in fact delivered by
//! [`executor_guard::is_single_simple_command`], which *is* live. And note that
//! [`InjectionDetector`] detects **shell** injection, not prompt injection;
//! there is no prompt-injection defence in this crate.
//!
//! ## Usage of the opt-in aggregate
//!
//! ```rust,ignore
//! use apollia_permissions::{PermissionEngine, PermissionDecision};
//! use apollia_core::config::PermissionsConfig;
//!
//! // Only reached if the host explicitly calls
//! // `ToolDispatcher::with_permission_engine(engine)`.
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
pub mod executor_guard;
pub mod injection_detector;
pub(crate) mod migrations;
pub mod prefix_rule_engine;
pub mod safe_list;

pub use audit_log::{PermissionAuditEntry, PermissionAuditLog};
pub use engine::{PermissionDecision, PermissionEngine, CONFIG_IMPORT_CREATOR};
pub use error::PermissionError;
pub use executor_guard::{is_code_executor, is_single_simple_command, CODE_EXECUTOR_TOOLS};
pub use injection_detector::{InjectionDetector, StructuralInjectionDetector};
pub use prefix_rule_engine::{
    PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction, ScopeContext,
};
pub use safe_list::SafeList;
