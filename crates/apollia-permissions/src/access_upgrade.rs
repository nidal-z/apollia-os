//! One-shot upgrade of the accesses persisted before standing consents.
//!
//! Before standing consents existed, "always" was a fine rule without an
//! argument prefix, keyed by tool name, and fine rules could target a project
//! or a session. The upgrade rewrites those rows once, in one transaction:
//!
//! - an Allow without prefix for one agent becomes a standing consent of that
//!   agent for the family of the tool;
//! - an Allow without prefix for every agent or for a project becomes a
//!   standing consent of Apollia ([`APOLLIA_AGENT_ID`]), the only agent that
//!   ever read them;
//! - the pre-authorized tools of the built-in chat become standing consents of
//!   Apollia;
//! - a rule on a code executor never becomes a standing consent, and an Allow
//!   whose family cannot be determined (an MCP tool of an absent server, an
//!   unclassified tool) or that carried an expiry is removed, never widened;
//! - a prefixed project rule becomes a rule for every agent when it denies and
//!   is removed when it allows; a session row is removed;
//! - every prefixed rule and every Deny otherwise stays a fine rule.
//!
//! The family of a tool comes from its descriptor, which this crate cannot
//! see, so the caller passes a resolver. The upgrade returns every conversion
//! and removal so the caller can journal them. A second run does nothing: the
//! `access_upgrade` row of `governance_upgrades` is stamped in the same
//! transaction.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::error::PermissionError;
use crate::executor_guard::is_code_executor;
use crate::family_consent::{
    insert_consent, ConsentFamily, ConsentScope, NewConsent, APOLLIA_AGENT_ID,
};

/// Agent identifier recorded for a change that concerns every agent.
pub const ALL_AGENTS: &str = "*";

/// `granted_by` of the consents the upgrade writes.
pub const UPGRADE_GRANTOR: &str = "upgrade";

/// Name of the upgrade in `governance_upgrades`.
const UPGRADE_NAME: &str = "access_upgrade";

/// What the upgrade did to one access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeChange {
    /// A standing consent was created.
    Granted,
    /// A rule was removed.
    Revoked,
    /// A rule was kept with another reach (a project Deny now for every agent).
    Rescoped,
}

/// One conversion or removal made by the upgrade.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpgradeEntry {
    /// Agent concerned, or [`ALL_AGENTS`].
    pub agent_id: String,
    /// The access: `family:<family>` for a consent, `rule:<tool>[ <prefix>]`
    /// for a rule.
    pub access_id: String,
    /// What happened to it.
    pub change: UpgradeChange,
    /// Why, in one sentence.
    pub detail: String,
}

/// Everything the upgrade changed, in the order it changed it. Empty when the
/// upgrade had already run or found nothing to change.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessUpgradeReport {
    /// The changes.
    pub entries: Vec<UpgradeEntry>,
}

/// Runs the upgrade on the `governance.db` at `db_path`, once.
///
/// `family_of` returns the family of a tool from its descriptor, or `None`
/// when the descriptor is unknown or carries no usable family.
///
/// # Errors
///
/// [`PermissionError::Database`] or [`PermissionError::Schema`] when the file
/// cannot be opened, migrated or written; nothing is changed in that case.
pub fn run_access_upgrade(
    db_path: &Path,
    family_of: &dyn Fn(&str) -> Option<ConsentFamily>,
) -> Result<AccessUpgradeReport, PermissionError> {
    let mut conn = Connection::open(db_path)?;
    crate::governance_schema::open_governance_schema(&conn)?;
    upgrade_connection(&mut conn, family_of, crate::prefix_rule_engine::unix_now())
}

/// A persisted rule as the upgrade reads it, whatever its scope.
struct RawRule {
    id: i64,
    tool_name: String,
    arg_prefix: Option<String>,
    allow: bool,
    scope: String,
    agent_id: Option<String>,
    expires_at: Option<i64>,
}

impl RawRule {
    fn access_id(&self) -> String {
        match &self.arg_prefix {
            Some(p) => format!("rule:{} {p}", self.tool_name),
            None => format!("rule:{}", self.tool_name),
        }
    }
}

fn upgrade_connection(
    conn: &mut Connection,
    family_of: &dyn Fn(&str) -> Option<ConsentFamily>,
    now: i64,
) -> Result<AccessUpgradeReport, PermissionError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let pending: Option<Option<i64>> = tx
        .query_row(
            "SELECT done_at FROM governance_upgrades WHERE name = ?",
            params![UPGRADE_NAME],
            |row| row.get(0),
        )
        .optional()?;
    if !matches!(pending, Some(None)) {
        return Ok(AccessUpgradeReport::default());
    }

    let mut upgrader = Upgrader {
        conn: &tx,
        family_of,
        now,
        report: AccessUpgradeReport::default(),
    };
    for rule in read_rules(&tx)? {
        upgrader.upgrade_rule(&rule)?;
    }
    for tool in read_chat_preauthorized(&tx)? {
        let family = match family_of(&tool) {
            Some(f) if f != ConsentFamily::ExecuteCode && !is_code_executor(&tool) => f,
            _ => continue,
        };
        upgrader.grant(
            APOLLIA_AGENT_ID,
            family,
            &format!("tool {tool} was pre-authorized in the chat"),
        )?;
    }
    let report = upgrader.report;

    tx.execute(
        "UPDATE governance_upgrades SET done_at = ? WHERE name = ?",
        params![now, UPGRADE_NAME],
    )?;
    tx.commit()?;
    tracing::event!(
        tracing::Level::INFO,
        changes = report.entries.len(),
        "permission.access_upgrade.done"
    );
    Ok(report)
}

fn read_rules(conn: &Connection) -> Result<Vec<RawRule>, PermissionError> {
    let mut stmt = conn.prepare(
        "SELECT id, tool_name, arg_prefix, action, scope, agent_id, expires_at \
         FROM permission_rules ORDER BY id ASC",
    )?;
    let rules = stmt
        .query_map([], |row| {
            Ok(RawRule {
                id: row.get(0)?,
                tool_name: row.get(1)?,
                arg_prefix: row.get(2)?,
                allow: row.get::<_, String>(3)? == "allow",
                scope: row.get(4)?,
                agent_id: row.get(5)?,
                expires_at: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rules)
}

fn read_chat_preauthorized(conn: &Connection) -> Result<Vec<String>, PermissionError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT allowed_tools FROM chat_libre_config WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let tools: Vec<String> = raw
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    Ok(tools)
}

/// The upgrade in progress: the transaction it writes in, the family
/// resolver, its clock, and what it has changed so far.
struct Upgrader<'a> {
    conn: &'a Connection,
    family_of: &'a dyn Fn(&str) -> Option<ConsentFamily>,
    now: i64,
    report: AccessUpgradeReport,
}

impl Upgrader<'_> {
    fn upgrade_rule(&mut self, rule: &RawRule) -> Result<(), PermissionError> {
        match (rule.scope.as_str(), rule.arg_prefix.is_some(), rule.allow) {
            ("session", _, _) => {
                let agent = rule
                    .agent_id
                    .clone()
                    .unwrap_or_else(|| ALL_AGENTS.to_string());
                self.remove(rule, agent, "session rule, sessions no longer hold rules")
            }
            ("project", true, true) => self.remove(
                rule,
                ALL_AGENTS.to_string(),
                "project rule, an allow is not carried to every agent",
            ),
            ("project", _, false) => {
                self.conn.execute(
                    "UPDATE permission_rules SET scope = 'global', project_path = NULL WHERE id = ?",
                    params![rule.id],
                )?;
                self.report.entries.push(UpgradeEntry {
                    agent_id: ALL_AGENTS.to_string(),
                    access_id: rule.access_id(),
                    change: UpgradeChange::Rescoped,
                    detail: "project deny rule now applies to every agent".to_string(),
                });
                Ok(())
            }
            ("agent", false, true) => match rule.agent_id.as_deref().map(str::trim) {
                Some(id) if !id.is_empty() => self.convert(rule, id.to_string()),
                _ => self.remove(rule, ALL_AGENTS.to_string(), "agent rule without an agent"),
            },
            ("global", false, true) | ("project", false, true) => {
                self.convert(rule, APOLLIA_AGENT_ID.to_string())
            }
            _ => Ok(()),
        }
    }

    fn convert(&mut self, rule: &RawRule, target: String) -> Result<(), PermissionError> {
        if is_code_executor(&rule.tool_name) {
            return self.remove(rule, target, "code executor, never an always");
        }
        if rule.expires_at.is_some() {
            return self.remove(rule, target, "temporary rule, not made permanent");
        }
        match (self.family_of)(&rule.tool_name) {
            Some(ConsentFamily::ExecuteCode) => {
                self.remove(rule, target, "executes code, never an always")
            }
            Some(family) => {
                self.remove(
                    rule,
                    target.clone(),
                    &format!("converted to an always for the family {}", family.as_str()),
                )?;
                self.grant(
                    &target,
                    family,
                    &format!("converted from the always rule on {}", rule.tool_name),
                )
            }
            None => self.remove(rule, target, "family of the tool unknown, not widened"),
        }
    }

    fn remove(
        &mut self,
        rule: &RawRule,
        agent_id: String,
        detail: &str,
    ) -> Result<(), PermissionError> {
        self.conn.execute(
            "DELETE FROM permission_rules WHERE id = ?",
            params![rule.id],
        )?;
        self.report.entries.push(UpgradeEntry {
            agent_id,
            access_id: rule.access_id(),
            change: UpgradeChange::Revoked,
            detail: detail.to_string(),
        });
        Ok(())
    }

    fn grant(
        &mut self,
        agent_id: &str,
        family: ConsentFamily,
        detail: &str,
    ) -> Result<(), PermissionError> {
        let created = insert_consent(
            self.conn,
            &NewConsent {
                agent_id,
                family,
                scope: &ConsentScope::Permanent,
                granted_by: UPGRADE_GRANTOR,
                granted_at: self.now,
            },
        )?;
        if created {
            self.report.entries.push(UpgradeEntry {
                agent_id: agent_id.to_string(),
                access_id: format!("family:{}", family.as_str()),
                change: UpgradeChange::Granted,
                detail: detail.to_string(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
