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
//! [`org_policy`] reads and verifies the signed organisation policy that IT
//! drops in a system folder; [`load_policy`] answers whether it is absent,
//! valid or invalid, and nothing in this crate applies it yet.
//!
//! [`FamilyConsentStore`] holds the standing consents ("always" for one agent
//! and one action family, permanent or for one batch run), and
//! [`run_access_upgrade`] converts, once, the rules persisted before them.
//! [`read_legacy_chat_config`] reads, once, the settings of the built-in chat
//! that the runtime carries over to the Apollia agent.
//!
//! [`PermissionAuditLog`] reads the `permission_audit` table of `governance.db`
//! for the `apollia permissions audit` command and for the desktop audit view.
//! Nothing in this crate writes to that table.

pub mod access_upgrade;
pub mod audit_log;
pub mod chat_config_upgrade;
pub mod error;
pub mod executor_guard;
pub mod family_consent;
pub mod governance_schema;
pub mod org_policy;
pub mod prefix_rule_engine;

pub use access_upgrade::{
    run_access_upgrade, AccessUpgradeReport, UpgradeChange, UpgradeEntry, ALL_AGENTS,
};
pub use audit_log::{PermissionAuditEntry, PermissionAuditLog};
pub use chat_config_upgrade::{
    drop_legacy_chat_config, read_legacy_chat_config, LegacyChatConfig, LegacyChatConfigState,
};
pub use error::PermissionError;
pub use executor_guard::{is_code_executor, is_single_simple_command, CODE_EXECUTOR_TOOLS};
pub use family_consent::{
    ConsentFamily, ConsentScope, FamilyConsent, FamilyConsentStore, APOLLIA_AGENT_ID,
};
pub use governance_schema::{
    open_governance_schema, GOVERNANCE_MIGRATIONS, GOVERNANCE_SCHEMA_VERSION,
};
pub use org_policy::{
    load_policy, parse_policy, policy_location, OrgPolicy, OwnerRequirement, PolicyInvalid,
    PolicyInvalidReason, PolicyLocation, PolicySchemaError, PolicyVerdict, ValidPolicy,
};
pub use prefix_rule_engine::{
    extract_first_arg, PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction, ScopeContext,
};
