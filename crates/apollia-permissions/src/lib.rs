#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! Apollia OS tool permission rules.
//!
//! ## What actually gates a tool call
//!
//! The governance decision of `apollia-runtime` decides every tool call of
//! every agent, before it runs. This crate holds what it reads:
//!
//! - [`PrefixRuleEngine`] as the **rule store**: the fine rules, by argument
//!   prefix, listed by the governance actor; [`rule_matches`] matches one
//!   against a call, with the call's first argument extracted by
//!   [`prefix_rule_engine::extract_first_arg`]. For a code executor the match
//!   goes through [`executor_guard::is_single_simple_command`], so an allowed
//!   prefix only ever covers a single simple command, and a rule without a
//!   prefix never matches an executor.
//! - [`executor_guard::is_code_executor`]: `bash_executor` and
//!   `python_executor` execute code whatever their descriptor says, so no
//!   standing consent and no grant ever covers them.
//!
//! Anything the decision does not allow or refuse asks a human, through the
//! HITL flow of `apollia-runtime`.
//!
//! [`org_policy`] reads and verifies the signed organisation policy that IT
//! drops in a system folder; [`load_policy`] answers whether it is absent,
//! valid or invalid, and nothing in this crate applies it yet.
//!
//! [`FamilyConsentStore`] holds the standing consents ("always" for one agent
//! and one action family, permanent or for one batch run), and
//! [`run_access_upgrade`] converts, once, the rules persisted before them.
//! Every change these two stores make to a rule or a consent leaves its event
//! in a [`JournalOutbox`], which the runtime empties into the signed journal.
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
pub mod journal_outbox;
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
    TOOL_CREDENTIALS_UPGRADE,
};
pub use journal_outbox::{
    JournalOutbox, OutboxEntry, RuleRecord, StoreEvent, OUTBOX_EVENT_VERSION,
};
pub use org_policy::{
    load_policy, parse_policy, policy_location, OrgPolicy, OwnerRequirement, PackagesPolicy,
    PolicyInvalid, PolicyInvalidReason, PolicyLocation, PolicySchemaError, PolicyVerdict,
    PolicyView, ValidPolicy, REQUIRE_VERIFIED_SETTING,
};
pub use prefix_rule_engine::{
    extract_first_arg, rule_matches, PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction,
    ScopeContext,
};
