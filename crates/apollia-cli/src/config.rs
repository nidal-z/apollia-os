//! Parsing et validation de la configuration Apollia OS depuis `apollia.toml`.
//!
//! Fournit [`parse_apollia_toml`] pour lire et désérialiser le fichier de config,
//! et [`validate_llm_config`] pour une validation non-fatale (warnings seulement)
//! des backends LLM avant de passer la config au Supervisor.
//!
//! # Exemple TOML minimal
//!
//! ```toml
//! [llm]
//! default = "anthropic"
//!
//! [[llm.backends]]
//! name        = "anthropic"
//! type        = "api"
//! api_url     = "https://api.anthropic.com"
//! api_key_env = "ANTHROPIC_API_KEY"
//! model       = "claude-haiku-4-5-20251001"
//! ```

use std::path::{Path, PathBuf};

use apollia_llm::{BackendKind, LlmConfig};

// ─────────────────────────────────────────────
// Erreurs
// ─────────────────────────────────────────────

/// Erreurs possibles lors du parsing de `apollia.toml`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Erreur I/O lors de la lecture du fichier de configuration.
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    /// Erreur de désérialisation TOML (champ invalide, type incorrect, etc.).
    #[error("invalid TOML config: {0}")]
    Parse(#[from] toml::de::Error),
}

// ─────────────────────────────────────────────
// Structures de configuration
// ─────────────────────────────────────────────

/// Configuration de la section `[agents]` dans `apollia.toml`.
#[derive(Debug, serde::Deserialize)]
pub struct AgentsConfig {
    /// Répertoire de chargement des modules Python agents.
    pub directory: Option<String>,
}

/// Configuration globale Apollia OS désérialisée depuis `apollia.toml`.
///
/// Tous les champs sont optionnels : un fichier minimal peut ne contenir
/// qu'une seule section ou être entièrement vide.
#[derive(Debug, serde::Deserialize)]
pub struct ApolliaCConfig {
    /// Section `[agents]` — répertoire des agents Python.
    pub agents: Option<AgentsConfig>,

    /// Section `[llm]` — configuration des backends LLM.
    ///
    /// Vaut `None` si la section `[llm]` est absente du fichier (AC-3).
    pub llm: Option<LlmConfig>,
}

// ─────────────────────────────────────────────
// Fonctions publiques
// ─────────────────────────────────────────────

/// Résout `~` vers `$HOME` dans un chemin de fichier.
///
/// Remplace un composant `~` en tête de chemin par la valeur de la
/// variable d'environnement `HOME`. Si `HOME` n'est pas définie,
/// `~` est remplacé par une chaîne vide (comportement non-fatal).
///
/// Cette fonction est **pure** — aucun accès au système de fichiers,
/// uniquement une transformation de chaîne.
pub fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(format!("{}{}", home, &s[1..]))
    } else if s == "~" {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home)
    } else {
        path.to_path_buf()
    }
}

/// Lit et désérialise `apollia.toml` depuis le chemin indiqué.
///
/// Après le parsing TOML, les chemins `model_path` des backends embarqués
/// sont normalisés via [`expand_tilde`] (`~` → `$HOME`).
///
/// La section `[llm]` est **optionnelle** : son absence produit `config.llm = None`
/// sans erreur (AC-3).
///
/// # Erreurs
///
/// - [`ConfigError::Io`] — le fichier est inaccessible ou illisible.
/// - [`ConfigError::Parse`] — le TOML est malformé ou contient des types invalides.
pub fn parse_apollia_toml(path: &Path) -> Result<ApolliaCConfig, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    let mut config: ApolliaCConfig = toml::from_str(&content)?;

    // Normalise les chemins model_path des backends embarqués (~ → $HOME).
    if let Some(ref mut llm) = config.llm {
        for _backend in &mut llm.backends {
            #[cfg(feature = "local")]
            if let BackendKind::Embedded(ref mut cfg) = _backend.kind {
                cfg.model_path = expand_tilde(&cfg.model_path);
            }
        }
    }

    Ok(config)
}

/// Validation non-fatale de la config LLM — émet des warnings, ne retourne jamais d'erreur.
///
/// Pour chaque backend configuré :
/// - **Backend embarqué** : vérifie que le fichier `.gguf` existe après expansion du `~`.
///   Si absent → `tracing::warn!` avec le chemin manquant (backend ignoré par le router).
/// - **Backend API** : vérifie que la variable d'environnement `api_key_env` est définie.
///   Si absente → `tracing::warn!` avec le nom de la variable (backend ignoré par le router).
///
/// Cette fonction est **intentionnellement non-fatale** (AC-4) : la validation stricte
/// est déléguée à [`apollia_llm::LlmRouter::from_config`] au démarrage du Supervisor.
///
/// Retourne toujours `Ok(())`.
pub fn validate_llm_config(config: &LlmConfig) -> Result<(), ConfigError> {
    for backend in &config.backends {
        match &backend.kind {
            #[cfg(feature = "local")]
            BackendKind::Embedded(cfg) => {
                let expanded = expand_tilde(&cfg.model_path);
                if !expanded.exists() {
                    tracing::warn!(
                        backend = %backend.name(),
                        path = %expanded.display(),
                        "model file not found — backend will be skipped"
                    );
                }
            }

            #[cfg(feature = "cloud")]
            BackendKind::Api(cfg) => {
                if std::env::var(&cfg.api_key_env).is_err() {
                    tracing::warn!(
                        backend = %backend.name(),
                        env_var = %cfg.api_key_env,
                        "API key env var not set — backend will be skipped"
                    );
                }
            }
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN un TOML avec [[llm.backends]] type = "embedded"
    // WHEN on désérialise en ApolliaCConfig
    // THEN config.llm.default == "local" ET backends[0] est Embedded
    #[cfg(feature = "local")]
    #[test]
    fn test_ac1_parse_embedded_backend() {
        // GIVEN
        let toml_str = r#"
[llm]
default = "local"

[[llm.backends]]
name = "local"
type = "embedded"
model_path = "/tmp/model.gguf"
quantization = "q4_k_m"
"#;
        // WHEN
        let config: ApolliaCConfig = toml::from_str(toml_str).unwrap();

        // THEN
        let llm = config.llm.unwrap();
        assert_eq!(llm.default, "local");
        assert_eq!(llm.backends[0].name(), "local");
        assert!(
            matches!(llm.backends[0].kind, BackendKind::Embedded(_)),
            "le kind doit être Embedded"
        );
    }

    // GIVEN un TOML avec [[llm.backends]] type = "api"
    // WHEN on désérialise en ApolliaCConfig
    // THEN config.llm.backends[0] est Api et le nom est correct
    #[cfg(feature = "cloud")]
    #[test]
    fn test_ac2_parse_api_backend() {
        // GIVEN
        let toml_str = r#"
[llm]
default = "anthropic"

[[llm.backends]]
name = "anthropic"
type = "api"
api_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"
model = "claude-haiku-4-5-20251001"
"#;
        // WHEN
        let config: ApolliaCConfig = toml::from_str(toml_str).unwrap();

        // THEN
        let llm = config.llm.unwrap();
        assert_eq!(llm.backends[0].name(), "anthropic");
        assert!(
            matches!(llm.backends[0].kind, BackendKind::Api(_)),
            "le kind doit être Api"
        );
    }

    // GIVEN un TOML sans section [llm]
    // WHEN on désérialise en ApolliaCConfig
    // THEN config.llm est None (pas d'erreur)
    #[test]
    fn test_ac3_no_llm_section_is_none() {
        // GIVEN
        let toml_str = r#"
[agents]
directory = "agents/"
"#;
        // WHEN
        let config: ApolliaCConfig = toml::from_str(toml_str).unwrap();

        // THEN
        assert!(
            config.llm.is_none(),
            "llm doit être None si section absente"
        );
    }

    // GIVEN une LlmConfig minimale (cloud, cloud backend sans clé API)
    // WHEN on appelle validate_llm_config
    // THEN Ok(()) est retourné — jamais d'erreur fatale (AC-4)
    #[cfg(feature = "cloud")]
    #[test]
    fn test_ac4_validate_llm_config_always_returns_ok() {
        // GIVEN — un backend dont la clé API est probablement absente en CI
        let toml_str = r#"
[llm]
default = "test-api"

[[llm.backends]]
name = "test-api"
type = "api"
api_url = "https://api.openai.com/v1"
api_key_env = "APOLLIA_NONEXISTENT_KEY_FOR_TEST_XYZ"
model = "gpt-4o-mini"
"#;
        let config: ApolliaCConfig = toml::from_str(toml_str).unwrap();
        let llm = config.llm.unwrap();

        // WHEN
        let result = validate_llm_config(&llm);

        // THEN — toujours Ok, même si la clé est absente
        assert!(
            result.is_ok(),
            "validate_llm_config doit retourner Ok même avec une clé API manquante"
        );
    }

    // GIVEN un TOML avec [llm] mais sans [llm.observability]
    // WHEN on désérialise
    // THEN les valeurs par défaut sont log_token_usage=true, log_latency=true, debug_log_prompt=false
    #[cfg(feature = "cloud")]
    #[test]
    fn test_ac5_observability_defaults() {
        // GIVEN — section [llm.observability] absente
        let toml_str = r#"
[llm]
default = "anthropic"

[[llm.backends]]
name = "anthropic"
type = "api"
api_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"
model = "claude-haiku-4-5-20251001"
"#;
        // WHEN
        let config: ApolliaCConfig = toml::from_str(toml_str).unwrap();

        // THEN
        let obs = &config.llm.unwrap().observability;
        assert!(
            obs.log_token_usage,
            "log_token_usage doit valoir true par défaut"
        );
        assert!(obs.log_latency, "log_latency doit valoir true par défaut");
        assert!(
            !obs.debug_log_prompt,
            "debug_log_prompt doit valoir false par défaut"
        );
    }

    // GIVEN un chemin commençant par ~/
    // WHEN on appelle expand_tilde
    // THEN le ~ est remplacé et le chemin ne contient plus ~
    #[test]
    fn test_expand_tilde_replaces_home() {
        // GIVEN
        let path = Path::new("~/.apollia/models/test.gguf");

        // WHEN
        let expanded = expand_tilde(path);

        // THEN
        let s = expanded.to_string_lossy();
        assert!(!s.contains('~'), "le ~ doit être remplacé, obtenu : {s}");
        assert!(s.contains(".apollia"), "le chemin doit contenir .apollia");
    }

    // GIVEN un chemin sans ~ (chemin absolu)
    // WHEN on appelle expand_tilde
    // THEN le chemin est retourné inchangé
    #[test]
    fn test_expand_tilde_noop_for_absolute_path() {
        // GIVEN
        let path = Path::new("/tmp/model.gguf");

        // WHEN
        let expanded = expand_tilde(path);

        // THEN
        assert_eq!(expanded, PathBuf::from("/tmp/model.gguf"));
    }
}
