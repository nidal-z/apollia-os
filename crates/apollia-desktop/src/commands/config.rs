//! Commandes IPC Tauri pour la vue Settings (STORY-149).
//!
//! La vue Settings est **lecture seule** (ADR-029) : le round-trip
//! TOML parse -> modifier -> sérialiser détruirait les commentaires
//! utilisateur. L'édition est déléguée à l'éditeur natif du système.
//!
//! Le fichier `apollia.toml` est lu depuis `~/.apollia/apollia.toml`
//! (emplacement standard). Si le fichier n'existe pas, des valeurs
//! par défaut sont retournées.

use std::path::PathBuf;

use serde::Serialize;

/// Entrée clé/valeur d'une section de configuration.
#[derive(Debug, Serialize)]
pub struct ConfigEntry {
    /// Nom de la clé.
    pub key: String,
    /// Valeur sous forme de chaîne lisible.
    pub value: String,
}

/// Section de configuration regroupée par thème.
#[derive(Debug, Serialize)]
pub struct ConfigSection {
    /// Nom de la section (ex: `"runtime"`, `"oria"`).
    pub name: String,
    /// Description courte de la section.
    pub description: String,
    /// Entrées clé/valeur.
    pub entries: Vec<ConfigEntry>,
    /// Si la section redirige vers une vue dédiée au lieu d'afficher inline.
    pub redirect_route: Option<String>,
}

/// Vue plate de la configuration Apollia OS pour l'UI.
#[derive(Debug, Serialize)]
pub struct ApollaConfigView {
    /// Chemin absolu vers le fichier `apollia.toml`.
    pub config_path: String,
    /// Indique si le fichier existe sur le disque.
    pub config_exists: bool,
    /// Sections de configuration.
    pub sections: Vec<ConfigSection>,
}

/// Résout le chemin standard `~/.apollia/apollia.toml`.
fn default_config_path() -> PathBuf {
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    home.join(".apollia").join("apollia.toml")
}

/// Extrait une valeur string d'une table TOML, ou retourne un défaut.
fn toml_string(table: &toml::Value, section: &str, key: &str, default: &str) -> String {
    table
        .get(section)
        .and_then(|s| s.get(key))
        .map(|v| match v {
            toml::Value::String(s) => s.clone(),
            toml::Value::Integer(n) => n.to_string(),
            toml::Value::Float(f) => f.to_string(),
            toml::Value::Boolean(b) => b.to_string(),
            other => other.to_string(),
        })
        .unwrap_or_else(|| default.to_string())
}

/// Construit la vue de configuration à partir du TOML parsé (ou des défauts).
fn build_config_view(
    toml_value: Option<&toml::Value>,
    config_path: &str,
    exists: bool,
) -> ApollaConfigView {
    let sections = vec![
        ConfigSection {
            name: "runtime".to_string(),
            description: "Runtime core settings".to_string(),
            entries: vec![
                ConfigEntry {
                    key: "socket_path".to_string(),
                    value: toml_value
                        .map(|t| toml_string(t, "runtime", "socket_path", "/tmp/apollia.sock"))
                        .unwrap_or_else(|| "/tmp/apollia.sock".to_string()),
                },
                ConfigEntry {
                    key: "api_port".to_string(),
                    value: toml_value
                        .map(|t| toml_string(t, "runtime", "api_port", "7771"))
                        .unwrap_or_else(|| "7771".to_string()),
                },
                ConfigEntry {
                    key: "max_concurrent_agents".to_string(),
                    value: toml_value
                        .map(|t| toml_string(t, "runtime", "max_concurrent_agents", "10"))
                        .unwrap_or_else(|| "10".to_string()),
                },
            ],
            redirect_route: None,
        },
        ConfigSection {
            name: "oria".to_string(),
            description: "ORIA engine (Observer-Reasoner-Actor)".to_string(),
            entries: vec![
                ConfigEntry {
                    key: "max_steps".to_string(),
                    value: toml_value
                        .map(|t| toml_string(t, "oria", "max_steps", "50"))
                        .unwrap_or_else(|| "50".to_string()),
                },
                ConfigEntry {
                    key: "wall_clock_timeout".to_string(),
                    value: toml_value
                        .map(|t| toml_string(t, "oria", "wall_clock_timeout", "300s"))
                        .unwrap_or_else(|| "300s".to_string()),
                },
            ],
            redirect_route: None,
        },
        ConfigSection {
            name: "observability".to_string(),
            description: "Observability and logging limits".to_string(),
            entries: vec![
                ConfigEntry {
                    key: "max_input_bytes".to_string(),
                    value: toml_value
                        .map(|t| toml_string(t, "observability", "max_input_bytes", "32768"))
                        .unwrap_or_else(|| "32768".to_string()),
                },
                ConfigEntry {
                    key: "debug_log_prompt".to_string(),
                    value: toml_value
                        .map(|t| toml_string(t, "observability", "debug_log_prompt", "false"))
                        .unwrap_or_else(|| "false".to_string()),
                },
            ],
            redirect_route: None,
        },
        ConfigSection {
            name: "memory".to_string(),
            description: "Memory engine settings".to_string(),
            entries: vec![ConfigEntry {
                key: "episodic_ttl_days".to_string(),
                value: toml_value
                    .map(|t| toml_string(t, "memory", "episodic_ttl_days", "30"))
                    .unwrap_or_else(|| "30".to_string()),
            }],
            redirect_route: None,
        },
        ConfigSection {
            name: "logging".to_string(),
            description: "Log level configuration".to_string(),
            entries: vec![ConfigEntry {
                key: "level".to_string(),
                value: toml_value
                    .map(|t| toml_string(t, "logging", "level", "info"))
                    .unwrap_or_else(|| "info".to_string()),
            }],
            redirect_route: None,
        },
        ConfigSection {
            name: "llm".to_string(),
            description: "LLM backend configuration".to_string(),
            entries: vec![],
            redirect_route: Some("llm".to_string()),
        },
        ConfigSection {
            name: "triggers".to_string(),
            description: "Trigger configuration".to_string(),
            entries: vec![],
            redirect_route: Some("triggers".to_string()),
        },
    ];

    ApollaConfigView {
        config_path: config_path.to_string(),
        config_exists: exists,
        sections,
    }
}

/// Retourne la configuration actuelle sous forme de vue plate pour l'UI.
///
/// Lit le fichier `~/.apollia/apollia.toml` et extrait les valeurs par section.
/// Si le fichier n'existe pas, retourne les valeurs par défaut.
#[tauri::command]
pub async fn get_config() -> Result<ApollaConfigView, String> {
    let path = default_config_path();
    let path_str = path.display().to_string();

    if !path.exists() {
        return Ok(build_config_view(None, &path_str, false));
    }

    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    let toml_value: toml::Value = content
        .parse()
        .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;

    Ok(build_config_view(Some(&toml_value), &path_str, true))
}

/// Ouvre le fichier `apollia.toml` dans l'éditeur par défaut du système.
///
/// Utilise `open::that()` pour la compatibilité cross-platform
/// (macOS: `open`, Linux: `xdg-open`, Windows: `start`).
#[tauri::command]
pub async fn open_config_in_editor() -> Result<(), String> {
    let path = default_config_path();

    if !path.exists() {
        let parent = path
            .parent()
            .ok_or_else(|| "cannot resolve config directory".to_string())?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create config directory: {e}"))?;
        tokio::fs::write(
            &path,
            "# Apollia OS configuration\n# See documentation for available options.\n",
        )
        .await
        .map_err(|e| format!("failed to create config file: {e}"))?;
    }

    open::that(&path).map_err(|e| format!("failed to open editor: {e}"))
}

/// Supprime le flag d'onboarding complété pour permettre de revoir l'onboarding.
///
/// Le flag est stocké dans `~/.apollia/.onboarded`. Sa suppression
/// déclenche l'affichage de la modale d'onboarding au prochain lancement.
#[tauri::command]
pub async fn reset_onboarding() -> Result<(), String> {
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    let flag_path = home.join(".apollia").join(".onboarded");

    if flag_path.exists() {
        tokio::fs::remove_file(&flag_path)
            .await
            .map_err(|e| format!("failed to remove onboarding flag: {e}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_path_ends_with_apollia_toml() {
        // GIVEN the default config path
        let path = default_config_path();
        // THEN it ends with apollia.toml inside .apollia directory
        assert!(path.ends_with("apollia.toml"));
        assert!(path.to_string_lossy().contains(".apollia"));
    }

    #[test]
    fn test_build_config_view_without_toml() {
        // GIVEN no TOML content
        // WHEN building the config view
        let view = build_config_view(None, "/fake/path.toml", false);

        // THEN all sections are present with defaults
        assert!(!view.config_exists);
        assert_eq!(view.sections.len(), 7);

        let runtime = &view.sections[0];
        assert_eq!(runtime.name, "runtime");
        assert_eq!(runtime.entries.len(), 3);
        assert_eq!(runtime.entries[1].key, "api_port");
        assert_eq!(runtime.entries[1].value, "7771");
    }

    #[test]
    fn test_build_config_view_with_toml() {
        // GIVEN a TOML value with custom runtime port
        let toml_str = r#"
            [runtime]
            api_port = 9999
            socket_path = "/custom/path.sock"

            [logging]
            level = "debug"
        "#;
        let toml_value: toml::Value = toml_str.parse().expect("valid toml");

        // WHEN building the config view
        let view = build_config_view(Some(&toml_value), "/test/apollia.toml", true);

        // THEN custom values are extracted
        assert!(view.config_exists);

        let runtime = &view.sections[0];
        assert_eq!(runtime.entries[0].value, "/custom/path.sock");
        assert_eq!(runtime.entries[1].value, "9999");
        assert_eq!(runtime.entries[2].value, "10"); // default

        let logging = &view.sections[4];
        assert_eq!(logging.entries[0].value, "debug");
    }

    #[test]
    fn test_build_config_view_llm_and_triggers_redirect() {
        // GIVEN a config view
        let view = build_config_view(None, "/fake.toml", false);

        // THEN llm and triggers sections redirect to dedicated views
        let llm = view
            .sections
            .iter()
            .find(|s| s.name == "llm")
            .expect("llm section");
        assert_eq!(llm.redirect_route, Some("llm".to_string()));
        assert!(llm.entries.is_empty());

        let triggers = view
            .sections
            .iter()
            .find(|s| s.name == "triggers")
            .expect("triggers section");
        assert_eq!(triggers.redirect_route, Some("triggers".to_string()));
        assert!(triggers.entries.is_empty());
    }

    #[test]
    fn test_toml_string_extracts_values() {
        // GIVEN a TOML table
        let toml_value: toml::Value = r#"
            [section]
            str_key = "hello"
            int_key = 42
            bool_key = true
        "#
        .parse()
        .expect("valid toml");

        // THEN values are extracted correctly
        assert_eq!(
            toml_string(&toml_value, "section", "str_key", "default"),
            "hello"
        );
        assert_eq!(toml_string(&toml_value, "section", "int_key", "0"), "42");
        assert_eq!(
            toml_string(&toml_value, "section", "bool_key", "false"),
            "true"
        );
        assert_eq!(
            toml_string(&toml_value, "section", "missing_key", "default"),
            "default"
        );
        assert_eq!(
            toml_string(&toml_value, "missing_section", "key", "default"),
            "default"
        );
    }
}
