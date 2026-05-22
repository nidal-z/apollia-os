//! Sampling defaults par famille de modèle.
//!
//! Trois sources, par précédence décroissante (la première qui répond
//! gagne sur les suivantes, champ par champ) :
//!
//! 1. **Override utilisateur** — `~/.apollia/models/sampling-defaults.json`,
//!    rempli automatiquement quand un modèle est téléchargé via Apollia
//!    depuis HuggingFace (`generation_config.json` du dépôt source) ou
//!    édité manuellement par l'opérateur.
//! 2. **Table curated embarquée** — `embedded.toml` shipped dans le
//!    binaire, ~10 familles populaires. Source de fallback offline.
//! 3. **Aucun match** — le caller utilise ses propres défauts globaux
//!    (typiquement 0.7 / 0.95 / 40).
//!
//! Le caller peut toujours surcharger via `CompletionRequest.temperature` ;
//! ce module ne fait que résoudre les *defaults* — pas de policy.
//!
//! Légalité : les valeurs publiées dans les `generation_config.json` HF
//! sont des paramètres numériques recommandés par les éditeurs (faits non
//! copyrightables au sens de Feist v. Rural). La table embarquée cite la
//! source pour chaque entrée. Le modèle lui-même reste sous sa licence
//! (Llama, Qwen, Gemma…) — ce module n'en redistribue rien.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Paramètres de sampling par défaut pour un modèle donné. Tous les champs
/// sont optionnels — un modèle peut n'avoir qu'une recommandation pour
/// `temperature` et laisser le reste aux défauts globaux.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelDefaults {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<u32>,
    pub repetition_penalty: Option<f32>,
}

impl ModelDefaults {
    /// Fusionne `other` dans `self` SANS écraser les champs déjà présents.
    /// Utilisé pour superposer les sources (override > embedded).
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

    /// `true` si tous les champs sont None — utile pour court-circuiter.
    pub fn is_empty(&self) -> bool {
        self.temperature.is_none()
            && self.top_p.is_none()
            && self.top_k.is_none()
            && self.repetition_penalty.is_none()
    }
}

// ── Table embarquée ────────────────────────────────────────────────

/// Une entrée du `embedded.toml`. Le matching teste `arch_pattern`
/// (exact ou prefix avec `*`) et, optionnellement, `name_pattern`
/// (substring case-insensitive) contre les métadonnées GGUF.
#[derive(Debug, Clone, Deserialize)]
struct EmbeddedEntry {
    /// Étiquette humaine pour les logs — ex. `"Qwen3 (instruct)"`.
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

/// Chargement paresseux de la table embarquée — parsé une fois par process.
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

// ── User overrides (JSON sur disque) ───────────────────────────────

/// Map persistée sur disque : clé = filename GGUF *ou* repo HF, valeur = defaults.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserOverrides {
    #[serde(flatten)]
    entries: HashMap<String, ModelDefaults>,
}

impl UserOverrides {
    /// Chemin canonique du fichier d'overrides côté utilisateur.
    /// `~/.apollia/models/sampling-defaults.json`.
    pub fn default_path() -> PathBuf {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir());
        home.join(".apollia")
            .join("models")
            .join("sampling-defaults.json")
    }

    /// Charge le fichier ; `Ok(default)` si absent, erreur si présent mais
    /// illisible ou JSON corrompu (préférable au silence — l'opérateur veut
    /// savoir si son override est ignoré).
    pub fn load(path: &Path) -> io::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => {
                serde_json::from_str(&s).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    /// Ajoute/remplace une entrée et persiste atomiquement (write-then-rename).
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

    /// Lookup : essaie `model_id`, puis `file_name`, puis chaque hint.
    pub fn lookup(&self, keys: &[&str]) -> ModelDefaults {
        for k in keys {
            if let Some(d) = self.entries.get(*k) {
                return d.clone();
            }
        }
        ModelDefaults::default()
    }
}

// ── Resolver public ────────────────────────────────────────────────

/// Indices fournis par l'appelant pour matcher un modèle.
///
/// Au moins un de [`ModelHints::arch`] et [`ModelHints::name`] doit être
/// non-vide pour que la résolution embedded ait une chance d'aboutir.
/// Les autres champs servent au lookup user override.
#[derive(Debug, Clone, Default)]
pub struct ModelHints<'a> {
    pub arch: Option<&'a str>,
    pub name: Option<&'a str>,
    pub file_name: Option<&'a str>,
    pub repo_id: Option<&'a str>,
    pub model_id: Option<&'a str>,
}

/// Résout les defaults pour un modèle en combinant les trois sources.
///
/// L'override utilisateur l'emporte champ par champ ; la table embarquée
/// remplit les champs restés `None`. Si aucune source ne renseigne un
/// champ, il reste `None` et le caller appliquera son hard-fallback global.
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
