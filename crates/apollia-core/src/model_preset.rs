//! Model presets, the model of each agent, and the default model of the machine.
//!
//! All three live in `system.db`, apart from the installed agent packages, so
//! updating an agent never erases what the user chose for it.
//!
//! - A **preset** is a named set of load settings and request settings
//!   attached to one model. A model has at most one default preset; an agent
//!   may choose another one of the same model. A preset never carries a system
//!   prompt: that is the agent's mission, not a setting.
//! - The **model of an agent** is the user's assignment: a model of the local
//!   registry or a model of a declared server, and optionally a preset of it.
//! - The **default model** stays the single `is_default` row of
//!   `llm_backends`. It designates a local engine row, or a model of a
//!   declared server through a row that names the server (see
//!   [`SERVER_REF_FIELD`]).
//!
//! A model is designated by a [`ModelSource`] and an identifier: for a local
//! model, the identifier the local registry gives it; for a server model, its
//! routing name `<server>/<model>` (see [`crate::inference_server::route_name`]).

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::inference_server::split_route_name;
use crate::llm_backend::{LlmBackendConfig, LlmBackendError, LlmBackendRepository, LlmProvider};
use crate::system_db::SYSTEM_PROMPT_KEYS;

/// `config_json` field of a backend row that designates a model of a declared
/// server by the server's name, instead of carrying an address of its own.
pub const SERVER_REF_FIELD: &str = "server";

/// Where a model runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelSource {
    /// On this machine, served by the local engine.
    Local,
    /// On a declared inference server.
    Server,
}

impl ModelSource {
    fn as_str(self) -> &'static str {
        match self {
            ModelSource::Local => "local",
            ModelSource::Server => "server",
        }
    }

    fn parse(raw: &str) -> Result<Self, LlmBackendError> {
        match raw {
            "local" => Ok(ModelSource::Local),
            "server" => Ok(ModelSource::Server),
            other => Err(LlmBackendError::Serialization(format!(
                "unknown model source: {other}"
            ))),
        }
    }
}

/// Request settings of a preset, sent with each completion. Every field is
/// optional: an absent one leaves the model's own default in force.
///
/// Unknown fields are refused, so a system prompt cannot slip in.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestSettings {
    /// Sampling temperature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Upper bound on the length of an answer, in tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Strings that end the answer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
    /// Top-k sampling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Top-p (nucleus) sampling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Min-p sampling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_p: Option<f32>,
    /// Repetition penalty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f32>,
    /// Presence penalty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// JSON Schema the answer must satisfy (structured output).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<serde_json::Value>,
}

/// A preset as `system.db` stores it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelPreset {
    /// Identifier, assigned at creation.
    pub id: i64,
    /// Name shown to the user, unique among the presets of one model.
    pub name: String,
    /// Where the model runs.
    pub model_source: ModelSource,
    /// The model the preset is attached to.
    pub model_id: String,
    /// Load settings, applied over the automatic computation. Kept as an
    /// object: the engine that loads the model reads the keys it knows.
    pub load: serde_json::Map<String, serde_json::Value>,
    /// Request settings.
    pub request: RequestSettings,
    /// Whether this is the default preset of its model.
    pub is_model_default: bool,
}

/// What the user writes to create or replace a preset. The model is fixed at
/// creation.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetDraft {
    /// Name, unique among the presets of the model.
    pub name: String,
    /// Load settings.
    pub load: serde_json::Map<String, serde_json::Value>,
    /// Request settings.
    pub request: RequestSettings,
    /// Whether the preset becomes the default of its model. Setting it clears
    /// the flag on the model's other presets.
    pub is_model_default: bool,
}

/// The model the user assigned to an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentModel {
    /// Agent name.
    pub agent: String,
    /// Where the model runs.
    pub model_source: ModelSource,
    /// The model: a local registry identifier, or `<server>/<model>`.
    pub model_id: String,
    /// The preset chosen for the agent, a preset of that same model; `None`
    /// leaves the model's default preset in force.
    pub preset_id: Option<i64>,
}

/// The default model of the machine, read from the `is_default` backend row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefaultModel {
    /// Name of the backend row holding the flag, the name the router routes by.
    pub backend: String,
    /// Where the model runs.
    pub model_source: ModelSource,
    /// The model, in the form [`LlmBackendRepository::set_default_model`]
    /// accepts: the backend name for a local engine row or an address-bearing
    /// server row, `<server>/<model>` for a row that names a declared server.
    pub model_id: String,
}

fn invalid(msg: impl Into<String>) -> LlmBackendError {
    LlmBackendError::InvalidPreset(msg.into())
}

fn check_model_id(model_id: &str) -> Result<(), LlmBackendError> {
    if model_id.trim().is_empty() {
        return Err(invalid("the model identifier is empty"));
    }
    Ok(())
}

fn check_no_system_prompt(
    column: &str,
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), LlmBackendError> {
    match SYSTEM_PROMPT_KEYS.iter().find(|k| object.contains_key(**k)) {
        Some(key) => Err(invalid(format!(
            "a preset carries no system prompt: '{key}' is refused in the {column} settings"
        ))),
        None => Ok(()),
    }
}

fn check_draft(draft: &PresetDraft) -> Result<(String, String), LlmBackendError> {
    if draft.name.trim().is_empty() {
        return Err(invalid("the preset name is empty"));
    }
    check_no_system_prompt("load", &draft.load)?;
    let load = serde_json::to_string(&draft.load)
        .map_err(|e| LlmBackendError::Serialization(e.to_string()))?;
    let request = serde_json::to_string(&draft.request)
        .map_err(|e| LlmBackendError::Serialization(e.to_string()))?;
    Ok((load, request))
}

/// A unique-constraint failure becomes [`LlmBackendError::PresetConflict`].
fn conflict_or_db(err: rusqlite::Error, name: &str) -> LlmBackendError {
    match &err {
        rusqlite::Error::SqliteFailure(e, _)
            if e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
        {
            LlmBackendError::PresetConflict(name.to_string())
        }
        _ => LlmBackendError::Db(err),
    }
}

const SELECT_PRESET: &str = "SELECT id, name, model_source, model_id, load_json, request_json, \
     is_model_default FROM model_presets";

type RawPreset = (i64, String, String, String, String, String, bool);

fn raw_preset(row: &rusqlite::Row<'_>) -> Result<RawPreset, rusqlite::Error> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get::<_, i64>(6)? != 0,
    ))
}

fn to_preset(raw: RawPreset) -> Result<ModelPreset, LlmBackendError> {
    let (id, name, source, model_id, load, request, is_model_default) = raw;
    let ser = |e: serde_json::Error| LlmBackendError::Serialization(e.to_string());
    Ok(ModelPreset {
        id,
        name,
        model_source: ModelSource::parse(&source)?,
        model_id,
        load: serde_json::from_str(&load).map_err(ser)?,
        request: serde_json::from_str(&request).map_err(ser)?,
        is_model_default,
    })
}

/// The name of a backend row created to hold a server model, derived from the
/// routing name and kept within `[a-z0-9_-]+`.
fn server_row_base_name(server: &str, model: &str) -> String {
    let mut out = String::with_capacity(server.len() + model.len() + 1);
    for c in format!("{server}-{model}").chars() {
        let c = c.to_ascii_lowercase();
        let keep = c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-';
        let mapped = if keep { c } else { '-' };
        if !(mapped == '-' && out.ends_with('-')) {
            out.push(mapped);
        }
    }
    out.trim_end_matches('-').to_string()
}

fn server_ref(cfg: &LlmBackendConfig) -> Option<&str> {
    cfg.config_json
        .get(SERVER_REF_FIELD)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
}

impl LlmBackendRepository {
    // ── Presets ──────────────────────────────────────────────────────────

    /// Every preset, or those of one model, sorted by model then name.
    ///
    /// # Errors
    /// [`LlmBackendError::Db`] or [`LlmBackendError::Serialization`].
    pub fn list_presets(
        &self,
        model: Option<(ModelSource, &str)>,
    ) -> Result<Vec<ModelPreset>, LlmBackendError> {
        let conn = self.conn.borrow();
        let order = "ORDER BY model_source, model_id, name";
        let raws: Vec<RawPreset> = match model {
            Some((source, id)) => {
                let mut stmt = conn.prepare(&format!(
                    "{SELECT_PRESET} WHERE model_source = ?1 AND model_id = ?2 {order}"
                ))?;
                let rows = stmt.query_map(params![source.as_str(), id], raw_preset)?;
                rows.collect::<Result<_, _>>()?
            }
            None => {
                let mut stmt = conn.prepare(&format!("{SELECT_PRESET} {order}"))?;
                let rows = stmt.query_map([], raw_preset)?;
                rows.collect::<Result<_, _>>()?
            }
        };
        raws.into_iter().map(to_preset).collect()
    }

    /// The preset `id`, or `None`.
    ///
    /// # Errors
    /// [`LlmBackendError::Db`] or [`LlmBackendError::Serialization`].
    pub fn find_preset(&self, id: i64) -> Result<Option<ModelPreset>, LlmBackendError> {
        let raw = self
            .conn
            .borrow()
            .query_row(
                &format!("{SELECT_PRESET} WHERE id = ?1"),
                params![id],
                raw_preset,
            )
            .optional()?;
        raw.map(to_preset).transpose()
    }

    /// Create a preset for a model and return it.
    ///
    /// # Errors
    /// - [`LlmBackendError::InvalidPreset`] for an empty name or model, or a
    ///   system prompt among the settings.
    /// - [`LlmBackendError::PresetConflict`] if the model already has a preset
    ///   of that name.
    /// - [`LlmBackendError::Db`] for any other SQLite error.
    pub fn create_preset(
        &self,
        model_source: ModelSource,
        model_id: &str,
        draft: &PresetDraft,
    ) -> Result<ModelPreset, LlmBackendError> {
        check_model_id(model_id)?;
        let (load, request) = check_draft(draft)?;
        let id = {
            let mut conn = self.conn.borrow_mut();
            let tx = conn.transaction()?;
            if draft.is_model_default {
                tx.execute(
                    "UPDATE model_presets SET is_model_default = 0
                     WHERE model_source = ?1 AND model_id = ?2",
                    params![model_source.as_str(), model_id],
                )?;
            }
            tx.execute(
                "INSERT INTO model_presets
                    (name, model_source, model_id, load_json, request_json, is_model_default)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    draft.name.trim(),
                    model_source.as_str(),
                    model_id,
                    load,
                    request,
                    draft.is_model_default as i64
                ],
            )
            .map_err(|e| conflict_or_db(e, &draft.name))?;
            let id = tx.last_insert_rowid();
            tx.commit()?;
            id
        };
        self.find_preset(id)?
            .ok_or(LlmBackendError::PresetNotFound(id))
    }

    /// Replace the name and settings of preset `id`, and return it.
    ///
    /// # Errors
    /// - [`LlmBackendError::PresetNotFound`] if `id` is absent.
    /// - otherwise as [`Self::create_preset`].
    pub fn update_preset(
        &self,
        id: i64,
        draft: &PresetDraft,
    ) -> Result<ModelPreset, LlmBackendError> {
        let (load, request) = check_draft(draft)?;
        let current = self
            .find_preset(id)?
            .ok_or(LlmBackendError::PresetNotFound(id))?;
        {
            let mut conn = self.conn.borrow_mut();
            let tx = conn.transaction()?;
            if draft.is_model_default {
                tx.execute(
                    "UPDATE model_presets SET is_model_default = 0
                     WHERE model_source = ?1 AND model_id = ?2 AND id != ?3",
                    params![current.model_source.as_str(), current.model_id, id],
                )?;
            }
            tx.execute(
                "UPDATE model_presets
                 SET name = ?2, load_json = ?3, request_json = ?4, is_model_default = ?5,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                params![
                    id,
                    draft.name.trim(),
                    load,
                    request,
                    draft.is_model_default as i64
                ],
            )
            .map_err(|e| conflict_or_db(e, &draft.name))?;
            tx.commit()?;
        }
        self.find_preset(id)?
            .ok_or(LlmBackendError::PresetNotFound(id))
    }

    /// Delete preset `id`. An agent that had chosen it falls back to its
    /// model's default preset (a trigger clears the choice).
    ///
    /// # Errors
    /// - [`LlmBackendError::PresetNotFound`] if `id` is absent.
    /// - [`LlmBackendError::Db`] for any SQLite error.
    pub fn delete_preset(&self, id: i64) -> Result<(), LlmBackendError> {
        let deleted = self
            .conn
            .borrow()
            .execute("DELETE FROM model_presets WHERE id = ?1", params![id])?;
        if deleted == 0 {
            return Err(LlmBackendError::PresetNotFound(id));
        }
        Ok(())
    }

    // ── Model of an agent ────────────────────────────────────────────────

    /// The model the user assigned to `agent`, or `None`.
    ///
    /// # Errors
    /// [`LlmBackendError::Db`] or [`LlmBackendError::Serialization`].
    pub fn agent_model(&self, agent: &str) -> Result<Option<AgentModel>, LlmBackendError> {
        let raw: Option<(String, String, String, Option<i64>)> = self
            .conn
            .borrow()
            .query_row(
                "SELECT agent, model_source, model_id, preset_id FROM agent_model
                 WHERE agent = ?1",
                params![agent],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        raw.map(|(agent, source, model_id, preset_id)| {
            Ok(AgentModel {
                agent,
                model_source: ModelSource::parse(&source)?,
                model_id,
                preset_id,
            })
        })
        .transpose()
    }

    /// Assign a model, and optionally a preset of it, to an agent.
    ///
    /// A server model must name a declared server (`<server>/<model>`) or an
    /// existing server backend row. A local model is not checked against the
    /// registry: a model that has gone falls back to the default at call time.
    ///
    /// # Errors
    /// - [`LlmBackendError::InvalidPreset`] for an empty agent or model, a
    ///   server model nothing declares, or a preset of another model.
    /// - [`LlmBackendError::PresetNotFound`] if the preset is absent.
    /// - [`LlmBackendError::Db`] for any SQLite error.
    pub fn set_agent_model(&self, assignment: &AgentModel) -> Result<(), LlmBackendError> {
        if assignment.agent.trim().is_empty() {
            return Err(invalid("the agent name is empty"));
        }
        check_model_id(&assignment.model_id)?;
        if assignment.model_source == ModelSource::Server {
            self.check_server_model(&assignment.model_id)?;
        }
        if let Some(preset_id) = assignment.preset_id {
            let preset = self
                .find_preset(preset_id)?
                .ok_or(LlmBackendError::PresetNotFound(preset_id))?;
            if preset.model_source != assignment.model_source
                || preset.model_id != assignment.model_id
            {
                return Err(invalid(format!(
                    "preset {preset_id} belongs to model '{}', not to '{}'",
                    preset.model_id, assignment.model_id
                )));
            }
        }
        self.conn.borrow().execute(
            "INSERT INTO agent_model (agent, model_source, model_id, preset_id)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(agent) DO UPDATE SET
                model_source = excluded.model_source,
                model_id     = excluded.model_id,
                preset_id    = excluded.preset_id,
                updated_at   = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
            params![
                assignment.agent,
                assignment.model_source.as_str(),
                assignment.model_id,
                assignment.preset_id
            ],
        )?;
        Ok(())
    }

    /// Remove the user's assignment for `agent`, which goes back to the model
    /// its package declares, or to the default. Returns whether one existed.
    ///
    /// # Errors
    /// [`LlmBackendError::Db`] for any SQLite error.
    pub fn clear_agent_model(&self, agent: &str) -> Result<bool, LlmBackendError> {
        let deleted = self
            .conn
            .borrow()
            .execute("DELETE FROM agent_model WHERE agent = ?1", params![agent])?;
        Ok(deleted > 0)
    }

    /// Refuse a server model that names no declared server and no server row.
    fn check_server_model(&self, model_id: &str) -> Result<(), LlmBackendError> {
        match split_route_name(model_id) {
            Some((server, _)) => {
                if self.find_server(server)?.is_none() {
                    return Err(invalid(format!("server '{server}' is not declared")));
                }
            }
            None => match self.find_by_name(model_id)? {
                Some(cfg) if cfg.provider != LlmProvider::LlamaCpp => {}
                _ => {
                    return Err(invalid(format!(
                        "'{model_id}' is neither <server>/<model> nor a server backend"
                    )))
                }
            },
        }
        Ok(())
    }

    // ── Default model of the machine ─────────────────────────────────────

    /// The default model, read from the single `is_default` backend row.
    ///
    /// # Errors
    /// [`LlmBackendError::Db`] or [`LlmBackendError::Serialization`].
    pub fn default_model(&self) -> Result<Option<DefaultModel>, LlmBackendError> {
        Ok(self.find_default()?.map(|cfg| {
            let (model_source, model_id) = match (&cfg.provider, server_ref(&cfg)) {
                (LlmProvider::LlamaCpp, _) => (ModelSource::Local, cfg.name.clone()),
                (_, Some(server)) => (
                    ModelSource::Server,
                    crate::inference_server::route_name(server, &cfg.model),
                ),
                (_, None) => (ModelSource::Server, cfg.name.clone()),
            };
            DefaultModel {
                backend: cfg.name,
                model_source,
                model_id,
            }
        }))
    }

    /// Make a model the default of the machine, and return it.
    ///
    /// - Local: `model_id` names a local engine backend row.
    /// - Server: `model_id` is `<server>/<model>` of a declared server, or the
    ///   name of an existing server backend row. A server model with no row
    ///   gets one, of provider `openai`, naming the server in
    ///   [`SERVER_REF_FIELD`] and carrying its address; a later call for the
    ///   same model reuses it.
    ///
    /// Either way the flag moves to that one row, so the default stays unique.
    ///
    /// # Errors
    /// - [`LlmBackendError::NotFound`] for a backend name that is absent.
    /// - [`LlmBackendError::InvalidPreset`] for a model of the wrong kind or a
    ///   server nothing declares.
    /// - [`LlmBackendError::Db`] for any SQLite error.
    pub fn set_default_model(
        &self,
        model_source: ModelSource,
        model_id: &str,
    ) -> Result<DefaultModel, LlmBackendError> {
        check_model_id(model_id)?;
        let backend = match (model_source, split_route_name(model_id)) {
            (ModelSource::Server, Some((server, model))) => self.server_model_row(server, model)?,
            (source, _) => {
                let cfg = self
                    .find_by_name(model_id)?
                    .ok_or_else(|| LlmBackendError::NotFound(model_id.to_string()))?;
                let local = cfg.provider == LlmProvider::LlamaCpp;
                if local != (source == ModelSource::Local) {
                    return Err(invalid(format!(
                        "backend '{model_id}' is not a {} model",
                        source.as_str()
                    )));
                }
                cfg.name
            }
        };
        self.set_default(&backend)?;
        self.default_model()?
            .ok_or(LlmBackendError::NotFound(backend))
    }

    /// The backend row that designates `model` of declared `server`, created
    /// when there is none yet.
    fn server_model_row(&self, server: &str, model: &str) -> Result<String, LlmBackendError> {
        let declared = self
            .find_server(server)?
            .ok_or_else(|| invalid(format!("server '{server}' is not declared")))?;
        let rows = self.list()?;
        if let Some(cfg) = rows
            .iter()
            .find(|cfg| server_ref(cfg) == Some(server) && cfg.model == model)
        {
            return Ok(cfg.name.clone());
        }
        let base = server_row_base_name(server, model);
        let mut name = base.clone();
        let mut n = 2;
        while rows.iter().any(|cfg| cfg.name == name) {
            name = format!("{base}-{n}");
            n += 1;
        }
        self.save(&LlmBackendConfig {
            name: name.clone(),
            provider: LlmProvider::OpenAi,
            model: model.to_string(),
            config_json: serde_json::json!({
                SERVER_REF_FIELD: server,
                "base_url": declared.base_url,
            }),
            enabled: true,
            is_default: false,
        })?;
        Ok(name)
    }
}

#[cfg(test)]
mod tests;
