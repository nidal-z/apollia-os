//! Named settings of `apollia.toml`, resolved with the source of their value.
//!
//! A named setting is a key in dotted notation, `desktop.developer_mode` for
//! instance, the same form the `[settings]` table of an organization policy
//! uses. Its value comes from one of three places, in this order:
//!
//! 1. the policy of the organization, when it imposes the key
//!    ([`SettingSource::Policy`]); the setting is then locked and
//!    [`write_setting`] refuses it with [`SettingError::LockedByPolicy`];
//! 2. the user's `apollia.toml`, when it carries the key
//!    ([`SettingSource::User`]);
//! 3. the default of the typed section ([`SettingSource::Default`]).
//!
//! This module holds the contract only. Where the policy comes from (a file
//! outside the user's folder, its format, its signature) belongs to
//! governance, which builds the [`PolicySettings`] from the `[settings]` table
//! of the loaded policy and checks each of its keys with
//! [`check_imposed_setting`]: a key that names no setting, or a value of the
//! wrong type, makes the whole policy invalid. Without a policy, callers pass
//! [`PolicySettings::none`] and every write goes through.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::{
    AuthorConfig, DesktopConfig, EnginesConfig, LlmDefaultsConfig, LlmDevicesConfig,
    LlmImagesConfig, LlmResidencyConfig, McpConfig, ModelsConfig, ServerConfig, CONTEXT_MODES,
    CONTEXT_MODE_KEY, ENGINE_LOG_LEVELS, GGUF_ENGINE_CHOICES, LOAD_BYPASS_KEY, LOAD_BYPASS_MODES,
    MLX_ENGINE_CHOICES,
};

/// Wire names of the load guardrail levels, the values `llm.guardrails.level`
/// accepts. The runtime holds its level enum to this list in a test.
pub const GUARDRAIL_LEVELS: &[&str] = &["disabled", "lenient", "balanced", "strict", "ceiling"];

/// Settings whose value is one of a closed list, by dotted key.
const CHOICES: &[(&str, &[&str])] = &[
    ("llm.guardrails.level", GUARDRAIL_LEVELS),
    (LOAD_BYPASS_KEY, LOAD_BYPASS_MODES),
    (CONTEXT_MODE_KEY, CONTEXT_MODES),
    ("engines.log_level", ENGINE_LOG_LEVELS),
    ("engines.gguf", GGUF_ENGINE_CHOICES),
    ("engines.mlx", MLX_ENGINE_CHOICES),
];

/// Where the value of a resolved setting comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingSource {
    /// No one wrote the key: the section's default applies.
    Default,
    /// The user's `apollia.toml` carries the key.
    User,
    /// The organization policy imposes the value; the setting is locked.
    Policy,
}

/// A named setting with its value and the source of that value.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResolvedSetting {
    /// Dotted key, for example `desktop.developer_mode`.
    pub key: String,
    /// The value in force.
    pub value: Value,
    /// Where the value comes from.
    pub source: SettingSource,
}

/// The settings an organization policy imposes, by dotted key.
///
/// An empty value, [`PolicySettings::none`], locks nothing: it is what a
/// machine without a policy resolves against.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PolicySettings {
    imposed: BTreeMap<String, Value>,
}

impl PolicySettings {
    /// A policy that imposes nothing.
    pub fn none() -> Self {
        Self::default()
    }

    /// Returns the policy with `key` imposed at `value`.
    #[must_use]
    pub fn impose(mut self, key: impl Into<String>, value: Value) -> Self {
        self.imposed.insert(key.into(), value);
        self
    }

    /// The value the policy imposes on `key`, if any.
    pub fn imposed(&self, key: &str) -> Option<&Value> {
        self.imposed.get(key)
    }

    /// Whether the policy locks `key`.
    pub fn locks(&self, key: &str) -> bool {
        self.imposed.contains_key(key)
    }

    /// Every imposed key with its value, in key order.
    pub(crate) fn imposed_keys(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.imposed.iter().map(|(k, v)| (k.as_str(), v))
    }
}

/// Why a named setting could not be read or written.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SettingError {
    /// The key names no setting of a typed section.
    #[error("unknown setting '{key}'")]
    UnknownKey {
        /// The dotted key asked for.
        key: String,
    },
    /// The organization policy imposes the value; the file is left unchanged.
    #[error("setting '{key}' is locked by the organization policy")]
    LockedByPolicy {
        /// The dotted key refused.
        key: String,
    },
    /// The value does not have the type of the setting.
    #[error("invalid value for setting '{key}': {reason}")]
    InvalidValue {
        /// The dotted key.
        key: String,
        /// What the setting expects.
        reason: String,
    },
    /// The configuration file could not be read.
    #[error("failed to read {}", path.display())]
    Read {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The configuration file is not valid TOML.
    #[error("invalid TOML in {}: {reason}", path.display())]
    Parse {
        /// The file.
        path: PathBuf,
        /// The parser's message.
        reason: String,
    },
    /// The configuration file could not be written.
    #[error("failed to write {}", path.display())]
    Write {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
}

/// The defaults of every named setting, as one JSON tree keyed by section name.
///
/// The whole `[desktop]` and `[mcp]` sections, then the inference settings an organisation
/// may fix: the model registry (`models.registry_url`,
/// `models.registry_mirror_url`, `models.registry_enabled`), the import rule
/// (`models.import.require_verified`) and the load guardrail
/// (`llm.guardrails.level`, `llm.guardrails.ceiling_gb`, where `0` sets no
/// ceiling, `llm.guardrails.bypass`), the idle unload delay of a local model
/// (`llm.residency.idle_unload_minutes`, where `0` never unloads), the default
/// context window (`[llm.defaults]`), the image ceiling (`[llm.images]`) and
/// the graphics devices held back (`[llm.devices]`), then the whole `[server]`
/// and `[engines]` sections.
fn defaults() -> serde_json::Map<String, Value> {
    let mut sections = serde_json::Map::new();
    // A struct of plain fields always serializes; a failure would only make
    // its keys unknown, never a wrong value.
    if let Ok(desktop) = serde_json::to_value(DesktopConfig::default()) {
        sections.insert("desktop".to_owned(), desktop);
    }
    if let Ok(Value::Object(mut models)) = serde_json::to_value(ModelsConfig::default()) {
        models.insert(
            "import".to_owned(),
            serde_json::json!({ "require_verified": false }),
        );
        sections.insert("models".to_owned(), Value::Object(models));
    }
    let mut llm = serde_json::json!({
        "guardrails": { "level": "balanced", "ceiling_gb": 0.0, "bypass": "confirm" }
    });
    if let Some(llm) = llm.as_object_mut() {
        for (name, section) in [
            (
                "residency",
                serde_json::to_value(LlmResidencyConfig::default()),
            ),
            (
                "defaults",
                serde_json::to_value(LlmDefaultsConfig::default()),
            ),
            ("images", serde_json::to_value(LlmImagesConfig::default())),
            ("devices", serde_json::to_value(LlmDevicesConfig::default())),
        ] {
            if let Ok(section) = section {
                llm.insert(name.to_owned(), section);
            }
        }
    }
    sections.insert("llm".to_owned(), llm);
    if let Ok(mcp) = serde_json::to_value(McpConfig::default()) {
        sections.insert("mcp".to_owned(), mcp);
    }
    if let Ok(server) = serde_json::to_value(ServerConfig::default()) {
        sections.insert("server".to_owned(), server);
    }
    if let Ok(engines) = serde_json::to_value(EnginesConfig::default()) {
        sections.insert("engines".to_owned(), engines);
    }
    if let Ok(author) = serde_json::to_value(AuthorConfig::default()) {
        sections.insert("author".to_owned(), author);
    }
    sections
}

/// The default value of the setting `key`, or `None` when `key` names no
/// setting (a whole section is not a setting).
fn default_of(key: &str) -> Option<Value> {
    let sections = defaults();
    let mut segments = key.split('.');
    let mut node = sections.get(segments.next()?)?;
    for segment in segments {
        node = node.as_object()?.get(segment)?;
    }
    (!node.is_object()).then(|| node.clone())
}

/// Checks that `value` has the JSON type of `default`, and is one of the
/// allowed values when the setting has a closed list.
fn check_type(key: &str, value: &Value, default: &Value) -> Result<(), SettingError> {
    let fits = match default {
        Value::Bool(_) => value.is_boolean(),
        Value::Number(n) if n.is_f64() => value.is_number(),
        Value::Number(_) => value.is_i64() || value.is_u64(),
        Value::String(_) => value.is_string(),
        Value::Array(_) => value.is_array(),
        Value::Null => true,
        Value::Object(_) => false,
    };
    if !fits {
        return Err(SettingError::InvalidValue {
            key: key.to_owned(),
            reason: format!("expected a value like {default}, got {value}"),
        });
    }
    let Some((_, allowed)) = CHOICES.iter().find(|(name, _)| *name == key) else {
        return Ok(());
    };
    if value.as_str().is_some_and(|v| allowed.contains(&v)) {
        return Ok(());
    }
    Err(SettingError::InvalidValue {
        key: key.to_owned(),
        reason: format!("expected one of {}, got {value}", allowed.join(", ")),
    })
}

/// Checks that a policy may impose `value` on the setting `key`: the key names
/// a setting of a typed section, and the value has that setting's type.
///
/// The policy loader calls it on every key of its `[settings]` table, so a
/// policy that names a setting this build does not know is refused whole
/// rather than half applied.
///
/// # Errors
///
/// [`SettingError::UnknownKey`] when `key` names no setting,
/// [`SettingError::InvalidValue`] when `value` has another type.
pub fn check_imposed_setting(key: &str, value: &Value) -> Result<(), SettingError> {
    let default = default_of(key).ok_or_else(|| SettingError::UnknownKey {
        key: key.to_owned(),
    })?;
    check_type(key, value, &default)
}

/// Reads `path`, an absent file reading as an empty document.
pub(crate) fn read_config(path: &Path) -> Result<String, SettingError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(source) => Err(SettingError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Resolves the setting `key` against the content of an `apollia.toml`.
///
/// `path` only names the document in errors.
///
/// # Errors
///
/// [`SettingError::UnknownKey`], [`SettingError::Parse`], or
/// [`SettingError::InvalidValue`] when the file holds a value of the wrong
/// type.
pub fn resolve_setting_in(
    content: &str,
    path: &Path,
    key: &str,
    policy: &PolicySettings,
) -> Result<ResolvedSetting, SettingError> {
    let default = default_of(key).ok_or_else(|| SettingError::UnknownKey {
        key: key.to_owned(),
    })?;
    if let Some(imposed) = policy.imposed(key) {
        return Ok(ResolvedSetting {
            key: key.to_owned(),
            value: imposed.clone(),
            source: SettingSource::Policy,
        });
    }
    let document: toml::Table = toml::from_str(content).map_err(|e| SettingError::Parse {
        path: path.to_path_buf(),
        reason: e.message().to_owned(),
    })?;
    let mut segments = key.split('.');
    let mut found = segments
        .next()
        .and_then(|first| document.get(first))
        .cloned();
    for segment in segments {
        found = found.and_then(|node| node.get(segment).cloned());
    }
    let Some(written) = found else {
        return Ok(ResolvedSetting {
            key: key.to_owned(),
            value: default,
            source: SettingSource::Default,
        });
    };
    let value = serde_json::to_value(&written).map_err(|e| SettingError::InvalidValue {
        key: key.to_owned(),
        reason: e.to_string(),
    })?;
    check_type(key, &value, &default)?;
    Ok(ResolvedSetting {
        key: key.to_owned(),
        value,
        source: SettingSource::User,
    })
}

/// Resolves the setting `key` from the `apollia.toml` at `path`: the value the
/// policy imposes, else the file's, else the default. An absent file resolves
/// every setting to its default.
///
/// This is the read side of the contract the configuration writer and the
/// locked lines of the settings editor call.
///
/// # Errors
///
/// [`SettingError::UnknownKey`], [`SettingError::Read`],
/// [`SettingError::Parse`], or [`SettingError::InvalidValue`].
pub fn resolve_setting(
    path: &Path,
    key: &str,
    policy: &PolicySettings,
) -> Result<ResolvedSetting, SettingError> {
    let content = read_config(path)?;
    resolve_setting_in(&content, path, key, policy)
}

/// Converts a JSON scalar or array into a TOML value.
pub(crate) fn to_toml(key: &str, value: &Value) -> Result<toml_edit::Value, SettingError> {
    let invalid = || SettingError::InvalidValue {
        key: key.to_owned(),
        reason: format!("{value} has no TOML form"),
    };
    Ok(match value {
        Value::Bool(b) => toml_edit::Value::from(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => toml_edit::Value::from(i),
            None => toml_edit::Value::from(n.as_f64().ok_or_else(invalid)?),
        },
        Value::String(s) => toml_edit::Value::from(s.as_str()),
        Value::Array(items) => {
            let mut array = toml_edit::Array::new();
            for item in items {
                array.push(to_toml(key, item)?);
            }
            toml_edit::Value::Array(array)
        }
        Value::Null | Value::Object(_) => return Err(invalid()),
    })
}

/// Sets `key` to `value` in `document`, creating the section header when the
/// file has none, and leaving every other line as written.
pub(crate) fn set_in_document(
    document: &mut toml_edit::DocumentMut,
    key: &str,
    value: toml_edit::Value,
) -> Result<(), SettingError> {
    let segments: Vec<&str> = key.split('.').collect();
    let Some((leaf, parents)) = segments.split_last() else {
        return Err(SettingError::UnknownKey {
            key: key.to_owned(),
        });
    };
    let mut table: &mut dyn toml_edit::TableLike = document.as_table_mut();
    for (depth, segment) in parents.iter().enumerate() {
        if table.get(segment).is_none() {
            let mut created = toml_edit::Table::new();
            created.set_implicit(depth + 1 < parents.len());
            table.insert(segment, toml_edit::Item::Table(created));
        }
        table = table
            .get_mut(segment)
            .and_then(toml_edit::Item::as_table_like_mut)
            .ok_or_else(|| SettingError::InvalidValue {
                key: key.to_owned(),
                reason: format!("'{segment}' is not a table in the file"),
            })?;
    }
    let mut value = value;
    // A value replaced keeps the spacing and the trailing comment of its line.
    if let Some(old) = table.get(leaf).and_then(toml_edit::Item::as_value) {
        *value.decor_mut() = old.decor().clone();
    }
    table.insert(leaf, toml_edit::Item::Value(value));
    Ok(())
}

/// Writes the setting `key` to the `apollia.toml` at `path`, keeping the
/// user's comments and layout, and returns it resolved.
///
/// A key the policy imposes is refused before the file is opened, so the file
/// is left exactly as it was. This is the write side of the contract the
/// configuration writer and the settings editor call.
///
/// # Errors
///
/// [`SettingError::LockedByPolicy`] when the policy imposes `key`;
/// [`SettingError::UnknownKey`], [`SettingError::InvalidValue`],
/// [`SettingError::Read`], [`SettingError::Parse`] or
/// [`SettingError::Write`] otherwise.
pub fn write_setting(
    path: &Path,
    key: &str,
    value: Value,
    policy: &PolicySettings,
) -> Result<ResolvedSetting, SettingError> {
    let default = default_of(key).ok_or_else(|| SettingError::UnknownKey {
        key: key.to_owned(),
    })?;
    if policy.locks(key) {
        tracing::event!(tracing::Level::WARN, key = %key, "config.setting.locked");
        return Err(SettingError::LockedByPolicy {
            key: key.to_owned(),
        });
    }
    check_type(key, &value, &default)?;
    super::writer::update_config(
        path,
        super::writer::ConfigEdit::Keys(vec![super::writer::KeyChange::set(key, value.clone())]),
        policy,
    )
    .map_err(|e| match e {
        super::writer::ConfigWriteError::Read { path, source } => {
            SettingError::Read { path, source }
        }
        super::writer::ConfigWriteError::Invalid {
            key: None,
            diagnostics,
        } => SettingError::Parse {
            path: path.to_path_buf(),
            reason: diagnostics
                .first()
                .map(|d| d.message.clone())
                .unwrap_or_default(),
        },
        super::writer::ConfigWriteError::Write { path, source } => {
            SettingError::Write { path, source }
        }
        other => SettingError::InvalidValue {
            key: key.to_owned(),
            reason: other.to_string(),
        },
    })?;
    tracing::event!(tracing::Level::INFO, key = %key, "config.setting.written");
    Ok(ResolvedSetting {
        key: key.to_owned(),
        value,
        source: SettingSource::User,
    })
}
