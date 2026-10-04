//! The one writer of `apollia.toml`.
//!
//! Every surface that changes the file goes through [`write_config`]: the
//! settings screens of the desktop application, the command line, the routes of
//! the runtime and the backends it mirrors. Whatever the caller, a write runs
//! the same four checks before a byte reaches the disk:
//!
//! 1. the candidate text validates ([`validate`](super::validate));
//! 2. no line gives a key a value other than the one the organisation policy
//!    imposes ([`PolicySettings`]);
//! 3. the file on disk is still the one the caller read, by its SHA-256
//!    fingerprint, so two writers never overwrite each other's change;
//! 4. the text goes to a sibling file renamed over the original, so a reader
//!    never sees half a file.
//!
//! An edit is either a whole text, the way the editor saves, or a list of key
//! changes applied with `toml_edit`, which keeps the user's comments and the
//! order of the file.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::keys;
use super::scan::{line_col, offset_of};
use super::settings::{read_config, set_in_document, to_toml};
use super::{default_file_text, validate, ConfigDiagnostic, PolicySettings, SettingError};

/// Serializes the writes of this process: the fingerprint check and the
/// rename are one step for every thread that writes the file.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// Distinguishes the staging files of concurrent writers of this process.
static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

/// One change of a key, in dotted notation.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyChange {
    /// Dotted key, for example `tools.python_interpreter`.
    pub key: String,
    /// The new value; `None` removes the key, which then reads as its default.
    pub value: Option<Value>,
}

impl KeyChange {
    /// Sets `key` to `value`.
    pub fn set(key: impl Into<String>, value: Value) -> Self {
        Self {
            key: key.into(),
            value: Some(value),
        }
    }

    /// Removes `key` from the file.
    pub fn remove(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: None,
        }
    }
}

/// What a caller asks the writer to write.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigEdit {
    /// The whole text of the file, validated strictly. A line may give a key
    /// the policy fixes its imposed value, never another one.
    Text(String),
    /// Key changes applied to the file as it is. A section a change leaves
    /// incomplete on purpose (`llm.default` written before `llm.backends`) is
    /// accepted; an unknown key, a withdrawn key, a value of the wrong type,
    /// or a key the policy fixes is not.
    Keys(Vec<KeyChange>),
}

/// The file as read, with the fingerprint a later write must present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConfigSnapshot {
    /// The text of the file; empty when the file does not exist.
    pub text: String,
    /// SHA-256 of `text`, lowercase hexadecimal.
    pub sha256: String,
    /// Whether the file exists.
    pub exists: bool,
}

/// A key the organisation policy imposes, with the value it imposes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LockedKey {
    /// Dotted key.
    pub key: String,
    /// The value the policy imposes.
    pub value: Value,
}

/// Why the writer refused a write. The file is left exactly as it was.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigWriteError {
    /// The candidate text does not validate.
    #[error("{}", invalid_message(key.as_deref(), diagnostics))]
    Invalid {
        /// The key whose change made the text invalid, for a key edit.
        key: Option<String>,
        /// Every fault, each at its line.
        diagnostics: Vec<ConfigDiagnostic>,
    },
    /// A line gives a key the policy imposes another value.
    #[error(
        "line {line}: '{key}' is fixed by the organisation policy at {imposed} and cannot be changed"
    )]
    LockedByPolicy {
        /// 1-based line of the key in the candidate text.
        line: usize,
        /// Dotted key.
        key: String,
        /// The value the policy imposes.
        imposed: Value,
    },
    /// The file changed on disk since the caller read it.
    #[error("the configuration file was modified in the meantime; read it again before saving")]
    Modified {
        /// The fingerprint the caller presented.
        expected: String,
        /// The fingerprint of the file on disk.
        actual: String,
    },
    /// A key change names no key of the registry.
    #[error("unknown config key: '{key}'")]
    UnknownKey {
        /// Dotted key.
        key: String,
    },
    /// A key change names a withdrawn key.
    #[error("'{key}' is a withdrawn setting: {detail}")]
    Withdrawn {
        /// Dotted key.
        key: String,
        /// What replaced it.
        detail: String,
    },
    /// A key change cannot be applied to the file as it is.
    #[error("invalid config value for '{key}': {reason}")]
    InvalidValue {
        /// Dotted key.
        key: String,
        /// Why.
        reason: String,
    },
    /// The file could not be read.
    #[error("failed to read {}", path.display())]
    Read {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The file could not be written.
    #[error("failed to write {}", path.display())]
    Write {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
}

fn invalid_message(key: Option<&str>, diagnostics: &[ConfigDiagnostic]) -> String {
    let rendered = render(diagnostics);
    match key {
        Some(key) => format!("invalid config value for '{key}': {rendered}"),
        None => format!("invalid configuration: {rendered}"),
    }
}

fn render(diagnostics: &[ConfigDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

impl From<SettingError> for ConfigWriteError {
    fn from(e: SettingError) -> Self {
        match e {
            SettingError::Read { path, source } => Self::Read { path, source },
            SettingError::Write { path, source } => Self::Write { path, source },
            SettingError::UnknownKey { key } => Self::UnknownKey { key },
            SettingError::InvalidValue { key, reason } => Self::InvalidValue { key, reason },
            SettingError::LockedByPolicy { key } => Self::LockedByPolicy {
                line: 1,
                key,
                imposed: Value::Null,
            },
            SettingError::Parse { path, reason } => Self::Invalid {
                key: None,
                diagnostics: vec![ConfigDiagnostic {
                    line: 1,
                    column: 1,
                    path: String::new(),
                    message: format!("{}: {reason}", path.display()),
                }],
            },
        }
    }
}

/// SHA-256 of `text`, lowercase hexadecimal.
pub fn fingerprint(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Reads the file at `path` with its fingerprint. An absent file reads as an
/// empty text.
///
/// # Errors
///
/// [`ConfigWriteError::Read`] when the file exists and cannot be read.
pub fn read_snapshot(path: &Path) -> Result<ConfigSnapshot, ConfigWriteError> {
    let exists = path.exists();
    let text = read_config(path)?;
    Ok(ConfigSnapshot {
        sha256: fingerprint(&text),
        text,
        exists,
    })
}

/// The 1-based line of `text` that declares the dotted `key`, or the line of
/// the nearest enclosing table the text declares; `None` when neither is
/// written.
pub fn key_line(text: &str, key: &str) -> Option<usize> {
    offset_of(text, key).map(|o| line_col(text, o).0)
}

/// The keys the policy imposes, with their values, in key order.
pub fn locked_keys(policy: &PolicySettings) -> Vec<LockedKey> {
    policy
        .imposed_keys()
        .map(|(key, value)| LockedKey {
            key: key.to_owned(),
            value: value.clone(),
        })
        .collect()
}

/// Every line of `text` that gives a key the policy imposes another value.
///
/// A line that is absent or commented imposes nothing and is never a fault:
/// the policy's value applies whatever the file says.
pub fn policy_faults(text: &str, policy: &PolicySettings) -> Vec<ConfigWriteError> {
    let Ok(document) = toml::from_str::<toml::Table>(text) else {
        return Vec::new();
    };
    policy
        .imposed_keys()
        .filter_map(|(key, imposed)| {
            let written = value_at(&document, key)?;
            if same_value(&written, imposed) {
                return None;
            }
            Some(locked_fault(text, key, imposed))
        })
        .collect()
}

fn locked_fault(text: &str, key: &str, imposed: &Value) -> ConfigWriteError {
    let line = offset_of(text, key).map_or(1, |o| line_col(text, o).0);
    ConfigWriteError::LockedByPolicy {
        line,
        key: key.to_owned(),
        imposed: imposed.clone(),
    }
}

/// The value the TOML document gives the dotted `key`, as JSON.
fn value_at(document: &toml::Table, key: &str) -> Option<Value> {
    let mut segments = key.split('.');
    let mut node = document.get(segments.next()?)?;
    for segment in segments {
        node = node.get(segment)?;
    }
    serde_json::to_value(node).ok()
}

/// Two JSON values, numbers compared by their value (`1` and `1.0` agree).
fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

/// The default text of the file, the keys the policy imposes written at their
/// imposed value: what "return to the defaults" saves without being refused.
pub fn default_text_under(policy: &PolicySettings) -> String {
    let text = default_file_text();
    let Ok(mut document) = text.parse::<toml_edit::DocumentMut>() else {
        return text;
    };
    for (key, value) in policy.imposed_keys() {
        let Ok(item) = to_toml(key, value) else {
            continue;
        };
        if set_in_document(&mut document, key, item).is_err() {
            continue;
        }
    }
    document.to_string()
}

/// Validates `text` as the writer would before writing it: strict validation,
/// then the policy. Returns every fault, empty when the text would be written.
pub fn check_text(text: &str, policy: &PolicySettings) -> Vec<ConfigDiagnostic> {
    let mut diagnostics = validate(text).err().unwrap_or_default();
    diagnostics.extend(policy_faults(text, policy).into_iter().filter_map(|e| {
        let ConfigWriteError::LockedByPolicy { line, key, imposed } = e else {
            return None;
        };
        Some(ConfigDiagnostic {
            line,
            column: 1,
            message: format!("fixed by the organisation policy at {imposed}"),
            path: key,
        })
    }));
    diagnostics
}

/// Applies `edit` to the file at `path`, after checking that the file on disk
/// still has the fingerprint `expected_sha256`, and returns what was written.
///
/// # Errors
///
/// [`ConfigWriteError::Modified`] when the file changed since it was read;
/// [`ConfigWriteError::Invalid`], [`ConfigWriteError::LockedByPolicy`],
/// [`ConfigWriteError::UnknownKey`], [`ConfigWriteError::Withdrawn`] or
/// [`ConfigWriteError::InvalidValue`] when the edit is refused;
/// [`ConfigWriteError::Read`] or [`ConfigWriteError::Write`] on an I/O
/// failure. The file is unchanged in every case.
pub fn write_config(
    path: &Path,
    edit: ConfigEdit,
    expected_sha256: &str,
    policy: &PolicySettings,
) -> Result<ConfigSnapshot, ConfigWriteError> {
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let current = read_snapshot(path)?;
    if current.sha256 != expected_sha256 {
        tracing::event!(
            tracing::Level::WARN,
            path = %path.display(),
            "config.write.modified"
        );
        return Err(ConfigWriteError::Modified {
            expected: expected_sha256.to_owned(),
            actual: current.sha256,
        });
    }
    let text = match edit {
        ConfigEdit::Text(text) => {
            check_whole_text(&text, policy)?;
            text
        }
        ConfigEdit::Keys(changes) => apply_changes(&current.text, &changes, policy)?,
    };
    if current.exists && text == current.text {
        return Ok(current);
    }
    write_atomically(path, &text)?;
    tracing::event!(
        tracing::Level::INFO,
        path = %path.display(),
        "config.write.done"
    );
    Ok(ConfigSnapshot {
        sha256: fingerprint(&text),
        text,
        exists: true,
    })
}

/// Reads the file, applies `edit`, and writes it back in one step: the
/// fingerprint presented is the one just read. For a caller that holds no
/// earlier reading, such as a command line that changes one key.
///
/// # Errors
///
/// As [`write_config`].
pub fn update_config(
    path: &Path,
    edit: ConfigEdit,
    policy: &PolicySettings,
) -> Result<ConfigSnapshot, ConfigWriteError> {
    let current = read_snapshot(path)?;
    write_config(path, edit, &current.sha256, policy)
}

fn check_whole_text(text: &str, policy: &PolicySettings) -> Result<(), ConfigWriteError> {
    validate(text).map_err(|diagnostics| ConfigWriteError::Invalid {
        key: None,
        diagnostics,
    })?;
    match policy_faults(text, policy).into_iter().next() {
        Some(fault) => Err(fault),
        None => Ok(()),
    }
}

fn apply_changes(
    original: &str,
    changes: &[KeyChange],
    policy: &PolicySettings,
) -> Result<String, ConfigWriteError> {
    for change in changes {
        if let Some(withdrawn) = keys::withdrawn_covering(&change.key) {
            return Err(ConfigWriteError::Withdrawn {
                key: change.key.clone(),
                detail: withdrawn.withdrawn.unwrap_or_default().to_owned(),
            });
        }
        if !keys::is_live_path(&change.key) {
            return Err(ConfigWriteError::UnknownKey {
                key: change.key.clone(),
            });
        }
    }
    // A key edit names the setting it changes: one the policy fixes is
    // refused whatever the value, as the settings screens lock it. A whole
    // text, by contrast, may carry the imposed value on its line.
    if let Some((change, imposed)) = changes
        .iter()
        .find_map(|c| policy.imposed(&c.key).map(|v| (c, v)))
    {
        tracing::event!(tracing::Level::WARN, key = %change.key, "config.setting.locked");
        return Err(locked_fault(original, &change.key, imposed));
    }
    let mut document: toml_edit::DocumentMut =
        original
            .parse()
            .map_err(|e: toml_edit::TomlError| ConfigWriteError::Invalid {
                key: None,
                diagnostics: vec![ConfigDiagnostic {
                    line: 1,
                    column: 1,
                    path: String::new(),
                    message: e.message().to_owned(),
                }],
            })?;
    for change in changes {
        match &change.value {
            Some(value) => {
                let item = to_toml(&change.key, value)?;
                set_in_document(&mut document, &change.key, item)?;
            }
            None => remove_in_document(&mut document, &change.key),
        }
    }
    let text = document.to_string();
    if let Err(diagnostics) = validate(&text) {
        // A key edit may leave a section incomplete on purpose; completeness
        // is what `apollia-os config validate` and the editor report.
        let blocking: Vec<ConfigDiagnostic> = diagnostics
            .into_iter()
            .filter(|d| !d.message.contains("missing field"))
            .collect();
        if !blocking.is_empty() {
            return Err(ConfigWriteError::Invalid {
                key: changes.first().map(|c| c.key.clone()),
                diagnostics: blocking,
            });
        }
    }
    Ok(text)
}

/// Removes the dotted `key` from `document`, leaving its table in place.
fn remove_in_document(document: &mut toml_edit::DocumentMut, key: &str) {
    let segments: Vec<&str> = key.split('.').collect();
    let Some((leaf, parents)) = segments.split_last() else {
        return;
    };
    let mut table: &mut dyn toml_edit::TableLike = document.as_table_mut();
    for segment in parents {
        match table
            .get_mut(segment)
            .and_then(toml_edit::Item::as_table_like_mut)
        {
            Some(next) => table = next,
            None => return,
        }
    }
    table.remove(leaf);
}

/// Creates the file at `path` holding `text` when no file is there, never
/// over an existing one, even one created between the check and the write.
///
/// Returns `true` when the file was written.
///
/// # Errors
///
/// The I/O error of creating the folder or writing the file.
pub(crate) fn create_if_absent(path: &Path, text: &str) -> std::io::Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(e) => return Err(e),
    };
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    Ok(true)
}

/// Writes `text` to a sibling of `path`, then renames it over `path`.
fn write_atomically(path: &Path, text: &str) -> Result<(), ConfigWriteError> {
    let failed = |source| ConfigWriteError::Write {
        path: path.to_path_buf(),
        source,
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(failed)?;
    let name = path
        .file_name()
        .map_or_else(|| "apollia.toml".into(), |n| n.to_string_lossy());
    let staging = parent.join(format!(
        ".{name}.{}.{}.writing",
        std::process::id(),
        STAGING_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| {
        let mut file = std::fs::File::create(&staging)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&staging, path)
    })();
    if let Err(source) = written {
        let _ = std::fs::remove_file(&staging);
        return Err(failed(source));
    }
    Ok(())
}
