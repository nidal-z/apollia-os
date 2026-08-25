#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Apollia OS tool permission rules.
//!
//! ## What actually gates a tool call
//!
//! - [`PrefixRuleEngine`] as a **rule store**: the chat manager lists its
//!   persisted rules once per message and seeds a name-only authorization set
//!   from the allow rules that carry no `arg_prefix`
//!   (`apply_chat_prefix_allow_rules` in `apollia-runtime`). That set is the
//!   mechanism behind "always allow" for ordinary tools.
//! - The **per-invocation prefix matching**: on a miss of that set, the chat
//!   ReAct loop consults [`PrefixRuleEngine::check_with_scope`] with the
//!   call's first argument (`build_prefix_checker` in `apollia-runtime`), which
//!   it extracts with [`prefix_rule_engine::extract_first_arg`].
//!   For a code executor the match goes through
//!   [`executor_guard::is_single_simple_command`], so an allowed prefix only
//!   ever covers a single simple command, and a rule without a prefix never
//!   matches an executor.
//! - [`executor_guard::is_code_executor`]: the invariant that `bash_executor`
//!   and `python_executor` are never blanket-authorised; the runtime filters
//!   them out of the authorization set on every seeding route.
//!
//! Anything the chat path does not auto-approve asks a human, through the HITL
//! flow of `apollia-runtime`.
//!
//! [`PermissionAuditLog`] reads the `permission_audit` table of `governance.db`
//! for the `apollia permissions audit` command and for the desktop audit view.
//! Nothing in this crate writes to that table.

pub mod audit_log;
pub mod error;
pub mod executor_guard;
pub mod governance_schema;
pub mod prefix_rule_engine;

pub use audit_log::{PermissionAuditEntry, PermissionAuditLog};
pub use error::PermissionError;
pub use executor_guard::{is_code_executor, is_single_simple_command, CODE_EXECUTOR_TOOLS};
pub use governance_schema::{
    open_governance_schema, GOVERNANCE_MIGRATIONS, GOVERNANCE_SCHEMA_VERSION,
};
pub use prefix_rule_engine::{
    extract_first_arg, PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction, ScopeContext,
};
