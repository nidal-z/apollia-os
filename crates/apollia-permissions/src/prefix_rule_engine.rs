//! Couche 2 du moteur de permissions — PrefixRuleEngine SQLite.
//!
//! Persiste des règles Allow/Deny par préfixe d'argument dans SQLite.
//! Permet à l'opérateur (ou au bouton "Toujours autoriser" HITL desktop)
//! d'ajouter des règles qui survivent aux redémarrages du runtime.
//!
//! Schéma SQLite :
//! ```sql
//! CREATE TABLE permission_rules (
//!     id          INTEGER PRIMARY KEY AUTOINCREMENT,
//!     tool_name   TEXT NOT NULL,
//!     arg_prefix  TEXT,
//!     action      TEXT NOT NULL,  -- 'allow' ou 'deny'
//!     created_at  INTEGER NOT NULL,
//!     created_by  TEXT
//! );
//! ```

use std::path::Path;

use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::error::PermissionError;
use crate::migrations::add_column_if_missing;

// ─────────────────────────────────────────────
// Types publics
// ─────────────────────────────────────────────

/// Action d'une règle de préfixe — Allow ou Deny.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuleAction {
    /// Auto-approuver les invocations correspondant à cette règle.
    Allow,
    /// Auto-refuser les invocations correspondant à cette règle.
    Deny,
}

impl RuleAction {
    fn as_str(&self) -> &'static str {
        match self {
            RuleAction::Allow => "allow",
            RuleAction::Deny => "deny",
        }
    }

    fn from_str(s: &str) -> Result<Self, PermissionError> {
        match s {
            "allow" => Ok(RuleAction::Allow),
            "deny" => Ok(RuleAction::Deny),
            other => Err(PermissionError::InvalidRule(format!(
                "unknown action '{other}', expected 'allow' or 'deny'"
            ))),
        }
    }
}

/// Règle de préfixe persistée dans SQLite.
///
/// Une règle associe un nom d'outil et un préfixe d'argument optionnel à une action.
/// `arg_prefix = None` signifie que la règle s'applique à tout argument.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefixRule {
    /// Identifiant unique (AUTOINCREMENT SQLite).
    pub id: i64,
    /// Nom de l'outil ciblé.
    pub tool_name: String,
    /// Préfixe de l'argument à matcher (None = tout argument).
    pub arg_prefix: Option<String>,
    /// Action à appliquer si la règle correspond.
    pub action: RuleAction,
    /// Timestamp de création (Unix epoch, secondes).
    pub created_at: i64,
    /// Nom de l'agent ayant créé la règle (None = opérateur humain).
    pub created_by_agent: Option<String>,
}

// ─────────────────────────────────────────────
// PrefixRuleEngine
// ─────────────────────────────────────────────

/// Moteur de règles préfixe persistées en SQLite (couche 2).
///
/// Gère le CRUD des règles et évalue les invocations d'outils
/// en cherchant la règle la plus spécifique (préfixe le plus long) en premier.
pub struct PrefixRuleEngine {
    db: Connection,
}

impl PrefixRuleEngine {
    /// Ouvre (ou crée) la base SQLite au chemin indiqué et migre le schéma.
    ///
    /// # Errors
    ///
    /// Retourne [`PermissionError::Database`] si l'ouverture ou la migration échoue.
    pub fn new(db_path: &Path) -> Result<Self, PermissionError> {
        let db = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        db.execute_batch("PRAGMA journal_mode=WAL;")?;
        let engine = Self { db };
        engine.migrate()?;
        Ok(engine)
    }

    /// Vérifie si l'invocation (`tool_name`, `first_arg`) correspond à une règle persistée.
    ///
    /// Les règles sont évaluées par ordre décroissant de spécificité :
    /// - préfixe le plus long en premier,
    /// - puis les règles sans préfixe (None).
    ///
    /// Retourne la première action correspondante, ou `None` si aucune règle ne correspond.
    ///
    /// # Errors
    ///
    /// Retourne [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn check(
        &self,
        tool_name: &str,
        first_arg: Option<&str>,
    ) -> Result<Option<RuleAction>, PermissionError> {
        // Récupère toutes les règles pour cet outil, les plus spécifiques en premier.
        let mut stmt = self.db.prepare_cached(
            "SELECT id, arg_prefix, action FROM permission_rules \
             WHERE tool_name = ? \
             ORDER BY CASE WHEN arg_prefix IS NULL THEN 0 ELSE LENGTH(arg_prefix) END DESC",
        )?;

        let mut rows = stmt.query(params![tool_name])?;
        while let Some(row) = rows.next()? {
            let arg_prefix: Option<String> = row.get(1)?;
            let action_str: String = row.get(2)?;
            let action = RuleAction::from_str(&action_str)?;

            let matches = match (&arg_prefix, first_arg) {
                // Règle sans filtre → s'applique à tout.
                (None, _) => true,
                // Règle avec préfixe → vérifier que l'argument commence par ce préfixe.
                (Some(prefix), Some(arg)) => arg.starts_with(prefix.as_str()),
                // Règle avec préfixe mais aucun argument fourni → pas de correspondance.
                (Some(_), None) => false,
            };

            if matches {
                // Retourner l'id pour permettre à l'engine de logguer rule_id.
                let id: i64 = row.get(0)?;
                tracing::debug!(
                    tool = %tool_name,
                    rule_id = id,
                    action = ?action,
                    "prefix rule matched"
                );
                return Ok(Some(action));
            }
        }

        Ok(None)
    }

    /// Retourne l'id de la première règle correspondante, en plus de l'action.
    ///
    /// Variante de [`check`](Self::check) qui expose l'identifiant de la règle
    /// pour permettre à l'`PermissionEngine` de peupler `AutoAllowedPrefixRule.rule_id`.
    pub fn check_with_id(
        &self,
        tool_name: &str,
        first_arg: Option<&str>,
    ) -> Result<Option<(i64, RuleAction)>, PermissionError> {
        let mut stmt = self.db.prepare_cached(
            "SELECT id, arg_prefix, action FROM permission_rules \
             WHERE tool_name = ? \
             ORDER BY CASE WHEN arg_prefix IS NULL THEN 0 ELSE LENGTH(arg_prefix) END DESC",
        )?;

        let mut rows = stmt.query(params![tool_name])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let arg_prefix: Option<String> = row.get(1)?;
            let action_str: String = row.get(2)?;
            let action = RuleAction::from_str(&action_str)?;

            let matches = match (&arg_prefix, first_arg) {
                (None, _) => true,
                (Some(prefix), Some(arg)) => arg.starts_with(prefix.as_str()),
                (Some(_), None) => false,
            };

            if matches {
                return Ok(Some((id, action)));
            }
        }

        Ok(None)
    }

    /// Persiste une nouvelle règle et retourne son identifiant auto-incrémenté.
    ///
    /// # Errors
    ///
    /// - [`PermissionError::InvalidRule`] si `tool_name` est vide.
    /// - [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn add_rule(&mut self, rule: &PrefixRule) -> Result<i64, PermissionError> {
        if rule.tool_name.trim().is_empty() {
            return Err(PermissionError::InvalidRule(
                "tool_name must not be empty".to_string(),
            ));
        }

        self.db.execute(
            "INSERT INTO permission_rules (tool_name, arg_prefix, action, created_at, created_by) \
             VALUES (?, ?, ?, ?, ?)",
            params![
                rule.tool_name,
                rule.arg_prefix,
                rule.action.as_str(),
                rule.created_at,
                rule.created_by_agent,
            ],
        )?;

        Ok(self.db.last_insert_rowid())
    }

    /// Supprime la règle identifiée par `id`.
    ///
    /// Silencieux si la règle n'existe pas.
    ///
    /// # Errors
    ///
    /// Retourne [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn remove_rule(&mut self, id: i64) -> Result<(), PermissionError> {
        self.db
            .execute("DELETE FROM permission_rules WHERE id = ?", params![id])?;
        Ok(())
    }

    /// Retourne toutes les règles persistées, triées par identifiant croissant.
    ///
    /// # Errors
    ///
    /// Retourne [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn list_rules(&self) -> Result<Vec<PrefixRule>, PermissionError> {
        let mut stmt = self.db.prepare_cached(
            "SELECT id, tool_name, arg_prefix, action, created_at, created_by \
             FROM permission_rules ORDER BY id ASC",
        )?;

        let rules = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })?
            .map(|r| {
                let (id, tool_name, arg_prefix, action_str, created_at, created_by_agent) = r?;
                let action = RuleAction::from_str(&action_str)?;
                Ok(PrefixRule {
                    id,
                    tool_name,
                    arg_prefix,
                    action,
                    created_at,
                    created_by_agent,
                })
            })
            .collect::<Result<Vec<_>, PermissionError>>()?;

        Ok(rules)
    }

    // ─────────────────────────────────────────────
    // Privé
    // ─────────────────────────────────────────────

    fn migrate(&self) -> Result<(), PermissionError> {
        self.db.execute_batch(
            "CREATE TABLE IF NOT EXISTS permission_rules (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                tool_name    TEXT NOT NULL,
                arg_prefix   TEXT,
                action       TEXT NOT NULL,
                created_at   INTEGER NOT NULL,
                created_by   TEXT,
                scope        TEXT NOT NULL DEFAULT 'global',
                project_path TEXT,
                expires_at   INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_rules_tool ON permission_rules(tool_name);",
        )?;

        add_column_if_missing(
            &self.db,
            "permission_rules",
            "scope",
            "TEXT NOT NULL DEFAULT 'global'",
        )?;
        add_column_if_missing(&self.db, "permission_rules", "project_path", "TEXT")?;
        add_column_if_missing(&self.db, "permission_rules", "expires_at", "INTEGER")?;

        Ok(())
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn tmp_engine() -> (PrefixRuleEngine, NamedTempFile) {
        let file = NamedTempFile::new().expect("tempfile");
        let engine = PrefixRuleEngine::new(file.path()).expect("engine init");
        (engine, file)
    }

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }

    #[test]
    fn prefix_rule_matches_git_wildcard() {
        // GIVEN un PrefixRuleEngine avec règle Allow "bash_executor(git:)"
        let (mut engine, _tmp) = tmp_engine();
        let rule = PrefixRule {
            id: 0,
            tool_name: "bash_executor".into(),
            arg_prefix: Some("git".into()),
            action: RuleAction::Allow,
            created_at: now(),
            created_by_agent: None,
        };
        engine.add_rule(&rule).expect("add_rule");
        // WHEN
        let result = engine
            .check("bash_executor", Some("git push origin main"))
            .expect("check");
        // THEN
        assert_eq!(result, Some(RuleAction::Allow));
    }

    #[test]
    fn prefix_rule_no_match_returns_none() {
        // GIVEN un PrefixRuleEngine vide
        let (engine, _tmp) = tmp_engine();
        // WHEN
        let result = engine
            .check("bash_executor", Some("git status"))
            .expect("check");
        // THEN
        assert!(result.is_none());
    }

    #[test]
    fn prefix_rule_deny_action_returned() {
        let (mut engine, _tmp) = tmp_engine();
        let rule = PrefixRule {
            id: 0,
            tool_name: "file_write".into(),
            arg_prefix: Some("/etc".into()),
            action: RuleAction::Deny,
            created_at: now(),
            created_by_agent: None,
        };
        engine.add_rule(&rule).expect("add_rule");
        let result = engine
            .check("file_write", Some("/etc/passwd"))
            .expect("check");
        assert_eq!(result, Some(RuleAction::Deny));
    }

    #[test]
    fn prefix_rule_none_prefix_matches_any_arg() {
        // GIVEN une règle sans filtre d'argument (None)
        let (mut engine, _tmp) = tmp_engine();
        let rule = PrefixRule {
            id: 0,
            tool_name: "file_read".into(),
            arg_prefix: None,
            action: RuleAction::Allow,
            created_at: now(),
            created_by_agent: None,
        };
        engine.add_rule(&rule).expect("add_rule");
        // WHEN / THEN
        assert_eq!(
            engine
                .check("file_read", Some("any/path.txt"))
                .expect("check"),
            Some(RuleAction::Allow)
        );
        assert_eq!(
            engine.check("file_read", None).expect("check"),
            Some(RuleAction::Allow)
        );
    }

    #[test]
    fn remove_rule_cleans_up() {
        let (mut engine, _tmp) = tmp_engine();
        let rule = PrefixRule {
            id: 0,
            tool_name: "bash_executor".into(),
            arg_prefix: Some("git".into()),
            action: RuleAction::Allow,
            created_at: now(),
            created_by_agent: None,
        };
        let id = engine.add_rule(&rule).expect("add_rule");
        engine.remove_rule(id).expect("remove_rule");
        let result = engine
            .check("bash_executor", Some("git status"))
            .expect("check");
        assert!(result.is_none());
    }

    #[test]
    fn list_rules_returns_all() {
        let (mut engine, _tmp) = tmp_engine();
        for (tool, prefix) in [("tool_a", "prefix_x"), ("tool_b", "prefix_y")] {
            let rule = PrefixRule {
                id: 0,
                tool_name: tool.into(),
                arg_prefix: Some(prefix.into()),
                action: RuleAction::Allow,
                created_at: now(),
                created_by_agent: None,
            };
            engine.add_rule(&rule).expect("add_rule");
        }
        let rules = engine.list_rules().expect("list_rules");
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn add_rule_rejects_empty_tool_name() {
        let (mut engine, _tmp) = tmp_engine();
        let rule = PrefixRule {
            id: 0,
            tool_name: "".into(),
            arg_prefix: None,
            action: RuleAction::Allow,
            created_at: now(),
            created_by_agent: None,
        };
        assert!(matches!(
            engine.add_rule(&rule),
            Err(PermissionError::InvalidRule(_))
        ));
    }
}
