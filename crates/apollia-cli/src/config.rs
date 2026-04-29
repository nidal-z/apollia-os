//! Parsing et validation de la configuration Apollia OS depuis `apollia.toml`.
//!
//! Fournit [`parse_apollia_toml`] pour lire et désérialiser le fichier de config
//! et [`validate_llm_config`] pour une validation non-fatale (warnings seulement)
//! des backends LLM.
//!
//! Les sections opérationnelles (`[[triggers]]`, `[notifications]`, `[stt]`) ne sont
//! plus gérées par le fichier TOML — elles sont désormais stockées en SQLite et
//! administrées via l'API REST ou l'application desktop. Les sections `[pipelines]`
//! et `[[pipelines]]` sont obsolètes — l'engine pipelines a été retiré.
//! Si un ancien fichier TOML contient ces sections, un warning est émis mais le boot
//! continue normalement.
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

use apollia_core::{
    A2AConfig, ApiConfig, FilesystemConfig, HitlConfig, McpConfig, ORIAConfig, PermissionsConfig,
    RegistryConfig, RuntimeConfig, ToolsConfig,
};
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

/// Configuration globale Apollia OS validée depuis `apollia.toml`.
///
/// Contient la configuration LLM, l'API REST, le runtime core, le HITL et le routing A2A.
/// La configuration opérationnelle (triggers, notifications, agents, stt)
/// est gérée en SQLite.
///
/// Pour désérialiser depuis un fichier, utiliser [`parse_apollia_toml`].
#[derive(Debug, serde::Deserialize)]
pub struct ApolliaCConfig {
    /// Section `[llm]` — configuration des backends LLM.
    ///
    /// Vaut `None` si la section `[llm]` est absente du fichier.
    pub llm: Option<LlmConfig>,

    /// Section `[api]` — configuration du listener TCP et de l'authentification.
    ///
    /// Vaut `None` si la section `[api]` est absente du fichier ; les valeurs
    /// par défaut de [`ApiConfig`] sont alors appliquées.
    pub api: Option<ApiConfig>,

    /// Section `[runtime]` — capacités EventBus et mailbox.
    ///
    /// Vaut `None` si absente ; les valeurs par défaut de [`RuntimeConfig`] s'appliquent.
    pub runtime: Option<RuntimeConfig>,

    /// Section `[hitl]` — timeout et scan interval Human-in-the-Loop.
    ///
    /// Vaut `None` si absente ; les valeurs par défaut de [`HitlConfig`] s'appliquent.
    pub hitl: Option<HitlConfig>,

    /// Section `[a2a]` — routing inter-agents.
    ///
    /// Vaut `None` si absente ; les valeurs par défaut de [`A2AConfig`] s'appliquent.
    pub a2a: Option<A2AConfig>,

    /// Section `[oria]` — moteur Observer-Reasoner-Actor.
    ///
    /// Vaut `None` si absente ; les valeurs par défaut de [`ORIAConfig`] s'appliquent.
    pub oria: Option<ORIAConfig>,

    /// Section `[registry]` — URL du registry communautaire.
    ///
    /// Vaut `None` si absente ; la valeur par défaut de [`RegistryConfig`] s'applique.
    pub registry: Option<RegistryConfig>,

    /// Section `[tools]` — outils natifs : limites, désactivations statiques,
    /// configuration `web_search` / `web_read`.
    ///
    /// Vaut `None` si absente ; les valeurs par défaut de [`ToolsConfig`] s'appliquent.
    pub tools: Option<ToolsConfig>,

    /// Section `[mcp]` — configuration du module MCP (TTL des approbations HITL).
    ///
    /// Vaut `None` si absente ; les valeurs par défaut de [`McpConfig`] s'appliquent.
    pub mcp: Option<McpConfig>,

    /// Section `[permissions]` — moteur de permissions (SafeList, détection d'injection).
    ///
    /// Vaut `None` si absente ; les valeurs par défaut de [`PermissionsConfig`] s'appliquent.
    pub permissions: Option<PermissionsConfig>,

    /// Section `[filesystem]` — journal réversible et configuration filesystem.
    ///
    /// Vaut `None` si absente ; les valeurs par défaut de [`FilesystemConfig`] s'appliquent.
    pub filesystem: Option<FilesystemConfig>,
}

/// Noms des sections TOML qui sont désormais obsolètes.
///
/// Utilisé par [`check_deprecated_sections`] pour émettre des warnings
/// si un ancien fichier `apollia.toml` contient encore ces sections.
const DEPRECATED_SECTIONS: &[&str] = &["triggers", "notifications", "stt", "pipelines"];

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
/// Après le parsing TOML :
/// - Les chemins `model_path` des backends embarqués sont normalisés via [`expand_tilde`].
/// - Les sections obsolètes (`[[triggers]]`, `[pipelines]` / `[[pipelines]]`,
///   `[notifications]`, `[stt]`) sont détectées et émettent un warning sans bloquer
///   le démarrage.
///
/// La section `[llm]` est **optionnelle** : son absence produit `config.llm = None`
/// sans erreur.
///
/// # Erreurs
///
/// - [`ConfigError::Io`] — le fichier est inaccessible ou illisible.
/// - [`ConfigError::Parse`] — le TOML est malformé ou contient des types invalides.
pub fn parse_apollia_toml(path: &Path) -> Result<ApolliaCConfig, ConfigError> {
    let content = std::fs::read_to_string(path)?;

    // Check for deprecated sections before strict deserialization.
    check_deprecated_sections(&content);

    // Parse as a loose table first to ignore unknown sections gracefully.
    let raw_table: toml::Value = toml::from_str(&content)?;

    // Build a filtered table with only known sections.
    let mut filtered = toml::map::Map::new();
    if let toml::Value::Table(table) = &raw_table {
        for key in &[
            "llm",
            "runtime",
            "memory",
            "tools",
            "budget",
            "api",
            "hitl",
            "a2a",
            "oria",
            "registry",
            "mcp",
            "permissions",
            "filesystem",
        ] {
            if let Some(v) = table.get(*key) {
                filtered.insert((*key).to_string(), v.clone());
            }
        }
    }

    let mut config: ApolliaCConfig = toml::Value::Table(filtered)
        .try_into()
        .map_err(|e: toml::de::Error| e)?;

    // Normalise les chemins des backends embarqués (~ → $HOME) — `model_path`
    // mono-fichier/split standard et chaque entrée de `model_paths` custom.
    if let Some(ref mut llm) = config.llm {
        for _backend in &mut llm.backends {
            #[cfg(feature = "local")]
            if let BackendKind::Embedded(ref mut cfg) = _backend.kind {
                if let Some(path) = cfg.model_path.as_mut() {
                    *path = expand_tilde(path);
                }
                if let Some(paths) = cfg.model_paths.as_mut() {
                    for p in paths.iter_mut() {
                        *p = expand_tilde(p);
                    }
                }
            }
        }
    }

    Ok(config)
}

/// Détecte et signale les sections TOML obsolètes.
///
/// Les sections `[[triggers]]`, `[notifications]`, `[stt]`, `[pipelines]` /
/// `[[pipelines]]` sont obsolètes. Si le fichier TOML les contient encore, un
/// warning est émis pour chaque section détectée.
fn check_deprecated_sections(content: &str) {
    for section in DEPRECATED_SECTIONS {
        let bracket_single = format!("[{section}]");
        let bracket_double = format!("[[{section}]]");
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed == bracket_single
                || trimmed == bracket_double
                || trimmed.starts_with(&format!("[{section}."))
            {
                tracing::warn!(
                    section = %section,
                    "apollia.toml contains deprecated section [{section}], \
                     use the desktop app or API to manage {section}"
                );
                break;
            }
        }
    }
}

/// Validation non-fatale de la config LLM — émet des warnings, ne retourne jamais d'erreur.
///
/// Pour chaque backend configuré :
/// - **Backend embarqué** : vérifie que le fichier `.gguf` existe après expansion du `~`.
///   Si absent → `tracing::warn!` avec le chemin manquant (backend ignoré par le router).
/// - **Backend API** : vérifie que la variable d'environnement `api_key_env` est définie.
///   Si absente → `tracing::warn!` avec le nom de la variable (backend ignoré par le router).
///
/// Cette fonction est **intentionnellement non-fatale** : la validation stricte
/// est déléguée à [`apollia_llm::LlmRouter::from_config`] au démarrage du Supervisor.
///
/// Retourne toujours `Ok(())`.
pub fn validate_llm_config(config: &LlmConfig) -> Result<(), ConfigError> {
    for backend in &config.backends {
        match &backend.kind {
            #[cfg(feature = "local")]
            BackendKind::Embedded(cfg) => {
                if let Some(path) = cfg.model_path.as_ref() {
                    let expanded = expand_tilde(path);
                    if !expanded.exists() {
                        tracing::warn!(
                            backend = %backend.name(),
                            path = %expanded.display(),
                            "model file not found — backend will be skipped"
                        );
                    }
                }
                if let Some(paths) = cfg.model_paths.as_ref() {
                    for p in paths {
                        let expanded = expand_tilde(p);
                        if !expanded.exists() {
                            tracing::warn!(
                                backend = %backend.name(),
                                path = %expanded.display(),
                                "shard file not found — backend will be skipped"
                            );
                        }
                    }
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

    /// Écrit le contenu dans un fichier temporaire et retourne le handle.
    fn write_toml(content: &str) -> tempfile::NamedTempFile {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().expect("failed to create temp file");
        write!(f, "{content}").expect("failed to write temp file");
        f
    }

    // GIVEN un TOML avec seulement [agents] et [llm]
    // WHEN parse_apollia_toml est appelé
    // THEN config est correctement parsée sans erreur
    #[cfg(feature = "local")]
    #[test]
    fn test_parse_structural_config_only() {
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
        let file = write_toml(toml_str);

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN
        let llm = config.llm.expect("llm should be present");
        assert_eq!(llm.default, "local");
        assert_eq!(llm.backends[0].name(), "local");
        assert!(
            matches!(llm.backends[0].kind, BackendKind::Embedded(_)),
            "kind should be Embedded"
        );
    }

    // GIVEN un TOML avec [[llm.backends]] type = "api"
    // WHEN on désérialise
    // THEN config.llm.backends[0] est Api
    #[cfg(feature = "cloud")]
    #[test]
    fn test_parse_api_backend() {
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
        let file = write_toml(toml_str);

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN
        let llm = config.llm.expect("llm should be present");
        assert_eq!(llm.backends[0].name(), "anthropic");
        assert!(
            matches!(llm.backends[0].kind, BackendKind::Api(_)),
            "kind should be Api"
        );
    }

    // GIVEN un TOML sans section [llm]
    // WHEN on parse
    // THEN config.llm est None (pas d'erreur)
    #[test]
    fn test_no_llm_section_is_none() {
        // GIVEN
        let file = write_toml("");

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN
        assert!(
            config.llm.is_none(),
            "llm should be None when section absent"
        );
    }

    // GIVEN une LlmConfig minimale (cloud backend sans clé API)
    // WHEN on appelle validate_llm_config
    // THEN Ok(()) est retourné — jamais d'erreur fatale
    #[cfg(feature = "cloud")]
    #[test]
    fn test_validate_llm_config_always_returns_ok() {
        // GIVEN
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
        let file = write_toml(toml_str);
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");
        let llm = config.llm.expect("llm should be present");

        // WHEN
        let result = validate_llm_config(&llm);

        // THEN
        assert!(
            result.is_ok(),
            "validate_llm_config should always return Ok"
        );
    }

    // GIVEN un TOML avec [llm] mais sans [llm.observability]
    // WHEN on désérialise
    // THEN les valeurs par défaut sont appliquées
    #[cfg(feature = "cloud")]
    #[test]
    fn test_observability_defaults() {
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
        let file = write_toml(toml_str);

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN
        let obs = &config.llm.expect("llm should be present").observability;
        assert!(
            obs.log_token_usage,
            "log_token_usage should default to true"
        );
        assert!(obs.log_latency, "log_latency should default to true");
        assert!(
            !obs.debug_log_prompt,
            "debug_log_prompt should default to false"
        );
    }

    // GIVEN un chemin commençant par ~/
    // WHEN on appelle expand_tilde
    // THEN le ~ est remplacé
    #[test]
    fn test_expand_tilde_replaces_home() {
        // GIVEN
        let path = Path::new("~/.apollia/models/test.gguf");

        // WHEN
        let expanded = expand_tilde(path);

        // THEN
        let s = expanded.to_string_lossy();
        assert!(!s.contains('~'), "~ should be replaced, got: {s}");
        assert!(s.contains(".apollia"), "path should contain .apollia");
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

    // ─── Deprecated sections warning ─────────────────────────────

    // GIVEN un TOML contenant une section [[triggers]]
    // WHEN parse_apollia_toml est appelé
    // THEN le parsing réussit (pas d'erreur) — les sections obsolètes sont ignorées
    #[test]
    fn test_deprecated_triggers_section_does_not_block_parsing() {
        // GIVEN — TOML with deprecated [[triggers]] section
        let toml = r#"
[agents]
directory = "agents/"

[[triggers]]
id             = "old-trigger"
agent          = "some-agent"
enabled        = true
on_busy        = "queue"
input_template = "test"

[triggers.source]
type     = "cron"
schedule = "* * * * *"
"#;
        let file = write_toml(toml);

        // WHEN
        let result = parse_apollia_toml(file.path());

        // THEN — parsing succeeds, deprecated sections are silently ignored
        assert!(
            result.is_ok(),
            "parsing should succeed despite deprecated sections, error: {:?}",
            result.err()
        );
    }

    // GIVEN un TOML contenant [notifications]
    // WHEN parse_apollia_toml est appelé
    // THEN le parsing réussit — la section est ignorée
    #[test]
    fn test_deprecated_notifications_section_does_not_block_parsing() {
        // GIVEN
        let toml = r#"
[agents]
directory = "agents/"

[notifications]
events = ["task.completed"]
"#;
        let file = write_toml(toml);

        // WHEN
        let result = parse_apollia_toml(file.path());

        // THEN
        assert!(
            result.is_ok(),
            "parsing should succeed despite deprecated [notifications], error: {:?}",
            result.err()
        );
    }

    // GIVEN un TOML vide
    // WHEN parse_apollia_toml est appelé
    // THEN le parsing réussit avec la section llm à None
    #[test]
    fn test_empty_toml_parses_ok() {
        // GIVEN
        let file = write_toml("");

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("empty TOML should parse");

        // THEN
        assert!(config.llm.is_none());
    }

    // GIVEN la constante DEPRECATED_SECTIONS
    // WHEN on l'inspecte
    // THEN elle contient triggers, notifications, stt et pipelines (engine retiré)
    #[test]
    fn test_deprecated_sections_constant() {
        assert!(DEPRECATED_SECTIONS.contains(&"triggers"));
        assert!(DEPRECATED_SECTIONS.contains(&"notifications"));
        assert!(DEPRECATED_SECTIONS.contains(&"stt"));
        assert!(DEPRECATED_SECTIONS.contains(&"pipelines"));
    }

    // GIVEN un TOML qui contient [[pipelines]] obsolète (format tableau)
    // WHEN parse_apollia_toml est appelé
    // THEN le parsing réussit — le tableau [[pipelines]] est silencieusement ignoré.
    #[test]
    fn test_deprecated_pipelines_array_section_does_not_block_parsing() {
        // GIVEN — [[pipelines]] array is the old pipeline-definitions format
        let toml = r#"
[[pipelines]]
id          = "old-pipeline"
description = "obsolete"

[[pipelines.steps]]
id    = "step-1"
agent = "a"
input = "x"
"#;
        let file = write_toml(toml);

        // WHEN
        let result = parse_apollia_toml(file.path());

        // THEN — array format is silently skipped; parsing succeeds
        assert!(
            result.is_ok(),
            "parsing should succeed — [[pipelines]] array is ignored, error: {:?}",
            result.err()
        );
    }

    // GIVEN apollia.toml contient [mcp], [permissions] et [filesystem.journal]
    // WHEN parse_apollia_toml est appelé
    // THEN les valeurs custom sont désérialisées correctement
    #[test]
    fn test_mcp_permissions_filesystem_sections_deserialized() {
        // GIVEN
        let toml = r#"
[mcp]
approval_ttl_hours = 48

[permissions]
injection_detection = false
safe_commands = ["bash_executor(git status)"]

[filesystem.journal]
max_sessions = 100
"#;
        let file = write_toml(toml);

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN
        let mcp = config.mcp.expect("mcp should be present");
        assert_eq!(mcp.approval_ttl_hours, 48);

        let perms = config.permissions.expect("permissions should be present");
        assert!(!perms.injection_detection);
        assert_eq!(perms.safe_commands, vec!["bash_executor(git status)"]);

        let fs = config.filesystem.expect("filesystem should be present");
        assert_eq!(fs.journal.max_sessions, 100);
    }

    // GIVEN apollia.toml sans [mcp]/[permissions]/[filesystem]
    // WHEN parse_apollia_toml est appelé
    // THEN les champs sont None — pas de régression
    #[test]
    fn test_mcp_permissions_filesystem_absent_is_none() {
        // GIVEN — empty TOML
        let file = write_toml("");

        // WHEN
        let config = parse_apollia_toml(file.path()).expect("parse should succeed");

        // THEN
        assert!(config.mcp.is_none());
        assert!(config.permissions.is_none());
        assert!(config.filesystem.is_none());
    }

    // GIVEN le struct ApolliaCConfig
    // WHEN on vérifie sa structure
    // THEN il contient les champs config statique (llm, api, runtime, hitl, a2a, oria, registry, tools, mcp, permissions, filesystem)
    #[test]
    fn test_config_struct_has_expected_fields() {
        let config = ApolliaCConfig {
            llm: None,
            api: None,
            runtime: None,
            hitl: None,
            a2a: None,
            oria: None,
            registry: None,
            tools: None,
            mcp: None,
            permissions: None,
            filesystem: None,
        };
        assert!(config.llm.is_none());
        assert!(config.runtime.is_none());
        assert!(config.hitl.is_none());
        assert!(config.a2a.is_none());
        assert!(config.registry.is_none());
        assert!(config.mcp.is_none());
        assert!(config.permissions.is_none());
        assert!(config.filesystem.is_none());
    }
}
