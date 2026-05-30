//! Sampling defaults by model family.
//!
//! Three sources, in decreasing precedence (the first that answers wins over
//! the rest, field by field):
//!
//! 1. **User override**: `~/.apollia/models/sampling-defaults.json`, filled
//!    automatically when a model is downloaded via Apollia from HuggingFace
//!    (the source repo's `generation_config.json`) or edited by hand by the
//!    operator.
//! 2. **Embedded curated table**: `embedded.toml` shipped in the binary,
//!    ~10 popular families. Offline fallback source.
//! 3. **No match**: the caller uses its own global defaults (typically
//!    0.7 / 0.95 / 40).
//!
//! The caller can always override via `CompletionRequest.temperature`; this
//! module only resolves the defaults, it sets no policy.
//!
//! Legal note: the values published in the HF `generation_config.json` files
//! are numeric parameters recommended by the authors (facts, not
//! copyrightable in the sense of Feist v. Rural). The embedded table cites the
//! source for each entry. The model itself stays under its own license
//! (Llama, Qwen, Gemma, etc.); this module redistributes none of it.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Default sampling parameters for a given model. All fields are optional: a
/// model may have a recommendation only for `temperature` and leave the rest
/// to the global defaults.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelDefaults {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub repetition_penalty: Option<f32>,
}

impl ModelDefaults {
    /// Merge `other` into `self` WITHOUT overwriting fields already present.
    /// Used to layer the sources (override > embedded).
    pub fn fill_missing(mut self, other: &ModelDefaults) -> Self {
        if self.temperature.is_none() {
            self.temperature = other.temperature;
        }
        if self.top_p.is_none() {
            self.top_p = other.top_p;
        }
        if self.top_k.is_none() {
            self.top_k = other.top_k;
        }
        if self.repetition_penalty.is_none() {
            self.repetition_penalty = other.repetition_penalty;
        }
        self
    }

    /// `true` if all fields are None, useful to short-circuit.
    pub fn is_empty(&self) -> bool {
        self.temperature.is_none()
            && self.top_p.is_none()
            && self.top_k.is_none()
            && self.repetition_penalty.is_none()
    }
}

// ── Embedded table ─────────────────────────────────────────────────

/// One entry of `embedded.toml`. Matching tests `arch_pattern` (exact or
/// prefix with `*`) and, optionally, `name_pattern` (case-insensitive
/// substring) against the GGUF metadata.
#[derive(Debug, Clone, Deserialize)]
struct EmbeddedEntry {
    /// Human label for logs, e.g. `"Qwen3 (instruct)"`.
    #[allow(dead_code)]
    name: String,
    arch_pattern: String,
    #[serde(default)]
    name_pattern: Option<String>,
    #[serde(default)]
    temperature: Option<f32>,
    #[serde(default)]
    top_p: Option<f32>,
    #[serde(default)]
    top_k: Option<u32>,
    #[serde(default)]
    repetition_penalty: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
struct EmbeddedTable {
    entry: Vec<EmbeddedEntry>,
}

/// Lazy load of the embedded table, parsed once per process.
fn embedded_table() -> &'static [EmbeddedEntry] {
    use std::sync::OnceLock;
    static TABLE: OnceLock<Vec<EmbeddedEntry>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let raw = include_str!("embedded.toml");
        let parsed: EmbeddedTable = toml::from_str(raw)
            .expect("embedded model_defaults.toml must parse — checked at compile time");
        parsed.entry
    })
}

fn arch_matches(pattern: &str, arch: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let arch = arch.to_ascii_lowercase();
    if let Some(prefix) = pattern.strip_suffix('*') {
        arch.starts_with(prefix)
    } else {
        pattern == arch
    }
}

fn name_matches(pattern: Option<&str>, hints: &[&str]) -> bool {
    match pattern {
        None => true,
        Some(p) => {
            let p = p.to_ascii_lowercase();
            hints.iter().any(|h| h.to_ascii_lowercase().contains(&p))
        }
    }
}

fn embedded_lookup(arch: &str, name_hints: &[&str]) -> ModelDefaults {
    for entry in embedded_table() {
        if !arch_matches(&entry.arch_pattern, arch) {
            continue;
        }
        if !name_matches(entry.name_pattern.as_deref(), name_hints) {
            continue;
        }
        return ModelDefaults {
            temperature: entry.temperature,
            top_p: entry.top_p,
            top_k: entry.top_k,
            repetition_penalty: entry.repetition_penalty,
        };
    }
    ModelDefaults::default()
}

// ── User overrides (JSON on disk) ──────────────────────────────────

/// Map persisted to disk: key = GGUF filename *or* HF repo, value = defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserOverrides {
    #[serde(flatten)]
    entries: HashMap<String, ModelDefaults>,
}

impl UserOverrides {
    /// Canonical path of the user-side overrides file.
    /// `~/.apollia/models/sampling-defaults.json`.
    pub fn default_path() -> PathBuf {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir());
        home.join(".apollia")
            .join("models")
            .join("sampling-defaults.json")
    }

    /// Load the file; `Ok(default)` if absent, error if present but unreadable
    /// or corrupt JSON (preferable to silence: the operator wants to know if
    /// their override is ignored).
    pub fn load(path: &Path) -> io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => {
                serde_json::from_str(&s).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Add or replace an entry and persist atomically (write-then-rename).
    pub fn upsert(path: &Path, key: &str, defaults: ModelDefaults) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut current = Self::load(path)?;
        current.entries.insert(key.to_string(), defaults);
        let json = serde_json::to_string_pretty(&current)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Lookup: tries `model_id`, then `file_name`, then each hint.
    pub fn lookup(&self, keys: &[&str]) -> ModelDefaults {
        for k in keys {
            if let Some(d) = self.entries.get(*k) {
                return d.clone();
            }
        }
        ModelDefaults::default()
    }
}

// ── Public resolver ────────────────────────────────────────────────

/// Hints provided by the caller to match a model.
///
/// At least one of [`ModelHints::arch`] and [`ModelHints::name`] must be
/// non-empty for the embedded resolution to have a chance of succeeding.
/// The other fields serve the user override lookup.
#[derive(Debug, Clone, Default)]
pub struct ModelHints<'a> {
    pub arch: Option<&'a str>,
    pub name: Option<&'a str>,
    pub file_name: Option<&'a str>,
    pub repo_id: Option<&'a str>,
    pub model_id: Option<&'a str>,
}

/// Resolve the defaults for a model by combining the three sources.
///
/// The user override wins field by field; the embedded table fills the fields
/// left `None`. If no source provides a field, it stays `None` and the caller
/// applies its global hard fallback.
pub fn resolve(hints: &ModelHints<'_>, overrides: &UserOverrides) -> ModelDefaults {
    let mut keys: Vec<&str> = Vec::new();
    if let Some(s) = hints.repo_id {
        keys.push(s);
    }
    if let Some(s) = hints.model_id {
        keys.push(s);
    }
    if let Some(s) = hints.file_name {
        keys.push(s);
    }
    if let Some(s) = hints.name {
        keys.push(s);
    }
    let user = overrides.lookup(&keys);

    let arch = hints.arch.unwrap_or("");
    let name_hints: Vec<&str> = [hints.name, hints.file_name, hints.model_id]
        .into_iter()
        .flatten()
        .collect();
    let embedded = embedded_lookup(arch, &name_hints);

    user.fill_missing(&embedded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn embedded_table_parses() {
        // GIVEN: the embedded TOML compiled into the binary
        // WHEN: lazily initialised
        let table = embedded_table();
        // THEN: it has at least one entry per major family we ship
        assert!(table.len() >= 5);
    }

    #[test]
    fn arch_pattern_matches_exact() {
        assert!(arch_matches("qwen3", "qwen3"));
        assert!(arch_matches("Qwen3", "qwen3")); // case-insensitive
        assert!(!arch_matches("qwen3", "qwen2"));
    }

    #[test]
    fn arch_pattern_matches_wildcard() {
        assert!(arch_matches("llama*", "llama"));
        assert!(arch_matches("llama*", "llamafoo"));
        assert!(!arch_matches("llama*", "qwen3"));
    }

    #[test]
    fn embedded_lookup_qwen3_thinking_beats_generic() {
        // GIVEN: a Qwen3 thinking model
        let d = embedded_lookup("qwen3", &["Qwen3-30B-A3B-Thinking-2507"]);
        // THEN: the thinking-specific entry wins (temperature 0.6)
        assert_eq!(d.temperature, Some(0.6));
    }

    #[test]
    fn embedded_lookup_qwen3_generic_when_no_specific_match() {
        let d = embedded_lookup("qwen3", &["Qwen3-30B-A3B"]);
        // Generic Qwen3 entry: temperature 0.7
        assert_eq!(d.temperature, Some(0.7));
    }

    #[test]
    fn embedded_lookup_returns_empty_when_no_match() {
        let d = embedded_lookup("unknown-arch", &["whatever"]);
        assert!(d.is_empty());
    }

    #[test]
    fn user_override_persists_round_trip() {
        // GIVEN: a temporary path
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("sampling-defaults.json");

        // WHEN: writing an override
        let d = ModelDefaults {
            temperature: Some(0.42),
            top_p: Some(0.5),
            top_k: None,
            repetition_penalty: None,
        };
        UserOverrides::upsert(&path, "Qwen3-test.gguf", d.clone()).expect("upsert should succeed");

        // THEN: reload returns the same entry
        let reloaded = UserOverrides::load(&path).expect("load");
        let got = reloaded.lookup(&["Qwen3-test.gguf"]);
        assert_eq!(got, d);
    }

    #[test]
    fn user_override_corrupt_file_errors_loudly() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("sampling-defaults.json");
        std::fs::write(&path, "{not valid json").expect("write");
        let err = UserOverrides::load(&path).expect_err("should fail");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn resolve_user_wins_field_by_field_over_embedded() {
        // GIVEN: a Qwen3 model with a partial user override (only temperature)
        let mut overrides = UserOverrides::default();
        overrides.entries.insert(
            "Qwen3-30B-A3B".to_string(),
            ModelDefaults {
                temperature: Some(0.1),
                ..Default::default()
            },
        );

        let hints = ModelHints {
            arch: Some("qwen3"),
            name: Some("Qwen3-30B-A3B"),
            ..Default::default()
        };
        let d = resolve(&hints, &overrides);

        // THEN: temperature comes from user; top_p/top_k from embedded.
        assert_eq!(d.temperature, Some(0.1));
        assert_eq!(d.top_p, Some(0.8)); // embedded Qwen3 generic
        assert_eq!(d.top_k, Some(20)); // embedded Qwen3 generic
    }

    #[test]
    fn resolve_returns_empty_when_no_match_anywhere() {
        let overrides = UserOverrides::default();
        let hints = ModelHints {
            arch: Some("brand-new-arch"),
            name: Some("Unknown"),
            ..Default::default()
        };
        let d = resolve(&hints, &overrides);
        assert!(d.is_empty());
    }
}
