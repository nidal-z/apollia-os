//! The definitions and overrides of [`AgentRepository`].
//!
//! A definition is replaced whole by an update and never touched by the
//! user; an override is the user's value for one field and survives every
//! update; "back to defaults" deletes the overrides of an agent.

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::agent_definition::{
    resolve, AgentOrigin, AgentOverride, OverrideField, ResolvedAgent, StoredAgent,
};
use crate::agent_repository::{AgentRepository, AgentRepositoryError};
use apollia_core::model_preset::AgentModel;
use apollia_core::AutonomyLevel;

const STORED_COLUMNS: &str =
    "name, origin, package, version, definition_json, enabled, installed_at, updated_at";

impl AgentRepository {
    /// The stored agent `name`: its definition and where it comes from.
    ///
    /// # Errors
    ///
    /// [`AgentRepositoryError::Sqlite`] or [`AgentRepositoryError::SerdeError`].
    pub fn stored(&self, name: &str) -> Result<Option<StoredAgent>, AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        stored_in(&conn, name)
    }

    /// Every stored agent, by name.
    ///
    /// # Errors
    ///
    /// [`AgentRepositoryError::Sqlite`] or [`AgentRepositoryError::SerdeError`].
    pub fn list_stored(&self) -> Result<Vec<StoredAgent>, AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let mut stmt = conn.prepare(&format!(
            "SELECT {STORED_COLUMNS} FROM agents ORDER BY name"
        ))?;
        let rows = stmt.query_map([], read_stored_row)?;
        rows.map(|row| row?.into_stored()).collect()
    }

    // TEST-ONLY: no production path stores a declarative agent yet; an agent
    // that runs code is stored by `AgentRepository::save`, which writes its
    // definition the same way and keeps its overrides.
    /// Stores a declarative agent, or replaces the definition of an existing
    /// one.
    ///
    /// The overrides are left in place. The code columns of an existing row
    /// are left as they are.
    ///
    /// # Errors
    ///
    /// [`AgentRepositoryError::Sqlite`].
    pub fn save_definition(&self, agent: &StoredAgent) -> Result<(), AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        conn.execute(
            "INSERT INTO agents \
                 (name, origin, package, version, definition_json, enabled, \
                  installed_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(name) DO UPDATE SET \
                 origin = excluded.origin, \
                 package = excluded.package, \
                 version = excluded.version, \
                 definition_json = excluded.definition_json, \
                 updated_at = excluded.updated_at",
            params![
                &agent.name,
                agent.origin.as_str(),
                &agent.package,
                &agent.version,
                agent.definition.to_string(),
                agent.enabled,
                &agent.installed_at,
                &agent.updated_at,
            ],
        )?;
        Ok(())
    }

    // TEST-ONLY: an update of an agent that runs code goes through
    // `AgentRepository::save`, which replaces the definition and keeps the
    // overrides the same way; no production path updates a declarative agent
    // yet.
    /// Replaces the definition of `name` with the one of `version`, as an
    /// update does. The overrides are kept.
    ///
    /// # Errors
    ///
    /// [`AgentRepositoryError::NotFound`] when `name` is not stored, or
    /// [`AgentRepositoryError::Sqlite`].
    pub fn replace_definition(
        &self,
        name: &str,
        version: &str,
        definition: &Value,
    ) -> Result<(), AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let changed = conn.execute(
            "UPDATE agents SET version = ?2, definition_json = ?3, \
                 updated_at = datetime('now') WHERE name = ?1",
            params![name, version, definition.to_string()],
        )?;
        if changed == 0 {
            return Err(AgentRepositoryError::NotFound(name.to_owned()));
        }
        Ok(())
    }

    /// The overrides of `name`, by field.
    ///
    /// # Errors
    ///
    /// [`AgentRepositoryError::Sqlite`] or [`AgentRepositoryError::SerdeError`].
    pub fn overrides(&self, name: &str) -> Result<Vec<AgentOverride>, AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        overrides_in(&conn, name)
    }

    /// Sets the user's value of `field` for `name`, and returns the value it
    /// replaces, if any.
    ///
    /// # Errors
    ///
    /// [`AgentRepositoryError::NotFound`] when `name` is not stored,
    /// [`AgentRepositoryError::InvalidOverride`] when `value` does not have
    /// the field's shape, or [`AgentRepositoryError::Sqlite`].
    pub fn set_override(
        &self,
        name: &str,
        field: OverrideField,
        value: &Value,
    ) -> Result<Option<Value>, AgentRepositoryError> {
        field.check(value)?;
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        require_agent(&conn, name)?;
        let tx = conn.unchecked_transaction()?;
        let previous = override_value(&tx, name, field)?;
        tx.execute(
            "INSERT INTO agent_overrides (agent, field, value_json, updated_at) \
             VALUES (?1, ?2, ?3, datetime('now')) \
             ON CONFLICT(agent, field) DO UPDATE SET \
                 value_json = excluded.value_json, updated_at = excluded.updated_at",
            params![name, field.as_str(), value.to_string()],
        )?;
        tx.commit()?;
        Ok(previous)
    }

    /// Removes the user's value of `field` for `name`, and returns it; `None`
    /// when the field was not overridden.
    ///
    /// # Errors
    ///
    /// [`AgentRepositoryError::NotFound`] when `name` is not stored, or
    /// [`AgentRepositoryError::Sqlite`].
    pub fn remove_override(
        &self,
        name: &str,
        field: OverrideField,
    ) -> Result<Option<Value>, AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        require_agent(&conn, name)?;
        let tx = conn.unchecked_transaction()?;
        let previous = override_value(&tx, name, field)?;
        tx.execute(
            "DELETE FROM agent_overrides WHERE agent = ?1 AND field = ?2",
            params![name, field.as_str()],
        )?;
        tx.commit()?;
        Ok(previous)
    }

    /// Back to defaults: removes every override of `name`, and returns them.
    ///
    /// # Errors
    ///
    /// [`AgentRepositoryError::NotFound`] when `name` is not stored, or
    /// [`AgentRepositoryError::Sqlite`].
    pub fn reset_overrides(&self, name: &str) -> Result<Vec<AgentOverride>, AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        require_agent(&conn, name)?;
        let tx = conn.unchecked_transaction()?;
        let removed = overrides_in(&tx, name)?;
        tx.execute(
            "DELETE FROM agent_overrides WHERE agent = ?1",
            params![name],
        )?;
        tx.commit()?;
        Ok(removed)
    }

    /// The agent `name` in force: its definition, then its overrides, with
    /// `model` as read from `agent_model` and `default_autonomy` as the tier
    /// when neither names one.
    ///
    /// # Errors
    ///
    /// [`AgentRepositoryError::Sqlite`] or [`AgentRepositoryError::SerdeError`].
    pub fn resolved(
        &self,
        name: &str,
        model: Option<AgentModel>,
        default_autonomy: AutonomyLevel,
    ) -> Result<Option<ResolvedAgent>, AgentRepositoryError> {
        let conn = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        let Some(stored) = stored_in(&conn, name)? else {
            return Ok(None);
        };
        let overrides = overrides_in(&conn, name)?;
        Ok(Some(resolve(&stored, &overrides, model, default_autonomy)))
    }
}

/// A row of `agents` before its JSON is parsed.
struct StoredRow {
    name: String,
    origin: String,
    package: Option<String>,
    version: String,
    definition_json: String,
    enabled: bool,
    installed_at: String,
    updated_at: String,
}

impl StoredRow {
    fn into_stored(self) -> Result<StoredAgent, AgentRepositoryError> {
        Ok(StoredAgent {
            definition: serde_json::from_str(&self.definition_json)?,
            name: self.name,
            origin: AgentOrigin::parse(&self.origin),
            package: self.package,
            version: self.version,
            enabled: self.enabled,
            installed_at: self.installed_at,
            updated_at: self.updated_at,
        })
    }
}

fn read_stored_row(row: &rusqlite::Row<'_>) -> Result<StoredRow, rusqlite::Error> {
    Ok(StoredRow {
        name: row.get(0)?,
        origin: row.get(1)?,
        package: row.get(2)?,
        version: row.get(3)?,
        definition_json: row.get(4)?,
        enabled: row.get(5)?,
        installed_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn stored_in(conn: &Connection, name: &str) -> Result<Option<StoredAgent>, AgentRepositoryError> {
    let row = conn
        .query_row(
            &format!("SELECT {STORED_COLUMNS} FROM agents WHERE name = ?1"),
            params![name],
            read_stored_row,
        )
        .optional()?;
    row.map(StoredRow::into_stored).transpose()
}

fn require_agent(conn: &Connection, name: &str) -> Result<(), AgentRepositoryError> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM agents WHERE name = ?1)",
        params![name],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(AgentRepositoryError::NotFound(name.to_owned()))
    }
}

fn override_value(
    conn: &Connection,
    name: &str,
    field: OverrideField,
) -> Result<Option<Value>, AgentRepositoryError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value_json FROM agent_overrides WHERE agent = ?1 AND field = ?2",
            params![name, field.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    raw.map(|json| serde_json::from_str(&json))
        .transpose()
        .map_err(AgentRepositoryError::from)
}

fn overrides_in(conn: &Connection, name: &str) -> Result<Vec<AgentOverride>, AgentRepositoryError> {
    let mut stmt = conn
        .prepare("SELECT field, value_json FROM agent_overrides WHERE agent = ?1 ORDER BY field")?;
    let rows = stmt.query_map(params![name], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut overrides = Vec::new();
    for row in rows {
        let (field, value_json) = row?;
        // A field this binary does not know was written by a newer one; it is
        // left in the table and out of the resolution.
        let Ok(field) = OverrideField::parse(&field) else {
            continue;
        };
        overrides.push(AgentOverride {
            field,
            value: serde_json::from_str(&value_json)?,
        });
    }
    Ok(overrides)
}

#[cfg(test)]
mod tests;
