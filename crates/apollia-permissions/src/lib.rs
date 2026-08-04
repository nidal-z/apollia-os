#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Apollia OS tool permission rules.
//!
//! ## What actually gates a tool call
//!
//! Read this before assuming a layer protects you. Two of the pieces in this
//! crate are **not** in the execution path of the shipped runtime.
//!
//! Live on the chat path:
//!
//! - [`PrefixRuleEngine`] as a **rule store**: the chat manager lists its
//!   persisted rules once per message and seeds a name-only authorization set
//!   from the allow rules that carry no `arg_prefix`
//!   (`apply_chat_prefix_allow_rules` in `apollia-runtime`). That set is the
//!   mechanism behind "always allow" for ordinary tools.
//! - The **per-invocation prefix matching**: on a miss of that set, the chat
//!   ReAct loop consults [`PrefixRuleEngine::check_with_scope`] with the
//!   call's first argument (`build_prefix_checker` in `apollia-runtime`).
//!   For a code executor the match goes through
//!   [`executor_guard::is_single_simple_command`], so an allowed prefix only
//!   ever covers a single simple command, and a rule without a prefix never
//!   matches an executor.
//! - [`executor_guard::is_code_executor`]: the invariant that `bash_executor`
//!   and `python_executor` are never blanket-authorised; the runtime filters
//!   them out of the authorization set on every seeding route.
//! - [`PermissionAuditLog`]: every decision recorded in SQLite.
//!
//! Present but **not wired** at all:
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
//! Practical consequence: the anti-chaining protection people usually
//! attribute to [`InjectionDetector`] is in fact delivered by
//! [`executor_guard::is_single_simple_command`], applied by the chat
//! checker's per-invocation match; outside a matching prefix rule, every code
//! executor call still asks a human. And note that [`InjectionDetector`]
//! detects **shell** injection, not prompt injection; there is no
//! prompt-injection defence in this crate.
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
pub use engine::{extract_first_arg, PermissionDecision, PermissionEngine, CONFIG_IMPORT_CREATOR};
pub use error::PermissionError;
pub use executor_guard::{is_code_executor, is_single_simple_command, CODE_EXECUTOR_TOOLS};
pub use injection_detector::{InjectionDetector, StructuralInjectionDetector};
pub use prefix_rule_engine::{
    PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction, ScopeContext,
};
pub use safe_list::SafeList;
