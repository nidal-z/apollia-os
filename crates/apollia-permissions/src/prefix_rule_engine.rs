//! Couche 2 du moteur de permissions — PrefixRuleEngine SQLite.
//!
//! Persiste des règles Allow/Deny par préfixe d'argument dans SQLite.
//! Permet à l'opérateur (ou au bouton "Toujours autoriser" HITL desktop)
//! d'ajouter des règles qui survivent aux redémarrages du runtime.
//!
//! Schéma SQLite :
//! ```sql
//! CREATE TABLE permission_rules (
//!     id           INTEGER PRIMARY KEY AUTOINCREMENT,
//!     tool_name    TEXT NOT NULL,
//!     arg_prefix   TEXT,
//!     action       TEXT NOT NULL,  -- 'allow' ou 'deny'
//!     created_at   INTEGER NOT NULL,
//!     created_by   TEXT,
//!     scope        TEXT NOT NULL DEFAULT 'global', -- 'project' ou 'global'
//!     project_path TEXT,
//!     expires_at   INTEGER
//! );
//! ```

use std::path::{Path, PathBuf};

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

/// Portée d'une règle de permission.
///
/// Détermine où la règle est stockée et comment elle est filtrée lors de l'évaluation.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermissionScope {
    /// Règle vivant uniquement en mémoire dans le `PermissionEngine`.
    ///
    /// Disparaît à l'arrêt du process. Jamais persistée en SQLite.
    Session,
    /// Règle persistée et filtrée par chemin canonique du projet courant.
    ///
    /// Ne s'applique qu'aux invocations émises depuis le projet correspondant.
    Project,
    /// Règle persistée et filtrée par identité de l'agent courant.
    ///
    /// S'applique à tout invocation émise par l'agent dont l'`agent_id` matche.
    /// Indépendante du projet — un agent peut être lancé hors projet.
    Agent,
    /// Règle persistée s'appliquant à n'importe quel projet.
    #[default]
    Global,
}

impl PermissionScope {
    /// Représentation textuelle stockée en base de données.
    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionScope::Session => "session",
            PermissionScope::Project => "project",
            PermissionScope::Agent => "agent",
            PermissionScope::Global => "global",
        }
    }

    /// Parse la valeur stockée en base — par défaut `Global` pour les colonnes nulles.
    fn from_db_str(s: &str) -> Result<Self, PermissionError> {
        match s {
            "session" => Ok(PermissionScope::Session),
            "project" => Ok(PermissionScope::Project),
            "agent" => Ok(PermissionScope::Agent),
            "global" => Ok(PermissionScope::Global),
            other => Err(PermissionError::InvalidRule(format!(
                "unknown scope '{other}', expected 'session' | 'project' | 'agent' | 'global'"
            ))),
        }
    }
}

/// Contexte d'évaluation d'une règle scope-aware.
///
/// Communiqué au `PrefixRuleEngine` pour filtrer les règles `Project`/`Agent`.
#[derive(Debug, Clone, Default)]
pub struct ScopeContext {
    /// Portée associée à l'invocation courante (informatif — non utilisé pour le filtrage).
    pub scope: PermissionScope,
    /// Chemin canonique du projet courant (`None` lorsque hors projet).
    pub project_path: Option<PathBuf>,
    /// Identifiant de l'agent courant (`None` lorsque hors contexte agent).
    pub agent_id: Option<String>,
}

/// Règle de préfixe persistée dans SQLite.
///
/// Une règle associe un nom d'outil et un préfixe d'argument optionnel à une action.
/// `arg_prefix = None` signifie que la règle s'applique à tout argument.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefixRule {
    /// Identifiant unique (AUTOINCREMENT SQLite). 0 pour une règle non persistée.
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
    /// Portée de la règle.
    pub scope: PermissionScope,
    /// Chemin canonique du projet (renseigné lorsque `scope == Project`).
    pub project_path: Option<PathBuf>,
    /// Identifiant de l'agent (renseigné lorsque `scope == Agent`).
    pub agent_id: Option<String>,
    /// Timestamp Unix d'expiration (None = règle permanente).
    pub expires_at: Option<i64>,
}

impl Default for PrefixRule {
    fn default() -> Self {
        Self {
            id: 0,
            tool_name: String::new(),
            arg_prefix: None,
            action: RuleAction::Allow,
            created_at: 0,
            created_by_agent: None,
            scope: PermissionScope::Global,
            project_path: None,
            agent_id: None,
            expires_at: None,
        }
    }
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
    /// Variante rétrocompatible : ne filtre pas par scope (toutes les règles project + global
    /// sont considérées) mais ignore les règles expirées.
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
        Ok(self
            .check_with_id(tool_name, first_arg)?
            .map(|(_, action)| action))
    }

    /// Variante de [`check`](Self::check) qui retourne aussi l'identifiant de la règle.
    ///
    /// # Errors
    ///
    /// Retourne [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn check_with_id(
        &self,
        tool_name: &str,
        first_arg: Option<&str>,
    ) -> Result<Option<(i64, RuleAction)>, PermissionError> {
        let now = current_unix_secs();
        let mut stmt = self.db.prepare_cached(
            "SELECT id, arg_prefix, action, expires_at FROM permission_rules \
             WHERE tool_name = ? AND scope NOT IN ('session', 'agent') \
             ORDER BY CASE WHEN arg_prefix IS NULL THEN 0 ELSE LENGTH(arg_prefix) END DESC",
        )?;

        let mut rows = stmt.query(params![tool_name])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            let arg_prefix: Option<String> = row.get(1)?;
            let action_str: String = row.get(2)?;
            let expires_at: Option<i64> = row.get(3)?;

            if is_expired(expires_at, now) {
                tracing::warn!(
                    rule_id = id,
                    tool = %tool_name,
                    "expired prefix rule encountered — ignored"
                );
                continue;
            }

            let action = RuleAction::from_str(&action_str)?;
            if prefix_matches(arg_prefix.as_deref(), first_arg) {
                return Ok(Some((id, action)));
            }
        }

        Ok(None)
    }

    /// Évalue les règles avec filtrage scope-aware.
    ///
    /// Ordre d'évaluation (du plus spécifique au plus large) :
    ///
    /// 1. Règles DB `scope = 'project'` filtrées par `scope_ctx.project_path`.
    /// 2. Règles DB `scope = 'agent'` filtrées par `scope_ctx.agent_id`.
    /// 3. `session_rules` (en mémoire, jamais persistées).
    /// 4. Règles DB `scope = 'global'`.
    ///
    /// À l'intérieur de chaque tier, la règle dont le préfixe est le plus long gagne.
    /// Les règles expirées (`expires_at` dans le passé) sont ignorées et tracées en warning.
    ///
    /// # Errors
    ///
    /// Retourne [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn check_with_scope(
        &self,
        tool_name: &str,
        first_arg: Option<&str>,
        scope_ctx: &ScopeContext,
        session_rules: &[PrefixRule],
    ) -> Result<Option<(i64, RuleAction)>, PermissionError> {
        let now = current_unix_secs();

        if let Some(project_path) = scope_ctx.project_path.as_deref() {
            if let Some(hit) = self.match_in_db_project(tool_name, first_arg, project_path, now)? {
                return Ok(Some(hit));
            }
        }

        if let Some(agent_id) = scope_ctx.agent_id.as_deref() {
            if let Some(hit) = self.match_in_db_agent(tool_name, first_arg, agent_id, now)? {
                return Ok(Some(hit));
            }
        }

        if let Some(hit) = match_in_session(tool_name, first_arg, session_rules, now) {
            return Ok(Some(hit));
        }

        self.match_in_db_global(tool_name, first_arg, now)
    }

    /// Persiste une nouvelle règle et retourne son identifiant auto-incrémenté.
    ///
    /// # Errors
    ///
    /// - [`PermissionError::InvalidRule`] si `tool_name` est vide, si `scope == Session`
    ///   (les règles de session vivent en mémoire), ou si `scope == Project` sans
    ///   `project_path`.
    /// - [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn add_rule(&mut self, rule: &PrefixRule) -> Result<i64, PermissionError> {
        if rule.tool_name.trim().is_empty() {
            return Err(PermissionError::InvalidRule(
                "tool_name must not be empty".to_string(),
            ));
        }
        if rule.scope == PermissionScope::Session {
            return Err(PermissionError::InvalidRule(
                "session rules must not be persisted; use PermissionEngine::add_session_rule"
                    .to_string(),
            ));
        }
        if rule.scope == PermissionScope::Project && rule.project_path.is_none() {
            return Err(PermissionError::InvalidRule(
                "project scope requires a project_path".to_string(),
            ));
        }
        if rule.scope == PermissionScope::Agent
            && rule.agent_id.as_deref().map(str::trim).unwrap_or("").is_empty()
        {
            return Err(PermissionError::InvalidRule(
                "agent scope requires a non-empty agent_id".to_string(),
            ));
        }

        let project_path_str = rule
            .project_path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());

        self.db.execute(
            "INSERT INTO permission_rules \
             (tool_name, arg_prefix, action, created_at, created_by, scope, project_path, agent_id, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                rule.tool_name,
                rule.arg_prefix,
                rule.action.as_str(),
                rule.created_at,
                rule.created_by_agent,
                rule.scope.as_str(),
                project_path_str,
                rule.agent_id,
                rule.expires_at,
            ],
        )?;

        Ok(self.db.last_insert_rowid())
    }

    /// Supprime la règle identifiée par `id`.
    ///
    /// Silencieux si la règle n'existe pas (utilisez [`remove_rule_checked`](Self::remove_rule_checked)
    /// pour distinguer les deux cas).
    ///
    /// # Errors
    ///
    /// Retourne [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn remove_rule(&mut self, id: i64) -> Result<(), PermissionError> {
        self.db
            .execute("DELETE FROM permission_rules WHERE id = ?", params![id])?;
        Ok(())
    }

    /// Supprime la règle identifiée par `id` et retourne `true` si une ligne a été supprimée.
    ///
    /// # Errors
    ///
    /// Retourne [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn remove_rule_checked(&mut self, id: i64) -> Result<bool, PermissionError> {
        let affected = self
            .db
            .execute("DELETE FROM permission_rules WHERE id = ?", params![id])?;
        Ok(affected > 0)
    }

    /// Supprime toutes les règles persistées correspondant à *scope* (et
    /// éventuellement à *project_path* lorsque *scope* est `Project`).
    ///
    /// Retourne le nombre de lignes supprimées.
    ///
    /// # Errors
    ///
    /// - [`PermissionError::InvalidRule`] si *scope* est `Session` (les règles
    ///   de session ne sont pas persistées).
    /// - [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn remove_rules_by_scope(
        &mut self,
        scope: PermissionScope,
        project_path: Option<&Path>,
    ) -> Result<u32, PermissionError> {
        if scope == PermissionScope::Session {
            return Err(PermissionError::InvalidRule(
                "session rules are not persisted; clear them via PermissionEngine::clear_session_rules"
                    .to_string(),
            ));
        }

        let affected = match (scope, project_path) {
            (PermissionScope::Project, Some(p)) => {
                let path_str = p.to_string_lossy().to_string();
                self.db.execute(
                    "DELETE FROM permission_rules WHERE scope = 'project' AND project_path = ?",
                    params![path_str],
                )?
            }
            (PermissionScope::Project, None) => self
                .db
                .execute("DELETE FROM permission_rules WHERE scope = 'project'", [])?,
            (PermissionScope::Agent, _) => self
                .db
                .execute("DELETE FROM permission_rules WHERE scope = 'agent'", [])?,
            (PermissionScope::Global, _) => self
                .db
                .execute("DELETE FROM permission_rules WHERE scope = 'global'", [])?,
            (PermissionScope::Session, _) => 0,
        };
        Ok(affected as u32)
    }

    /// Supprime toutes les règles `scope = 'agent'` correspondant à `agent_id`.
    ///
    /// Retourne le nombre de lignes supprimées.
    ///
    /// # Errors
    ///
    /// Retourne [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn remove_rules_by_agent(&mut self, agent_id: &str) -> Result<u32, PermissionError> {
        let affected = self.db.execute(
            "DELETE FROM permission_rules WHERE scope = 'agent' AND agent_id = ?",
            params![agent_id],
        )?;
        Ok(affected as u32)
    }

    /// Supprime toutes les règles dont le champ `created_by` correspond à `created_by`.
    ///
    /// Utilisé pour les opérations d'audit ou de reset ciblé (ex : nettoyer toutes
    /// les règles écrites par un agent particulier avant qu'il en propose de
    /// nouvelles). Les règles `session` (RAM) ne sont pas concernées : elles ne sont
    /// pas persistées.
    ///
    /// Retourne le nombre de lignes supprimées.
    ///
    /// # Errors
    ///
    /// Retourne [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn remove_rules_by_creator(
        &mut self,
        created_by: &str,
    ) -> Result<u32, PermissionError> {
        let affected = self.db.execute(
            "DELETE FROM permission_rules WHERE created_by = ?",
            params![created_by],
        )?;
        Ok(affected as u32)
    }

    /// Liste les règles persistées dont le champ `created_by` correspond.
    ///
    /// Renvoie les règles triées par identifiant croissant. N'inclut pas les
    /// règles de session (RAM uniquement, jamais persistées).
    ///
    /// # Errors
    ///
    /// Retourne [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn list_rules_by_creator(
        &self,
        created_by: &str,
    ) -> Result<Vec<PrefixRule>, PermissionError> {
        let mut stmt = self.db.prepare(
            "SELECT id, tool_name, arg_prefix, action, created_at, created_by, \
                    scope, project_path, agent_id, expires_at \
             FROM permission_rules \
             WHERE created_by = ? \
             ORDER BY id ASC",
        )?;
        let rules = stmt
            .query_map(params![created_by], row_to_rule)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .collect::<Result<Vec<_>, PermissionError>>()?;
        Ok(rules)
    }

    /// Liste les règles `scope = 'agent'` filtrées par `agent_id`.
    ///
    /// # Errors
    ///
    /// Retourne [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn list_rules_for_agent(
        &self,
        agent_id: &str,
    ) -> Result<Vec<PrefixRule>, PermissionError> {
        let mut stmt = self.db.prepare(
            "SELECT id, tool_name, arg_prefix, action, created_at, created_by, \
                    scope, project_path, agent_id, expires_at \
             FROM permission_rules \
             WHERE scope = 'agent' AND agent_id = ? \
             ORDER BY id ASC",
        )?;
        let rules = stmt
            .query_map(params![agent_id], row_to_rule)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .collect::<Result<Vec<_>, PermissionError>>()?;
        Ok(rules)
    }

    /// Retourne toutes les règles persistées, triées par identifiant croissant.
    ///
    /// # Errors
    ///
    /// Retourne [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn list_rules(&self) -> Result<Vec<PrefixRule>, PermissionError> {
        self.list_rules_filtered(None, None)
    }

    /// Retourne les règles persistées, optionnellement filtrées par scope et chemin de projet.
    ///
    /// - `scope = Some(Project)` + `project_path = Some(p)` : règles du projet `p`.
    /// - `scope = Some(Project)` + `project_path = None` : toutes règles `project`.
    /// - `scope = Some(Global)` : règles `global`.
    /// - `scope = None` : toutes les règles persistées (project + global).
    ///
    /// Les règles `session` ne sont jamais persistées et n'apparaissent donc pas ici.
    ///
    /// # Errors
    ///
    /// Retourne [`PermissionError::Database`] en cas d'erreur SQLite.
    pub fn list_rules_filtered(
        &self,
        scope: Option<PermissionScope>,
        project_path: Option<&Path>,
    ) -> Result<Vec<PrefixRule>, PermissionError> {
        let project_path_str = project_path.map(|p| p.to_string_lossy().to_string());

        let (sql, params): (String, Vec<rusqlite::types::Value>) = match (scope, &project_path_str)
        {
            (Some(PermissionScope::Session), _) => {
                // Aucune règle persistée n'a scope=session.
                return Ok(Vec::new());
            }
            (Some(PermissionScope::Project), Some(p)) => (
                "SELECT id, tool_name, arg_prefix, action, created_at, created_by, \
                        scope, project_path, agent_id, expires_at \
                 FROM permission_rules \
                 WHERE scope = 'project' AND project_path = ? \
                 ORDER BY id ASC"
                    .to_string(),
                vec![rusqlite::types::Value::Text(p.clone())],
            ),
            (Some(PermissionScope::Project), None) => (
                "SELECT id, tool_name, arg_prefix, action, created_at, created_by, \
                        scope, project_path, agent_id, expires_at \
                 FROM permission_rules \
                 WHERE scope = 'project' \
                 ORDER BY id ASC"
                    .to_string(),
                Vec::new(),
            ),
            (Some(PermissionScope::Agent), _) => (
                "SELECT id, tool_name, arg_prefix, action, created_at, created_by, \
                        scope, project_path, agent_id, expires_at \
                 FROM permission_rules \
                 WHERE scope = 'agent' \
                 ORDER BY id ASC"
                    .to_string(),
                Vec::new(),
            ),
            (Some(PermissionScope::Global), _) => (
                "SELECT id, tool_name, arg_prefix, action, created_at, created_by, \
                        scope, project_path, agent_id, expires_at \
                 FROM permission_rules \
                 WHERE scope = 'global' \
                 ORDER BY id ASC"
                    .to_string(),
                Vec::new(),
            ),
            (None, _) => (
                "SELECT id, tool_name, arg_prefix, action, created_at, created_by, \
                        scope, project_path, agent_id, expires_at \
                 FROM permission_rules \
                 WHERE scope != 'session' \
                 ORDER BY id ASC"
                    .to_string(),
                Vec::new(),
            ),
        };

        let mut stmt = self.db.prepare(&sql)?;
        let rules = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), row_to_rule)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .collect::<Result<Vec<_>, PermissionError>>()?;
        Ok(rules)
    }

    // ─────────────────────────────────────────────
    // Privé
    // ─────────────────────────────────────────────

    fn match_in_db_project(
        &self,
        tool_name: &str,
        first_arg: Option<&str>,
        project_path: &Path,
        now: i64,
    ) -> Result<Option<(i64, RuleAction)>, PermissionError> {
        let path_str = project_path.to_string_lossy().to_string();
        let mut stmt = self.db.prepare_cached(
            "SELECT id, arg_prefix, action, expires_at FROM permission_rules \
             WHERE tool_name = ? AND scope = 'project' AND project_path = ? \
             ORDER BY CASE WHEN arg_prefix IS NULL THEN 0 ELSE LENGTH(arg_prefix) END DESC",
        )?;
        let mut rows = stmt.query(params![tool_name, path_str])?;
        scan_rows(&mut rows, tool_name, "project", first_arg, now)
    }

    fn match_in_db_agent(
        &self,
        tool_name: &str,
        first_arg: Option<&str>,
        agent_id: &str,
        now: i64,
    ) -> Result<Option<(i64, RuleAction)>, PermissionError> {
        let mut stmt = self.db.prepare_cached(
            "SELECT id, arg_prefix, action, expires_at FROM permission_rules \
             WHERE tool_name = ? AND scope = 'agent' AND agent_id = ? \
             ORDER BY CASE WHEN arg_prefix IS NULL THEN 0 ELSE LENGTH(arg_prefix) END DESC",
        )?;
        let mut rows = stmt.query(params![tool_name, agent_id])?;
        scan_rows(&mut rows, tool_name, "agent", first_arg, now)
    }

    fn match_in_db_global(
        &self,
        tool_name: &str,
        first_arg: Option<&str>,
        now: i64,
    ) -> Result<Option<(i64, RuleAction)>, PermissionError> {
        let mut stmt = self.db.prepare_cached(
            "SELECT id, arg_prefix, action, expires_at FROM permission_rules \
             WHERE tool_name = ? AND scope = 'global' \
             ORDER BY CASE WHEN arg_prefix IS NULL THEN 0 ELSE LENGTH(arg_prefix) END DESC",
        )?;
        let mut rows = stmt.query(params![tool_name])?;
        scan_rows(&mut rows, tool_name, "global", first_arg, now)
    }

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
            CREATE INDEX IF NOT EXISTS idx_rules_tool ON permission_rules(tool_name);
            CREATE INDEX IF NOT EXISTS idx_rules_scope_project
                ON permission_rules(scope, project_path);",
        )?;

        add_column_if_missing(
            &self.db,
            "permission_rules",
            "scope",
            "TEXT NOT NULL DEFAULT 'global'",
        )?;
        add_column_if_missing(&self.db, "permission_rules", "project_path", "TEXT")?;
        add_column_if_missing(&self.db, "permission_rules", "agent_id", "TEXT")?;
        add_column_if_missing(&self.db, "permission_rules", "expires_at", "INTEGER")?;

        Ok(())
    }
}

// ─────────────────────────────────────────────
// Helpers libres
// ─────────────────────────────────────────────

fn current_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn is_expired(expires_at: Option<i64>, now: i64) -> bool {
    matches!(expires_at, Some(deadline) if deadline <= now)
}

fn prefix_matches(arg_prefix: Option<&str>, first_arg: Option<&str>) -> bool {
    match (arg_prefix, first_arg) {
        (None, _) => true,
        (Some(prefix), Some(arg)) => arg.starts_with(prefix),
        (Some(_), None) => false,
    }
}

fn match_in_session(
    tool_name: &str,
    first_arg: Option<&str>,
    session_rules: &[PrefixRule],
    now: i64,
) -> Option<(i64, RuleAction)> {
    let mut candidates: Vec<&PrefixRule> = session_rules
        .iter()
        .filter(|r| r.tool_name == tool_name)
        .filter(|r| {
            if is_expired(r.expires_at, now) {
                tracing::warn!(
                    rule_id = r.id,
                    tool = %tool_name,
                    "expired session rule encountered — ignored"
                );
                false
            } else {
                true
            }
        })
        .collect();

    candidates.sort_by(|a, b| {
        let la = a.arg_prefix.as_deref().map(str::len).unwrap_or(0);
        let lb = b.arg_prefix.as_deref().map(str::len).unwrap_or(0);
        lb.cmp(&la)
    });

    for rule in candidates {
        if prefix_matches(rule.arg_prefix.as_deref(), first_arg) {
            return Some((rule.id, rule.action.clone()));
        }
    }
    None
}

fn row_to_rule(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<PrefixRule, PermissionError>> {
    let id: i64 = row.get(0)?;
    let tool_name: String = row.get(1)?;
    let arg_prefix: Option<String> = row.get(2)?;
    let action_str: String = row.get(3)?;
    let created_at: i64 = row.get(4)?;
    let created_by_agent: Option<String> = row.get(5)?;
    let scope_str: String = row.get(6)?;
    let project_path_str: Option<String> = row.get(7)?;
    let agent_id: Option<String> = row.get(8)?;
    let expires_at: Option<i64> = row.get(9)?;

    Ok((|| -> Result<PrefixRule, PermissionError> {
        Ok(PrefixRule {
            id,
            tool_name,
            arg_prefix,
            action: RuleAction::from_str(&action_str)?,
            created_at,
            created_by_agent,
            scope: PermissionScope::from_db_str(&scope_str)?,
            project_path: project_path_str.map(PathBuf::from),
            agent_id,
            expires_at,
        })
    })())
}

fn scan_rows(
    rows: &mut rusqlite::Rows<'_>,
    tool_name: &str,
    scope: &str,
    first_arg: Option<&str>,
    now: i64,
) -> Result<Option<(i64, RuleAction)>, PermissionError> {
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let arg_prefix: Option<String> = row.get(1)?;
        let action_str: String = row.get(2)?;
        let expires_at: Option<i64> = row.get(3)?;

        if is_expired(expires_at, now) {
            tracing::warn!(
                rule_id = id,
                tool = %tool_name,
                scope = %scope,
                "expired prefix rule encountered — ignored"
            );
            continue;
        }

        let action = RuleAction::from_str(&action_str)?;
        if prefix_matches(arg_prefix.as_deref(), first_arg) {
            tracing::debug!(
                tool = %tool_name,
                rule_id = id,
                scope = %scope,
                action = ?action,
                "prefix rule matched"
            );
            return Ok(Some((id, action)));
        }
    }
    Ok(None)
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
        current_unix_secs()
    }

    fn rule(tool: &str, prefix: Option<&str>, action: RuleAction) -> PrefixRule {
        PrefixRule {
            tool_name: tool.into(),
            arg_prefix: prefix.map(str::to_string),
            action,
            created_at: now(),
            ..PrefixRule::default()
        }
    }

    #[test]
    fn prefix_rule_matches_git_wildcard() {
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule("bash_executor", Some("git"), RuleAction::Allow))
            .expect("add_rule");
        let result = engine
            .check("bash_executor", Some("git push origin main"))
            .expect("check");
        assert_eq!(result, Some(RuleAction::Allow));
    }

    #[test]
    fn prefix_rule_no_match_returns_none() {
        let (engine, _tmp) = tmp_engine();
        let result = engine
            .check("bash_executor", Some("git status"))
            .expect("check");
        assert!(result.is_none());
    }

    #[test]
    fn prefix_rule_deny_action_returned() {
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule("file_write", Some("/etc"), RuleAction::Deny))
            .expect("add_rule");
        let result = engine
            .check("file_write", Some("/etc/passwd"))
            .expect("check");
        assert_eq!(result, Some(RuleAction::Deny));
    }

    #[test]
    fn prefix_rule_none_prefix_matches_any_arg() {
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule("file_read", None, RuleAction::Allow))
            .expect("add_rule");
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
        let id = engine
            .add_rule(&rule("bash_executor", Some("git"), RuleAction::Allow))
            .expect("add_rule");
        engine.remove_rule(id).expect("remove_rule");
        let result = engine
            .check("bash_executor", Some("git status"))
            .expect("check");
        assert!(result.is_none());
    }

    #[test]
    fn remove_rule_checked_reports_existence() {
        let (mut engine, _tmp) = tmp_engine();
        let id = engine
            .add_rule(&rule("bash_executor", None, RuleAction::Allow))
            .expect("add_rule");
        assert!(engine.remove_rule_checked(id).expect("remove_rule_checked"));
        assert!(!engine.remove_rule_checked(id).expect("remove_rule_checked"));
    }

    #[test]
    fn list_rules_returns_all() {
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule("tool_a", Some("prefix_x"), RuleAction::Allow))
            .expect("add_rule");
        engine
            .add_rule(&rule("tool_b", Some("prefix_y"), RuleAction::Allow))
            .expect("add_rule");
        let rules = engine.list_rules().expect("list_rules");
        assert_eq!(rules.len(), 2);
    }

    fn rule_with_creator(
        tool: &str,
        action: RuleAction,
        creator: &str,
    ) -> PrefixRule {
        PrefixRule {
            tool_name: tool.into(),
            action,
            created_at: now(),
            created_by_agent: Some(creator.into()),
            ..PrefixRule::default()
        }
    }

    #[test]
    fn list_rules_by_creator_returns_only_matching() {
        // GIVEN trois règles : deux par "onboarding-agent", une par "user-hitl"
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule_with_creator(
                "tool_a",
                RuleAction::Allow,
                "onboarding-agent",
            ))
            .expect("add_rule");
        engine
            .add_rule(&rule_with_creator(
                "tool_b",
                RuleAction::Deny,
                "onboarding-agent",
            ))
            .expect("add_rule");
        engine
            .add_rule(&rule_with_creator("tool_c", RuleAction::Allow, "user-hitl"))
            .expect("add_rule");

        // WHEN on liste par créateur
        let rules = engine
            .list_rules_by_creator("onboarding-agent")
            .expect("list_rules_by_creator");

        // THEN seules les deux règles de l'onboarding-agent reviennent
        assert_eq!(rules.len(), 2);
        assert!(rules
            .iter()
            .all(|r| r.created_by_agent.as_deref() == Some("onboarding-agent")));
    }

    #[test]
    fn list_rules_by_creator_unknown_returns_empty() {
        // GIVEN engine vide
        let (engine, _tmp) = tmp_engine();
        // WHEN on liste un créateur inconnu
        let rules = engine
            .list_rules_by_creator("ghost")
            .expect("list_rules_by_creator");
        // THEN aucune règle
        assert!(rules.is_empty());
    }

    #[test]
    fn remove_rules_by_creator_deletes_only_matching() {
        // GIVEN deux règles "onboarding-agent" + une "user-hitl"
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule_with_creator(
                "tool_a",
                RuleAction::Allow,
                "onboarding-agent",
            ))
            .expect("add_rule");
        engine
            .add_rule(&rule_with_creator(
                "tool_b",
                RuleAction::Deny,
                "onboarding-agent",
            ))
            .expect("add_rule");
        engine
            .add_rule(&rule_with_creator("tool_c", RuleAction::Allow, "user-hitl"))
            .expect("add_rule");

        // WHEN on supprime par créateur
        let removed = engine
            .remove_rules_by_creator("onboarding-agent")
            .expect("remove_rules_by_creator");

        // THEN deux règles supprimées, la règle user-hitl reste
        assert_eq!(removed, 2);
        let remaining = engine.list_rules().expect("list_rules");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].created_by_agent.as_deref(), Some("user-hitl"));
    }

    #[test]
    fn remove_rules_by_creator_unknown_returns_zero() {
        // GIVEN une règle existante
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&rule_with_creator("tool_a", RuleAction::Allow, "user-hitl"))
            .expect("add_rule");
        // WHEN on supprime un créateur inexistant
        let removed = engine
            .remove_rules_by_creator("ghost")
            .expect("remove_rules_by_creator");
        // THEN zéro suppression, la règle reste
        assert_eq!(removed, 0);
        assert_eq!(engine.list_rules().expect("list").len(), 1);
    }

    #[test]
    fn add_rule_rejects_empty_tool_name() {
        let (mut engine, _tmp) = tmp_engine();
        let r = PrefixRule {
            tool_name: String::new(),
            ..PrefixRule::default()
        };
        assert!(matches!(
            engine.add_rule(&r),
            Err(PermissionError::InvalidRule(_))
        ));
    }

    #[test]
    fn add_rule_rejects_session_scope() {
        let (mut engine, _tmp) = tmp_engine();
        let r = PrefixRule {
            tool_name: "bash_executor".into(),
            scope: PermissionScope::Session,
            ..PrefixRule::default()
        };
        assert!(matches!(
            engine.add_rule(&r),
            Err(PermissionError::InvalidRule(_))
        ));
    }

    #[test]
    fn add_rule_rejects_project_scope_without_path() {
        let (mut engine, _tmp) = tmp_engine();
        let r = PrefixRule {
            tool_name: "bash_executor".into(),
            scope: PermissionScope::Project,
            project_path: None,
            ..PrefixRule::default()
        };
        assert!(matches!(
            engine.add_rule(&r),
            Err(PermissionError::InvalidRule(_))
        ));
    }

    #[test]
    fn test_session_rule_found_in_memory() {
        let (engine, _tmp) = tmp_engine();
        let session = vec![PrefixRule {
            id: 7,
            tool_name: "bash_executor".into(),
            arg_prefix: Some("git".into()),
            action: RuleAction::Allow,
            scope: PermissionScope::Session,
            ..PrefixRule::default()
        }];
        let ctx = ScopeContext::default();
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx, &session)
            .expect("check_with_scope");
        assert_eq!(hit, Some((7, RuleAction::Allow)));
    }

    #[test]
    fn test_agent_rule_filtered_by_id() {
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Agent,
                agent_id: Some("apollia:chat".into()),
                ..PrefixRule::default()
            })
            .expect("add_rule");
        let ctx_other = ScopeContext {
            agent_id: Some("apollia:other".into()),
            ..ScopeContext::default()
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx_other, &[])
            .expect("check_with_scope");
        assert!(hit.is_none(), "rule for apollia:chat must not match apollia:other");

        let ctx_chat = ScopeContext {
            agent_id: Some("apollia:chat".into()),
            ..ScopeContext::default()
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx_chat, &[])
            .expect("check_with_scope");
        assert!(matches!(hit, Some((_, RuleAction::Allow))));
    }

    #[test]
    fn test_add_rule_rejects_agent_scope_without_id() {
        let (mut engine, _tmp) = tmp_engine();
        let r = PrefixRule {
            tool_name: "bash_executor".into(),
            scope: PermissionScope::Agent,
            agent_id: None,
            ..PrefixRule::default()
        };
        assert!(matches!(
            engine.add_rule(&r),
            Err(PermissionError::InvalidRule(_))
        ));
    }

    #[test]
    fn test_project_rule_filtered_by_path() {
        let (mut engine, _tmp) = tmp_engine();
        let project_a = PathBuf::from("/home/user/projet-a");
        engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Project,
                project_path: Some(project_a.clone()),
                ..PrefixRule::default()
            })
            .expect("add_rule");
        let ctx_b = ScopeContext {
            scope: PermissionScope::Project,
            project_path: Some(PathBuf::from("/home/user/projet-b")),
            agent_id: None,
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx_b, &[])
            .expect("check_with_scope");
        assert!(hit.is_none(), "rule from projet-a must not match projet-b");

        let ctx_a = ScopeContext {
            scope: PermissionScope::Project,
            project_path: Some(project_a),
            agent_id: None,
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx_a, &[])
            .expect("check_with_scope");
        assert!(matches!(hit, Some((_, RuleAction::Allow))));
    }

    #[test]
    fn test_global_rule_applies_everywhere() {
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Global,
                ..PrefixRule::default()
            })
            .expect("add_rule");
        let ctx = ScopeContext {
            scope: PermissionScope::Project,
            project_path: Some(PathBuf::from("/home/user/anywhere")),
            agent_id: None,
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx, &[])
            .expect("check_with_scope");
        assert!(matches!(hit, Some((_, RuleAction::Allow))));
    }

    #[test]
    fn test_expired_rule_ignored() {
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Allow,
                created_at: now() - 1000,
                scope: PermissionScope::Global,
                expires_at: Some(now() - 1),
                ..PrefixRule::default()
            })
            .expect("add_rule");
        let ctx = ScopeContext {
            scope: PermissionScope::Global,
            project_path: None,
            agent_id: None,
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx, &[])
            .expect("check_with_scope");
        assert!(hit.is_none());
        let hit_legacy = engine
            .check("bash_executor", Some("git status"))
            .expect("check");
        assert!(hit_legacy.is_none());
    }

    #[test]
    fn test_scope_priority_project_over_agent_over_session_over_global() {
        // Ordre attendu : Project > Agent > Session > Global.
        let (mut engine, _tmp) = tmp_engine();
        let project = PathBuf::from("/home/user/projet");
        let global_id = engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Deny,
                created_at: now(),
                scope: PermissionScope::Global,
                ..PrefixRule::default()
            })
            .expect("add_rule global");
        let agent_id_rule = engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Deny,
                created_at: now(),
                scope: PermissionScope::Agent,
                agent_id: Some("apollia:chat".into()),
                ..PrefixRule::default()
            })
            .expect("add_rule agent");
        let project_id_rule = engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Project,
                project_path: Some(project.clone()),
                ..PrefixRule::default()
            })
            .expect("add_rule project");

        let session = vec![PrefixRule {
            id: 99,
            tool_name: "bash_executor".into(),
            arg_prefix: Some("git".into()),
            action: RuleAction::Deny,
            scope: PermissionScope::Session,
            ..PrefixRule::default()
        }];

        // Project gagne avec project_path renseigné.
        let ctx = ScopeContext {
            scope: PermissionScope::Project,
            project_path: Some(project),
            agent_id: Some("apollia:chat".into()),
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx, &session)
            .expect("check_with_scope");
        assert_eq!(hit, Some((project_id_rule, RuleAction::Allow)));

        // Sans projet, Agent gagne sur Session et Global.
        let ctx_no_project = ScopeContext {
            scope: PermissionScope::Agent,
            project_path: None,
            agent_id: Some("apollia:chat".into()),
        };
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx_no_project, &session)
            .expect("check_with_scope");
        assert_eq!(hit, Some((agent_id_rule, RuleAction::Deny)));

        // Sans projet ni agent, Session gagne sur Global.
        let ctx_bare = ScopeContext::default();
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx_bare, &session)
            .expect("check_with_scope");
        assert_eq!(hit, Some((99, RuleAction::Deny)));

        // Sans rien d'autre, Global gagne en dernier ressort.
        let hit = engine
            .check_with_scope("bash_executor", Some("git status"), &ctx_bare, &[])
            .expect("check_with_scope");
        assert_eq!(hit, Some((global_id, RuleAction::Deny)));
    }

    #[test]
    fn test_backward_compat_check_without_scope() {
        let (mut engine, _tmp) = tmp_engine();
        engine
            .add_rule(&PrefixRule {
                tool_name: "bash_executor".into(),
                arg_prefix: Some("git".into()),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Global,
                ..PrefixRule::default()
            })
            .expect("add_rule");
        let result = engine
            .check("bash_executor", Some("git push"))
            .expect("check");
        assert_eq!(result, Some(RuleAction::Allow));
    }

    #[test]
    fn list_rules_filtered_by_scope_and_path() {
        let (mut engine, _tmp) = tmp_engine();
        let project = PathBuf::from("/home/user/projet-a");
        engine
            .add_rule(&PrefixRule {
                tool_name: "tool_g".into(),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Global,
                ..PrefixRule::default()
            })
            .expect("add_rule");
        engine
            .add_rule(&PrefixRule {
                tool_name: "tool_p".into(),
                action: RuleAction::Allow,
                created_at: now(),
                scope: PermissionScope::Project,
                project_path: Some(project.clone()),
                ..PrefixRule::default()
            })
            .expect("add_rule");

        let globals = engine
            .list_rules_filtered(Some(PermissionScope::Global), None)
            .expect("list globals");
        assert_eq!(globals.len(), 1);
        assert_eq!(globals[0].tool_name, "tool_g");

        let projects = engine
            .list_rules_filtered(Some(PermissionScope::Project), Some(&project))
            .expect("list project");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].tool_name, "tool_p");

        let none_for_session = engine
            .list_rules_filtered(Some(PermissionScope::Session), None)
            .expect("list session");
        assert!(none_for_session.is_empty());
    }
}
