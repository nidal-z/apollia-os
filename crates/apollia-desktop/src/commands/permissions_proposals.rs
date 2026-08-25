//! Tauri IPC commands for the onboarding permission proposals flow.
//!
//! Background: when the onboarding-agent finalises the user profile, it
//! derives a list of permission rules from the answers and serialises them to
//! the memory key `onboarding.proposed_rules`.
//!
//! The desktop reads that key and renders one approval card per proposal in
//! the onboarding modal. On approval, this module persists the rule directly
//! through `PrefixRuleEngine::add_rule`, rather than by calling the
//! `permission_rule_add` native tool.
//!
//! Why: `permission_rule_add` is an ordinary native tool, so a chat invocation
//! of it goes through the approval flow like any other. Reaching it from a
//! Tauri command triggered by an explicit user click would re-ask a question
//! the user has just answered in the UI.

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

/// Candidate (db_filename, namespace) pairs to look in. Mirrors
/// `check_onboarding_finalized` to support both the modern and legacy agent
/// manifest namespaces.
const NAMESPACE_CANDIDATES: [(&str, &str); 2] = [
    ("onboarding.db", "onboarding"),
    ("onboarding-agent.db", "onboarding-agent"),
];

/// Minimal projection of one proposed permission rule for the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedRuleView {
    /// Position in the persisted JSON list, used as the cursor for apply/dismiss.
    pub index: usize,
    pub tool_name: String,
    pub action: String,
    pub arg_prefix: Option<String>,
    pub scope: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands
// ─────────────────────────────────────────────────────────────────────────────

/// Reads the persisted proposal list from the agent's semantic memory.
///
/// Returns an empty list when the key is absent or empty. Errors only on
/// catastrophic disk / parse failures; a missing namespace is normal pre-finalize.
#[tauri::command]
pub async fn list_proposed_permission_rules() -> Result<Vec<ProposedRuleView>, String> {
    let memory_dir = memory_dir();
    if !memory_dir.exists() {
        return Ok(Vec::new());
    }
    tokio::task::spawn_blocking(move || -> Result<Vec<ProposedRuleView>, String> {
        let raw = read_proposed_rules_raw(&memory_dir)?.unwrap_or_default();
        Ok(parse_proposals(&raw))
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// Applies the proposal at `index`: persists it in `governance.db` with
/// `created_by="onboarding-agent"`, then removes it from the proposal list.
///
/// Returns the new rule's `id`.
#[tauri::command]
pub async fn apply_proposed_permission_rule(index: usize) -> Result<i64, String> {
    let memory_dir = memory_dir();
    let governance_dir = governance_dir()?;

    tokio::task::spawn_blocking(move || -> Result<i64, String> {
        // 1. Load + remove the chosen proposal.
        let raw = read_proposed_rules_raw(&memory_dir)?
            .ok_or_else(|| "no proposed rules in memory".to_string())?;
        let mut proposals = parse_proposals_raw(&raw)?;
        if index >= proposals.len() {
            return Err(format!(
                "proposal index {} out of range (len={})",
                index,
                proposals.len()
            ));
        }
        let chosen = proposals.remove(index);

        // 2. Persist as a real rule in governance.db.
        let rule_id = persist_chosen_rule(&governance_dir, &chosen)?;

        // 3. Re-write the truncated list (or remove the key when empty).
        write_proposed_rules(&memory_dir, &proposals)?;
        Ok(rule_id)
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// Removes the proposal at `index` from memory without persisting any rule.
#[tauri::command]
pub async fn dismiss_proposed_permission_rule(index: usize) -> Result<(), String> {
    let memory_dir = memory_dir();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let raw = read_proposed_rules_raw(&memory_dir)?
            .ok_or_else(|| "no proposed rules in memory".to_string())?;
        let mut proposals = parse_proposals_raw(&raw)?;
        if index >= proposals.len() {
            return Err(format!(
                "proposal index {} out of range (len={})",
                index,
                proposals.len()
            ));
        }
        proposals.remove(index);
        write_proposed_rules(&memory_dir, &proposals)?;
        Ok(())
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn memory_dir() -> PathBuf {
    let home = apollia_core::paths::home_dir_or_temp()
        .display()
        .to_string();
    apollia_core::paths::data_dir_under(home).join("memory")
}

fn governance_dir() -> Result<PathBuf, String> {
    let home = apollia_core::paths::home_string_or_err()?;
    Ok(apollia_core::paths::data_dir_under(home))
}

/// Returns the JSON string stored at `onboarding.proposed_rules`, scanning
/// both candidate namespaces. `None` when no entry was found.
fn read_proposed_rules_raw(memory_dir: &Path) -> Result<Option<String>, String> {
    for (filename, namespace) in NAMESPACE_CANDIDATES {
        let db_path = memory_dir.join(filename);
        if !db_path.exists() {
            continue;
        }
        let store = MemoryStore::open(&db_path)
            .map_err(|e| format!("failed to open {}: {e}", db_path.display()))?;
        let sem = SemanticMemory::new(&store);
        let entries = sem
            .recall_all(namespace, None)
            .map_err(|e| format!("recall_all failed: {e}"))?;
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
fn write_proposed_rules(memory_dir: &Path, proposals: &[serde_json::Value]) -> Result<(), String> {
    for (filename, namespace) in NAMESPACE_CANDIDATES {
        let db_path = memory_dir.join(filename);
        if !db_path.exists() {
            continue;
        }
        let store = MemoryStore::open(&db_path)
            .map_err(|e| format!("failed to open {}: {e}", db_path.display()))?;
        let sem = SemanticMemory::new(&store);
        if proposals.is_empty() {
            // Drop the entry entirely when there is nothing left to approve.
            let _ = sem.forget(namespace, PROPOSED_RULES_KEY);
        } else {
            let serialised = serde_json::to_string(proposals)
                .map_err(|e| format!("failed to re-serialise proposals: {e}"))?;
            sem.remember(RememberInput {
                namespace,
                key: PROPOSED_RULES_KEY,
                value: &serde_json::Value::String(serialised),
                confidence: 0.9,
                source: Some(CREATOR),
                expires_at: None,
            })
            .map_err(|e| format!("failed to persist truncated list: {e}"))?;
        }
        return Ok(());
    }
    Err("no onboarding namespace database found".to_string())
}

/// Parses + projects the raw JSON list into [`ProposedRuleView`] items
/// indexed by their position. Tolerant: invalid items are silently skipped.
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

/// Parses the raw JSON into a list of `Value`s, used both for projection and
/// for write-back (we re-serialise the list minus the chosen item).
fn parse_proposals_raw(raw: &str) -> Result<Vec<serde_json::Value>, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("invalid proposals JSON: {e}"))?;
    match value {
        serde_json::Value::Array(items) => Ok(items),
        _ => Err("proposed_rules is not a JSON array".to_string()),
    }
}

/// Persists one proposal as a real rule in `governance.db`, stamped with
/// `created_by_agent="onboarding-agent"`. Bypasses the tool dispatcher.
fn persist_chosen_rule(base_dir: &Path, proposal: &serde_json::Value) -> Result<i64, String> {
    let obj = proposal
        .as_object()
        .ok_or_else(|| "proposal is not a JSON object".to_string())?;
    let tool_name = obj
        .get("tool_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing tool_name".to_string())?
        .to_owned();
    // The rule store accepts any tool name, and a name absent from the
    // registry can never match an invocation: the rule would be displayed as
    // effective while being dead. Persist anyway (the registry is not the
    // only historical source of names) but leave a trace.
    if !apollia_tools::NATIVE_TOOL_NAMES.contains(&tool_name.as_str()) {
        tracing::warn!(
            tool = %tool_name,
            "permissions.proposal_unknown_tool"
        );
    }
    let action_str = obj
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing action".to_string())?;
    let action = match action_str {
        "allow" => RuleAction::Allow,
        "deny" => RuleAction::Deny,
        other => return Err(format!("unknown action: {other}")),
    };
    let arg_prefix = obj
        .get("arg_prefix")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let scope_str = obj
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("global");
    let scope = match scope_str {
        "global" => PermissionScope::Global,
        "project" => PermissionScope::Project,
        "agent" => PermissionScope::Agent,
        // session is in-memory only and never persisted to governance.db.
        other => return Err(format!("scope '{other}' not persistable")),
    };

    // Ensure schema is migrated, then open the engine.
    let _migrate = GovernanceDb::open(base_dir)
        .map_err(|e| format!("failed to open governance database: {e}"))?;
    let db_path = base_dir.join(GOVERNANCE_DB_FILENAME);
    let mut engine = PrefixRuleEngine::new(&db_path)
        .map_err(|e| format!("failed to open prefix rule engine: {e}"))?;

    let rule = PrefixRule {
        tool_name,
        arg_prefix,
        action,
        created_at: chrono::Utc::now().timestamp(),
        scope,
        project_path: None,
        agent_id: None,
        created_by_agent: Some(CREATOR.to_owned()),
        ..PrefixRule::default()
    };
    engine
        .add_rule(&rule)
        .map_err(|e| format!("failed to persist prefix rule: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_proposals_extracts_fields() {
        let raw = r#"[
          {"tool_name":"http_fetch","action":"deny","arg_prefix":"https://","scope":"global"},
          {"tool_name":"file_read","action":"allow","scope":"global"}
        ]"#;
        let views = parse_proposals(raw);
        assert_eq!(views.len(), 2);
        assert_eq!(views[0].index, 0);
        assert_eq!(views[0].tool_name, "http_fetch");
        assert_eq!(views[0].action, "deny");
        assert_eq!(views[0].arg_prefix.as_deref(), Some("https://"));
        assert_eq!(views[1].index, 1);
        assert_eq!(views[1].arg_prefix, None);
    }

    #[test]
    fn parse_proposals_tolerates_invalid_items() {
        let raw = r#"[{"tool_name":"ok","action":"allow"}, "garbage", 42]"#;
        let views = parse_proposals(raw);
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].tool_name, "ok");
    }

    #[test]
    fn parse_proposals_empty_on_garbage() {
        assert!(parse_proposals("not json").is_empty());
    }
}
