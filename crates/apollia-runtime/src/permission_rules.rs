//! The permission rules a user writes by hand or accepts from a proposal.
//!
//! The onboarding agent derives permission rules from the user's answers and
//! serialises them to the memory key `onboarding.proposed_rules`. The user
//! reads them, applies or dismisses each; an applied one becomes a rule in
//! `governance.db`, stamped as created by the onboarding agent. No native tool
//! writes a permission rule: an agent proposes, the user decides.
//!
//! The desktop running the runtime in its own process calls these functions
//! directly; a client of the daemon reaches the same functions through
//! `/api/v1/permissions/*`. Both read the same data directory and refuse with
//! the same words.

use std::path::{Path, PathBuf};

use apollia_memory::semantic::{RememberInput, SemanticMemory};
use apollia_memory::store::MemoryStore;
use apollia_permissions::prefix_rule_engine::{
    PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction,
};
use apollia_tools::governance_db::{GovernanceDb, GOVERNANCE_DB_FILENAME};
use serde::{Deserialize, Serialize};

/// Author tag for rules created via the onboarding approval flow.
const CREATOR: &str = "onboarding-agent";

/// Memory key under which the agent serialises the JSON proposals list.
const PROPOSED_RULES_KEY: &str = "onboarding.proposed_rules";

/// Candidate (db_filename, namespace) pairs to look in, the modern and the
/// legacy onboarding namespaces.
const NAMESPACE_CANDIDATES: [(&str, &str); 2] = [
    ("onboarding.db", "onboarding"),
    ("onboarding-agent.db", "onboarding-agent"),
];

/// One proposed permission rule, as the approval card draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProposedRuleView {
    /// Position in the persisted list, the cursor for apply and dismiss.
    pub index: usize,
    /// Tool the rule targets.
    pub tool_name: String,
    /// `allow` or `deny`.
    pub action: String,
    /// Argument prefix the rule matches, when it has one.
    pub arg_prefix: Option<String>,
    /// `global` or `agent`.
    pub scope: String,
}

/// Why a permission rule operation did not complete. The display text is
/// what the interface shows, the same whichever process ran the operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PermissionRuleError {
    /// No proposal list is stored.
    #[error("no proposed rules in memory")]
    NoProposals,
    /// The cursor points past the end of the list.
    #[error("proposal index {index} out of range (len={len})")]
    OutOfRange {
        /// The cursor received.
        index: usize,
        /// The length of the list.
        len: usize,
    },
    /// The stored proposal cannot become a rule.
    #[error("{0}")]
    InvalidProposal(String),
    /// The rule written by hand is refused before it is stored.
    #[error("{0}")]
    InvalidRule(String),
    /// A database did not open, read or write.
    #[error("{0}")]
    Storage(String),
    /// The blocking task did not complete.
    #[error("spawn_blocking failed: {0}")]
    Join(#[source] tokio::task::JoinError),
}

/// The memory directory under the data directory, where the onboarding
/// agent's namespace databases live.
fn memory_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("memory")
}

/// Every proposal stored by the onboarding agent, empty when none is.
///
/// # Errors
///
/// [`PermissionRuleError::Storage`] when a namespace database exists and does
/// not read.
pub async fn list_proposals(
    data_dir: PathBuf,
) -> Result<Vec<ProposedRuleView>, PermissionRuleError> {
    let memory_dir = memory_dir(&data_dir);
    if !memory_dir.exists() {
        return Ok(Vec::new());
    }
    tokio::task::spawn_blocking(move || {
        let raw = read_proposed_rules_raw(&memory_dir)?.unwrap_or_default();
        Ok(parse_proposals(&raw))
    })
    .await
    .map_err(PermissionRuleError::Join)?
}

/// Applies the proposal at `index`: stores it in `governance.db` as created
/// by the onboarding agent, then removes it from the list. Returns the new
/// rule's id.
///
/// # Errors
///
/// [`PermissionRuleError::NoProposals`], [`PermissionRuleError::OutOfRange`],
/// [`PermissionRuleError::InvalidProposal`] for a proposal that cannot be a
/// rule, [`PermissionRuleError::Storage`] when a database refuses.
pub async fn apply_proposal(data_dir: PathBuf, index: usize) -> Result<i64, PermissionRuleError> {
    tokio::task::spawn_blocking(move || {
        let memory_dir = memory_dir(&data_dir);
        let mut proposals = load_proposals(&memory_dir)?;
        check_index(index, proposals.len())?;
        let chosen = proposals.remove(index);
        let rule_id = persist_chosen_rule(&data_dir, &chosen)?;
        write_proposed_rules(&memory_dir, &proposals)?;
        Ok(rule_id)
    })
    .await
    .map_err(PermissionRuleError::Join)?
}

/// Removes the proposal at `index` from the list without storing any rule.
///
/// # Errors
///
/// [`PermissionRuleError::NoProposals`], [`PermissionRuleError::OutOfRange`],
/// [`PermissionRuleError::Storage`] when the namespace database refuses.
pub async fn dismiss_proposal(data_dir: PathBuf, index: usize) -> Result<(), PermissionRuleError> {
    tokio::task::spawn_blocking(move || {
        let memory_dir = memory_dir(&data_dir);
        let mut proposals = load_proposals(&memory_dir)?;
        check_index(index, proposals.len())?;
        proposals.remove(index);
        write_proposed_rules(&memory_dir, &proposals)
    })
    .await
    .map_err(PermissionRuleError::Join)?
}

/// Adds a prefix rule for every agent, as the "Always allow" buttons of the
/// approval cards write it. A rule targets one agent or every agent; this
/// writes the latter, and `scope` must say `global`. Returns the rule's id.
///
/// # Errors
///
/// [`PermissionRuleError::InvalidRule`] for an empty tool name, an action
/// other than `allow` or `deny`, a scope other than `global`;
/// [`PermissionRuleError::Storage`] when `governance.db` refuses.
pub async fn add_global_rule(
    data_dir: PathBuf,
    tool_name: String,
    arg_prefix: Option<String>,
    action: String,
    scope: String,
) -> Result<i64, PermissionRuleError> {
    if tool_name.trim().is_empty() {
        return Err(PermissionRuleError::InvalidRule(
            "tool_name must not be empty".to_owned(),
        ));
    }
    let action = match action.as_str() {
        "allow" => RuleAction::Allow,
        "deny" => RuleAction::Deny,
        other => {
            return Err(PermissionRuleError::InvalidRule(format!(
                "unknown action '{other}', expected 'allow' or 'deny'"
            )))
        }
    };
    if scope != "global" {
        return Err(PermissionRuleError::InvalidRule(format!(
            "unknown scope '{scope}', expected 'global' (a rule targets one agent or every agent)"
        )));
    }
    let rule = PrefixRule {
        tool_name: tool_name.clone(),
        arg_prefix: arg_prefix.clone(),
        action,
        created_at: chrono::Utc::now().timestamp(),
        scope: PermissionScope::Global,
        agent_id: None,
        ..PrefixRule::default()
    };
    let id = tokio::task::spawn_blocking(move || persist_rule(&data_dir, &rule))
        .await
        .map_err(PermissionRuleError::Join)??;
    tracing::event!(
        tracing::Level::INFO,
        tool = %tool_name,
        arg_prefix = ?arg_prefix,
        scope = "global",
        "permission.rule.added"
    );
    Ok(id)
}

fn check_index(index: usize, len: usize) -> Result<(), PermissionRuleError> {
    if index >= len {
        return Err(PermissionRuleError::OutOfRange { index, len });
    }
    Ok(())
}

fn load_proposals(memory_dir: &Path) -> Result<Vec<serde_json::Value>, PermissionRuleError> {
    if !memory_dir.exists() {
        return Err(PermissionRuleError::NoProposals);
    }
    let raw = read_proposed_rules_raw(memory_dir)?.ok_or(PermissionRuleError::NoProposals)?;
    parse_proposals_raw(&raw)
}

/// Returns the JSON string stored at `onboarding.proposed_rules`, scanning
/// both candidate namespaces. `None` when no entry was found.
fn read_proposed_rules_raw(memory_dir: &Path) -> Result<Option<String>, PermissionRuleError> {
    for (filename, namespace) in NAMESPACE_CANDIDATES {
        let db_path = memory_dir.join(filename);
        if !db_path.exists() {
            continue;
        }
        let store = MemoryStore::open(&db_path).map_err(|e| {
            PermissionRuleError::Storage(format!("failed to open {}: {e}", db_path.display()))
        })?;
        let sem = SemanticMemory::new(&store);
        let entries = sem
            .recall_all(namespace, None)
            .map_err(|e| PermissionRuleError::Storage(format!("recall_all failed: {e}")))?;
        if let Some(entry) = entries.iter().find(|e| e.key == PROPOSED_RULES_KEY) {
            // Stored as a JSON string-of-list (the agent does json.dumps(proposals)).
            let raw = entry
                .value
                .as_str()
                .map(|s| s.to_owned())
                .unwrap_or_else(|| entry.value.to_string());
            return Ok(Some(raw));
        }
    }
    Ok(None)
}

/// Re-writes (or deletes) the proposals list in the first existing namespace.
fn write_proposed_rules(
    memory_dir: &Path,
    proposals: &[serde_json::Value],
) -> Result<(), PermissionRuleError> {
    for (filename, namespace) in NAMESPACE_CANDIDATES {
        let db_path = memory_dir.join(filename);
        if !db_path.exists() {
            continue;
        }
        let store = MemoryStore::open(&db_path).map_err(|e| {
            PermissionRuleError::Storage(format!("failed to open {}: {e}", db_path.display()))
        })?;
        let sem = SemanticMemory::new(&store);
        if proposals.is_empty() {
            // Drop the entry entirely when there is nothing left to approve.
            let _ = sem.forget(namespace, PROPOSED_RULES_KEY);
        } else {
            let serialised = serde_json::to_string(proposals).map_err(|e| {
                PermissionRuleError::Storage(format!("failed to re-serialise proposals: {e}"))
            })?;
            sem.remember(RememberInput {
                namespace,
                key: PROPOSED_RULES_KEY,
                value: &serde_json::Value::String(serialised),
                confidence: 0.9,
                source: Some(CREATOR),
                expires_at: None,
            })
            .map_err(|e| {
                PermissionRuleError::Storage(format!("failed to persist truncated list: {e}"))
            })?;
        }
        return Ok(());
    }
    Err(PermissionRuleError::Storage(
        "no onboarding namespace database found".to_owned(),
    ))
}

/// Parses and projects the raw JSON list into [`ProposedRuleView`] items
/// indexed by their position. Tolerant: invalid items are skipped.
fn parse_proposals(raw: &str) -> Vec<ProposedRuleView> {
    parse_proposals_raw(raw)
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let obj = p.as_object()?;
            Some(ProposedRuleView {
                index: i,
                tool_name: obj.get("tool_name")?.as_str()?.to_owned(),
                action: obj.get("action")?.as_str()?.to_owned(),
                arg_prefix: obj
                    .get("arg_prefix")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
                scope: obj
                    .get("scope")
                    .and_then(|v| v.as_str())
                    .unwrap_or("global")
                    .to_owned(),
            })
        })
        .collect()
}

/// Parses the raw JSON into a list of values, used both for projection and
/// for write-back (the list is re-serialised minus the chosen item).
fn parse_proposals_raw(raw: &str) -> Result<Vec<serde_json::Value>, PermissionRuleError> {
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
        PermissionRuleError::InvalidProposal(format!("invalid proposals JSON: {e}"))
    })?;
    match value {
        serde_json::Value::Array(items) => Ok(items),
        _ => Err(PermissionRuleError::InvalidProposal(
            "proposed_rules is not a JSON array".to_owned(),
        )),
    }
}

fn invalid(message: impl Into<String>) -> PermissionRuleError {
    PermissionRuleError::InvalidProposal(message.into())
}

/// Stores one proposal as a rule in `governance.db`, stamped as created by
/// the onboarding agent. Bypasses the tool dispatcher.
fn persist_chosen_rule(
    data_dir: &Path,
    proposal: &serde_json::Value,
) -> Result<i64, PermissionRuleError> {
    let obj = proposal
        .as_object()
        .ok_or_else(|| invalid("proposal is not a JSON object"))?;
    let tool_name = obj
        .get("tool_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| invalid("missing tool_name"))?
        .to_owned();
    // The rule store accepts any tool name, and a name absent from the
    // registry can never match an invocation: the rule would be displayed as
    // effective while being dead. Persist anyway (the registry is not the
    // only historical source of names) but leave a trace.
    if !apollia_tools::NATIVE_TOOL_NAMES.contains(&tool_name.as_str()) {
        tracing::event!(
            tracing::Level::WARN,
            tool = %tool_name,
            "permissions.proposal_unknown_tool"
        );
    }
    let action = match obj.get("action").and_then(|v| v.as_str()) {
        Some("allow") => RuleAction::Allow,
        Some("deny") => RuleAction::Deny,
        Some(other) => return Err(invalid(format!("unknown action: {other}"))),
        None => return Err(invalid("missing action")),
    };
    let arg_prefix = obj
        .get("arg_prefix")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let scope = match obj
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("global")
    {
        "global" => PermissionScope::Global,
        "agent" => PermissionScope::Agent,
        // A rule targets one agent or every agent, never a project or a session.
        other => return Err(invalid(format!("scope '{other}' not persistable"))),
    };
    persist_rule(
        data_dir,
        &PrefixRule {
            tool_name,
            arg_prefix,
            action,
            created_at: chrono::Utc::now().timestamp(),
            scope,
            agent_id: None,
            created_by_agent: Some(CREATOR.to_owned()),
            ..PrefixRule::default()
        },
    )
}

/// Migrates `governance.db` under `data_dir` if needed, then stores `rule`.
/// Returns its id.
///
/// # Errors
///
/// [`PermissionRuleError::Storage`] when the database does not open or write.
pub fn persist_rule(data_dir: &Path, rule: &PrefixRule) -> Result<i64, PermissionRuleError> {
    let _migrate = GovernanceDb::open(data_dir).map_err(|e| {
        PermissionRuleError::Storage(format!("failed to open governance database: {e}"))
    })?;
    let db_path = data_dir.join(GOVERNANCE_DB_FILENAME);
    let mut engine = PrefixRuleEngine::new(&db_path).map_err(|e| {
        PermissionRuleError::Storage(format!("failed to open prefix rule engine: {e}"))
    })?;
    engine
        .add_rule(rule)
        .map_err(|e| PermissionRuleError::Storage(format!("failed to persist prefix rule: {e}")))
}

/// Stores `proposals` as the onboarding agent does, in the modern namespace
/// under `data_dir`.
#[cfg(test)]
pub(crate) fn seed_proposals(data_dir: &Path, proposals: &serde_json::Value) {
    let memory_dir = memory_dir(data_dir);
    std::fs::create_dir_all(&memory_dir).unwrap();
    let store = MemoryStore::open(&memory_dir.join(NAMESPACE_CANDIDATES[0].0)).unwrap();
    SemanticMemory::new(&store)
        .remember(RememberInput {
            namespace: NAMESPACE_CANDIDATES[0].1,
            key: PROPOSED_RULES_KEY,
            value: &serde_json::Value::String(proposals.to_string()),
            confidence: 0.9,
            source: Some(CREATOR),
            expires_at: None,
        })
        .unwrap();
}

#[cfg(test)]
mod tests;
