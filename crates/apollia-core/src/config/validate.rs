//! The one validation of `apollia.toml`, shared by the daemon, the CLI, the
//! desktop application and the editor.

use serde::{Deserialize, Serialize};

use super::scan::{covers, line_col, offset_of, path_at, scan, LineKind};
use super::{warn_retired_keys, ApolliaConfig, ConfigError};

/// One fault of an `apollia.toml` document, placed at its line.
///
/// Carried by `RuntimeEvent::ConfigInvalid`; every field reads as its default
/// from a payload written before it existed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfigDiagnostic {
    /// 1-based line of the fault.
    pub line: usize,
    /// 1-based column of the fault, in characters.
    pub column: usize,
    /// Dotted path of the key at fault, or of the nearest table the document
    /// declares; empty at the root.
    pub path: String,
    /// What is wrong, in English.
    pub message: String,
}

impl std::fmt::Display for ConfigDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}:{}: {}", self.line, self.column, self.message)
        } else {
            write!(
                f,
                "{}:{}: {}: {}",
                self.line, self.column, self.path, self.message
            )
        }
    }
}

/// Validates the text of an `apollia.toml` and returns the configuration it
/// describes, or every fault found, each at its line.
///
/// In order:
/// 1. the withdrawn keys of the registry are set aside, each with a
///    `config.key.retired` warning: a file written for an earlier version is
///    never refused for them;
/// 2. the rest is deserialized strictly: a syntax error, a value of the wrong
///    type, or a key no section declares is a fault;
/// 3. the `validate()` of every section the file writes runs: a value out of
///    its bounds is a fault.
///
/// An empty text is valid and gives the defaults.
///
/// # Errors
///
/// The diagnostics, never empty.
pub fn validate(text: &str) -> Result<ApolliaConfig, Vec<ConfigDiagnostic>> {
    let withdrawn = warn_retired_keys(text);
    let kept = set_aside(text, &withdrawn);
    let config: ApolliaConfig = toml::from_str(&kept).map_err(|e| vec![from_toml(&kept, &e)])?;
    let written = written_paths(&kept);
    let diagnostics: Vec<ConfigDiagnostic> = section_checks(&config, &written)
        .into_iter()
        .map(|e| from_section(&kept, &e))
        .collect();
    if diagnostics.is_empty() {
        Ok(config)
    } else {
        Err(diagnostics)
    }
}

/// The text with every line of a withdrawn key blanked, byte for byte, so the
/// positions the deserializer reports are those of the original text.
fn set_aside(text: &str, withdrawn: &[&str]) -> String {
    if withdrawn.is_empty() {
        return text.to_owned();
    }
    let mut bytes = text.as_bytes().to_vec();
    for line in scan(text) {
        if line.kind == LineKind::Other {
            continue;
        }
        if withdrawn.iter().any(|w| covers(w, &line.path)) {
            for b in bytes.iter_mut().take(line.end).skip(line.start) {
                *b = b' ';
            }
        }
    }
    // Only whole lines were blanked and a line ends on a character boundary,
    // so the bytes are still UTF-8; the fallback is unreachable.
    String::from_utf8(bytes).unwrap_or_else(|_| text.to_owned())
}

/// The dotted paths of every header and key the text writes.
fn written_paths(text: &str) -> Vec<String> {
    scan(text)
        .into_iter()
        .filter(|line| line.kind != LineKind::Other)
        .map(|line| line.path)
        .collect()
}

/// Runs the `validate()` of every section the file writes.
fn section_checks(config: &ApolliaConfig, written: &[String]) -> Vec<ConfigError> {
    let wrote = |section: &str| written.iter().any(|p| covers(section, p));
    let mut errors = Vec::new();
    let mut check = |result: Result<(), ConfigError>| {
        if let Err(e) = result {
            errors.push(e);
        }
    };
    if wrote("api") {
        // The default socket lies in the data folder, which the runtime
        // creates itself: only a socket the file names must have its folder.
        match config.api.validate() {
            Err(ConfigError::SocketParentMissing { .. }) if !wrote("api.unix_socket") => {}
            result => check(result),
        }
    }
    if wrote("runtime") {
        check(config.runtime.validate());
    }
    if wrote("hitl") {
        check(config.hitl.validate());
    }
    if wrote("tools") {
        check(config.tools.validate());
    }
    if wrote("mcp") {
        check(config.mcp.validate());
    }
    if wrote("hooks") {
        check(config.hooks.validate());
    }
    if wrote("workflows") {
        check(config.workflows.validate());
    }
    if let Some(hybrid) = config
        .llm
        .as_ref()
        .and_then(|llm| llm.routing.as_ref())
        .and_then(|routing| routing.hybrid.as_ref())
    {
        check(hybrid.validate());
    }
    errors
}

fn from_toml(text: &str, error: &toml::de::Error) -> ConfigDiagnostic {
    let message = error.message().trim().to_owned();
    let Some(span) = error.span() else {
        return ConfigDiagnostic {
            line: 1,
            column: 1,
            path: String::new(),
            message,
        };
    };
    let (line, column) = line_col(text, span.start);
    ConfigDiagnostic {
        line,
        column,
        path: path_at(text, span.start),
        message,
    }
}

fn from_section(text: &str, error: &ConfigError) -> ConfigDiagnostic {
    let path = error.key_path();
    let (line, column) = offset_of(text, &path).map_or((1, 1), |o| line_col(text, o));
    ConfigDiagnostic {
        line,
        column,
        path,
        message: error.to_string(),
    }
}

impl ConfigError {
    /// Dotted path of the key the error is about, list indices removed.
    pub fn key_path(&self) -> String {
        let raw = match self {
            Self::InvalidValue { field, .. } => field.as_str(),
            Self::OutOfBounds { key, .. } => key.as_str(),
            Self::SocketParentMissing { .. } => "api.unix_socket",
            Self::HybridFrontierMissing => "llm.routing.hybrid.frontier",
            Self::HybridCeilingInvalid { .. } => "llm.routing.hybrid.cost_ceiling_usd",
        };
        let mut path = String::with_capacity(raw.len());
        let mut in_index = false;
        for c in raw.chars() {
            match c {
                '[' => in_index = true,
                ']' => in_index = false,
                _ if !in_index => path.push(c),
                _ => {}
            }
        }
        path
    }
}
