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
//! governance; until it lands every caller passes [`PolicySettings::none`],
//! and every write goes through.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::{DesktopConfig, ServerConfig};

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
/// Empty in this version: the source of the policy is not decided yet, and an
/// empty policy locks nothing.
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

/// The defaults of every typed section whose fields are named settings, as one
/// JSON tree keyed by section name.
fn defaults() -> serde_json::Map<String, Value> {
    let mut sections = serde_json::Map::new();
    // A struct of plain fields always serializes; a failure would only make
    // its keys unknown, never a wrong value.
    if let Ok(desktop) = serde_json::to_value(DesktopConfig::default()) {
        sections.insert("desktop".to_owned(), desktop);
    }
    if let Ok(server) = serde_json::to_value(ServerConfig::default()) {
        sections.insert("server".to_owned(), server);
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

/// Checks that `value` has the JSON type of `default`.
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
    if fits {
        return Ok(());
    }
    Err(SettingError::InvalidValue {
        key: key.to_owned(),
        reason: format!("expected a value like {default}, got {value}"),
    })
}

/// Reads `path`, an absent file reading as an empty document.
fn read_config(path: &Path) -> Result<String, SettingError> {
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
fn to_toml(key: &str, value: &Value) -> Result<toml_edit::Value, SettingError> {
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
fn set_in_document(
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
    table.insert(leaf, toml_edit::Item::Value(value));
    Ok(())
}

/// Writes `content` to `path` through a sibling file renamed over it, so a
/// reader never sees half a file.
fn write_atomically(path: &Path, content: &str) -> Result<(), SettingError> {
    let failed = |source| SettingError::Write {
        path: path.to_path_buf(),
        source,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(failed)?;
    }
    let staging = path.with_extension("toml.writing");
    let mut file = std::fs::File::create(&staging).map_err(failed)?;
    file.write_all(content.as_bytes()).map_err(failed)?;
    file.sync_all().map_err(failed)?;
    std::fs::rename(&staging, path).map_err(failed)
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
    let content = read_config(path)?;
    let mut document: toml_edit::DocumentMut =
        content
            .parse()
            .map_err(|e: toml_edit::TomlError| SettingError::Parse {
                path: path.to_path_buf(),
                reason: e.message().to_owned(),
            })?;
    set_in_document(&mut document, key, to_toml(key, &value)?)?;
    let written = document.to_string();
    let invalid = |e: toml::de::Error| SettingError::InvalidValue {
        key: key.to_owned(),
        reason: e.message().to_owned(),
    };
    DesktopConfig::from_toml_str(&written).map_err(invalid)?;
    ServerConfig::from_toml_str(&written).map_err(invalid)?;
    write_atomically(path, &written)?;
    tracing::event!(tracing::Level::INFO, key = %key, "config.setting.written");
    Ok(ResolvedSetting {
        key: key.to_owned(),
        value,
        source: SettingSource::User,
    })
}
