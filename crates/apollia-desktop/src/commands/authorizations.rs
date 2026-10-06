//! Tauri IPC commands behind the authorizations screen and the sheet of an
//! agent: the standing consents ("always" for one agent and one action
//! family) and the fine rules that target one agent.
//!
//! The four commands read and write `governance.db` of this machine and tell
//! the governance actor of the runtime running in this process to read it
//! again. They have no route on the daemon yet, so the attached mode refuses
//! them before their body runs. The rules for every agent go through
//! `tool_governance` and `hitl`, which are served in both modes.

use std::path::{Path, PathBuf};

use apollia_permissions::{
    is_code_executor, is_single_simple_command, ConsentFamily, ConsentScope, FamilyConsentStore,
    PermissionScope, PrefixRule, PrefixRuleEngine, RuleAction,
};
use apollia_runtime::embedded::RuntimeHandle;
use apollia_runtime::governance::{ConsentAnswer, GovernanceContract, GovernanceEvent};
use apollia_tools::{ActionFamily, GovernanceDb};
use serde::{Deserialize, Serialize};
use tauri::State;

/// What the journal names as the access of a consent given back from the
/// authorizations screen, where no tool call is being answered.
const RESTORED_FROM_SCREEN: &str = "authorizations";

/// One standing consent, as the authorizations screen lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyConsentDto {
    /// Agent the consent is given to.
    pub agent_id: String,
    /// Family it covers, by its wire name.
    pub family: String,
    /// When it was given (Unix epoch, seconds).
    pub granted_at: i64,
    /// Who gave it (`user`, `upgrade`).
    pub granted_by: String,
}

/// Opens (and migrates if needed) `governance.db`, then returns its path.
fn governance_db_path() -> Result<PathBuf, String> {
    let home = apollia_core::paths::home_string_or_err()?;
    let base = apollia_core::paths::data_dir_under(home);
    let db = GovernanceDb::open(&base)
        .map_err(|e| format!("failed to open governance database: {e}"))?;
    Ok(db.path().to_path_buf())
}

fn list_consents_at(db: &Path) -> Result<Vec<FamilyConsentDto>, String> {
    let store = FamilyConsentStore::new(db).map_err(|e| e.to_string())?;
    let consents = store.list_permanent_consents().map_err(|e| e.to_string())?;
    Ok(consents
        .into_iter()
        .map(|c| FamilyConsentDto {
            agent_id: c.agent_id,
            family: c.family.as_str().to_owned(),
            granted_at: c.granted_at,
            granted_by: c.granted_by,
        })
        .collect())
}

fn revoke_consent_at(db: &Path, agent_id: &str, family: &str) -> Result<(), String> {
    let family = ConsentFamily::parse(family).map_err(|e| e.to_string())?;
    let mut store = FamilyConsentStore::new(db).map_err(|e| e.to_string())?;
    let removed = store
        .revoke(agent_id, family, &ConsentScope::Permanent)
        .map_err(|e| e.to_string())?;
    if removed {
        Ok(())
    } else {
        Err(format!(
            "no standing consent for agent '{agent_id}' and family '{}'",
            family.as_str()
        ))
    }
}

fn add_agent_rule_at(
    db: &Path,
    agent_id: &str,
    tool_name: &str,
    arg_prefix: Option<String>,
    action: &str,
) -> Result<i64, String> {
    if agent_id.trim().is_empty() {
        return Err("agent_id must not be empty".to_owned());
    }
    if tool_name.trim().is_empty() {
        return Err("tool_name must not be empty".to_owned());
    }
    let action = match action {
        "allow" => RuleAction::Allow,
        "deny" => RuleAction::Deny,
        other => {
            return Err(format!(
                "unknown action '{other}', expected 'allow' or 'deny'"
            ))
        }
    };
    let arg_prefix = arg_prefix
        .map(|p| p.trim().to_owned())
        .filter(|p| !p.is_empty());
    if action == RuleAction::Allow && is_code_executor(tool_name) {
        let simple = arg_prefix.as_deref().is_some_and(is_single_simple_command);
        if !simple {
            return Err(format!(
                "an allow rule on '{tool_name}' needs a single simple command as its prefix"
            ));
        }
    }
    let rule = PrefixRule {
        tool_name: tool_name.to_owned(),
        arg_prefix,
        action,
        created_at: chrono::Utc::now().timestamp(),
        scope: PermissionScope::Agent,
        agent_id: Some(agent_id.to_owned()),
        ..PrefixRule::default()
    };
    let mut engine = PrefixRuleEngine::new(db).map_err(|e| e.to_string())?;
    engine.add_rule(&rule).map_err(|e| e.to_string())
}

/// Runs a blocking store operation off the async executor.
async fn blocking<T, F>(operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?
}

/// Lists every permanent standing consent, whatever the agent.
///
/// # Errors
///
/// Returns a serializable error if `governance.db` cannot be opened or read.
#[tauri::command]
pub async fn list_family_consents(
    _state: State<'_, RuntimeHandle>,
) -> Result<Vec<FamilyConsentDto>, String> {
    blocking(|| list_consents_at(&governance_db_path()?)).await
}

/// Withdraws the standing consent of `agent_id` for `family`. The withdrawal
/// reaches the journal, and the next call of that family asks again.
///
/// # Errors
///
/// Returns a serializable error if the family is unknown, if no such consent
/// exists, or if `governance.db` cannot be written.
#[tauri::command]
pub async fn revoke_family_consent(
    state: State<'_, RuntimeHandle>,
    agent_id: String,
    family: String,
) -> Result<(), String> {
    let logged_family = family.clone();
    blocking(move || revoke_consent_at(&governance_db_path()?, &agent_id, &family)).await?;
    state.governance.refresh().await;
    tracing::event!(
        tracing::Level::INFO,
        family = %logged_family,
        "authorizations.consent.revoked"
    );
    Ok(())
}

/// Gives back the standing consent of `agent_id` for `family`, the undo of a
/// withdrawal made on the authorizations screen. Recorded in the journal as a
/// consent answered "always".
///
/// # Errors
///
/// Returns a serializable error if the family cannot be consented to for
/// good, or if the consent cannot be written.
#[tauri::command]
pub async fn restore_family_consent(
    state: State<'_, RuntimeHandle>,
    agent_id: String,
    family: String,
) -> Result<(), String> {
    let action_family: ActionFamily =
        serde_json::from_value(serde_json::Value::String(family.clone()))
            .map_err(|e| format!("unknown family '{family}': {e}"))?;
    state
        .governance
        .grant_always(&agent_id, action_family)
        .await
        .map_err(|e| e.to_string())?;
    if let Err(error) = state.governance.record(&GovernanceEvent::ConsentAnswered {
        agent_id,
        access_id: RESTORED_FROM_SCREEN.to_owned(),
        family: action_family,
        answer: ConsentAnswer::Always,
    }) {
        tracing::event!(
            tracing::Level::WARN,
            error = %error,
            "authorizations.consent.restore_not_journaled"
        );
    }
    tracing::event!(
        tracing::Level::INFO,
        family = %family,
        "authorizations.consent.restored"
    );
    Ok(())
}

/// Adds a fine rule that targets one agent. An `allow` on a tool that runs
/// code is refused unless its prefix is a single simple command.
///
/// # Errors
///
/// Returns a serializable error if the agent, the tool or the action is
/// invalid, or if `governance.db` cannot be written.
#[tauri::command]
pub async fn add_agent_permission_rule(
    state: State<'_, RuntimeHandle>,
    agent_id: String,
    tool_name: String,
    arg_prefix: Option<String>,
    action: String,
) -> Result<i64, String> {
    let rule_id = blocking(move || {
        add_agent_rule_at(
            &governance_db_path()?,
            &agent_id,
            &tool_name,
            arg_prefix,
            &action,
        )
    })
    .await?;
    state.governance.refresh().await;
    tracing::event!(
        tracing::Level::INFO,
        rule_id,
        "authorizations.agent_rule.added"
    );
    Ok(rule_id)
}

#[cfg(test)]
mod tests {
    use apollia_permissions::JournalOutbox;
    use tempfile::NamedTempFile;

    use super::*;

    fn database() -> NamedTempFile {
        NamedTempFile::new().expect("tempfile")
    }

    #[test]
    fn test_a_withdrawn_consent_leaves_the_listing_and_waits_for_the_journal() {
        // GIVEN two agents holding a standing consent each
        let file = database();
        let mut store = FamilyConsentStore::new(file.path()).expect("store");
        store
            .grant(
                "writer",
                ConsentFamily::SendExternal,
                &ConsentScope::Permanent,
                "user",
            )
            .expect("grant");
        store
            .grant(
                "sorter",
                ConsentFamily::WriteOrDelete,
                &ConsentScope::Permanent,
                "upgrade",
            )
            .expect("grant");
        drop(store);

        // WHEN the consent of the first one is withdrawn
        revoke_consent_at(file.path(), "writer", "send_external").expect("revoke");

        // THEN only the other is listed, with who gave it, and the withdrawal
        // waits in the outbox the journal is fed from
        let listed = list_consents_at(file.path()).expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].agent_id, "sorter");
        assert_eq!(listed[0].family, "write_or_delete");
        assert_eq!(listed[0].granted_by, "upgrade");
        let pending = JournalOutbox::new(file.path())
            .expect("outbox")
            .pending()
            .expect("pending");
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn test_withdrawing_a_consent_nobody_holds_is_refused() {
        // GIVEN an empty store
        let file = database();

        // WHEN a consent that does not exist is withdrawn, and one of an
        // unknown family
        let absent = revoke_consent_at(file.path(), "writer", "send_external");
        let unknown = revoke_consent_at(file.path(), "writer", "session");

        // THEN both are refused with words
        assert!(absent.expect_err("absent").contains("no standing consent"));
        assert!(unknown.is_err());
    }

    #[test]
    fn test_an_agent_rule_is_stored_for_that_agent_only() {
        // GIVEN an empty rule store
        let file = database();

        // WHEN a deny rule is added for one agent
        let id = add_agent_rule_at(
            file.path(),
            "writer",
            "bash_executor",
            Some(" git push ".to_owned()),
            "deny",
        )
        .expect("add");

        // THEN the rule is the agent's, with its prefix trimmed
        let rules = PrefixRuleEngine::new(file.path())
            .expect("engine")
            .list_rules_for_agent("writer")
            .expect("list");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, id);
        assert_eq!(rules[0].scope, PermissionScope::Agent);
        assert_eq!(rules[0].arg_prefix.as_deref(), Some("git push"));
        assert_eq!(rules[0].action, RuleAction::Deny);
    }

    #[test]
    fn test_an_allow_on_a_code_executor_needs_a_single_simple_command() {
        // GIVEN an empty rule store
        let file = database();

        // WHEN allow rules are added on a tool that runs code: with no
        // prefix, with a chained command, then with a simple command
        let blanket = add_agent_rule_at(file.path(), "writer", "bash_executor", None, "allow");
        let chained = add_agent_rule_at(
            file.path(),
            "writer",
            "bash_executor",
            Some("git status && rm -rf /".to_owned()),
            "allow",
        );
        let simple = add_agent_rule_at(
            file.path(),
            "writer",
            "bash_executor",
            Some("git status".to_owned()),
            "allow",
        );

        // THEN only the simple command is stored
        assert!(blanket.is_err());
        assert!(chained.is_err());
        assert!(simple.is_ok());
        let rules = PrefixRuleEngine::new(file.path())
            .expect("engine")
            .list_rules()
            .expect("list");
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn test_a_rule_without_agent_tool_or_known_action_is_refused() {
        // GIVEN an empty rule store
        let file = database();

        // WHEN a rule misses its agent, its tool, or names an unknown action
        let no_agent = add_agent_rule_at(file.path(), " ", "file_read", None, "deny");
        let no_tool = add_agent_rule_at(file.path(), "writer", "", None, "deny");
        let bad_action = add_agent_rule_at(file.path(), "writer", "file_read", None, "ask");

        // THEN each is refused and nothing is stored
        assert!(no_agent.is_err());
        assert!(no_tool.is_err());
        assert!(bad_action.is_err());
        let rules = PrefixRuleEngine::new(file.path())
            .expect("engine")
            .list_rules()
            .expect("list");
        assert!(rules.is_empty());
    }
}
