//! Commandes IPC Tauri pour la vue Settings.
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

/// Résout le chemin du flag d'onboarding `~/.apollia/.onboarded`.
fn onboarded_flag_path() -> PathBuf {
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    home.join(".apollia").join(".onboarded")
}

/// Vérifie si l'onboarding a déjà été effectué.
///
/// Retourne `true` si le fichier `~/.apollia/.onboarded` existe.
#[tauri::command]
pub async fn check_onboarded() -> Result<bool, String> {
    Ok(onboarded_flag_path().exists())
}

/// Marque l'onboarding comme terminé en créant le fichier flag.
///
/// Crée `~/.apollia/.onboarded` (et le répertoire parent si nécessaire).
#[tauri::command]
pub async fn mark_onboarded() -> Result<(), String> {
    let flag_path = onboarded_flag_path();

    if let Some(parent) = flag_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create directory: {e}"))?;
    }

    tokio::fs::write(&flag_path, "")
        .await
        .map_err(|e| format!("failed to write onboarding flag: {e}"))
}

/// Supprime le flag d'onboarding complété pour permettre de revoir l'onboarding.
///
/// Le flag est stocké dans `~/.apollia/.onboarded`. Sa suppression
/// déclenche l'affichage de la modale d'onboarding au prochain lancement.
#[tauri::command]
pub async fn reset_onboarding() -> Result<(), String> {
    let flag_path = onboarded_flag_path();

    if flag_path.exists() {
        tokio::fs::remove_file(&flag_path)
            .await
            .map_err(|e| format!("failed to remove onboarding flag: {e}"))?;
    }

    Ok(())
}

/// Informations système affichées dans la section Avancé de Settings.
#[derive(Debug, Serialize)]
pub struct SystemInfo {
    /// Version d'Apollia OS (ex: `"0.1.0"`).
    pub version: String,
    /// Système d'exploitation et architecture (ex: `"macos aarch64"`).
    pub os: String,
    /// Chemin absolu vers l'interpréteur Python 3, si détecté.
    pub python_path: Option<String>,
}

/// Retourne les informations système pour la section Avancé de Settings.
///
/// Détecte la version d'Apollia, l'OS, et le chemin Python 3 via
/// `python3 -c "import sys; print(sys.executable)"`.
#[tauri::command]
pub async fn get_system_info() -> Result<SystemInfo, String> {
    let version = env!("CARGO_PKG_VERSION").to_string();
    let os = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);

    let python_path = match tokio::process::Command::new("python3")
        .args(["-c", "import sys; print(sys.executable)"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if path.is_empty() {
                None
            } else {
                Some(path)
            }
        }
        _ => None,
    };

    Ok(SystemInfo {
        version,
        os,
        python_path,
    })
}

/// Vérifie si Python 3 est disponible sur le système.
///
/// Exécute `python3 --version` et retourne `true` si la commande réussit.
#[tauri::command]
pub async fn check_python() -> Result<bool, String> {
    let result = tokio::process::Command::new("python3")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    match result {
        Ok(status) => Ok(status.success()),
        Err(_) => Ok(false),
    }
}

/// Vérifie si au moins un backend LLM est configuré.
///
/// Délègue à `GET /api/v1/llm/status` sur l'API REST interne et
/// retourne `true` si au moins un backend est disponible.
#[tauri::command]
pub async fn check_llm_configured(
    state: tauri::State<'_, apollia_runtime::embedded::RuntimeHandle>,
) -> Result<bool, String> {
    let json = super::http_get_json(state.api_port, "/api/v1/llm/status").await;

    match json {
        Ok(resp) => {
            let has_backends = resp
                .get("backends")
                .and_then(|v| v.as_array())
                .map(|arr| !arr.is_empty())
                .unwrap_or(false);
            Ok(has_backends)
        }
        Err(_) => Ok(false),
    }
}

/// Vérifie si `hello_agent.py` existe dans le répertoire `agents/`.
///
/// Cherche dans le répertoire de travail courant.
/// Retourne le chemin absolu si trouvé, sinon `None`.
#[tauri::command]
pub async fn check_hello_agent_exists() -> Result<Option<String>, String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let path = cwd.join("agents").join("hello_agent.py");

    if path.exists() {
        Ok(Some(path.display().to_string()))
    } else {
        Ok(None)
    }
}

/// Information about a pre-installed agent discovered in the `agents/` directory.
#[derive(Debug, Serialize)]
pub struct AvailableAgent {
    /// File name without extension (e.g. `"document-analyst"`).
    pub id: String,
    /// Absolute path to the `.py` file.
    pub path: String,
}

/// Scans the `agents/` directory for Python agent files.
///
/// Returns all `.py` files found, excluding `__init__.py` and base classes
/// (files whose name contains `_base`). Each entry includes the stem as `id`
/// and the absolute path.
#[tauri::command]
pub async fn list_available_agents() -> Result<Vec<AvailableAgent>, String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let agents_dir = cwd.join("agents");

    if !agents_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut entries = tokio::fs::read_dir(&agents_dir)
        .await
        .map_err(|e| format!("failed to read agents directory: {e}"))?;

    let mut agents = Vec::new();

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("failed to read directory entry: {e}"))?
    {
        let path = entry.path();
        let Some(ext) = path.extension() else {
            continue;
        };
        if ext != "py" {
            continue;
        }

        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();

        if name == "__init__.py" || name.contains("_base") {
            continue;
        }

        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        agents.push(AvailableAgent {
            id: stem,
            path: path.display().to_string(),
        });
    }

    agents.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(agents)
}

/// Result of the local LLM setup operation.
#[derive(Debug, Serialize)]
pub struct SetupLlmResult {
    /// Absolute path where the model is stored.
    pub model_path: String,
    /// Inferred quantization from the filename (e.g. `"q8_0"`, `"q4_k_m"`).
    pub quantization: String,
}

/// Sets up a local embedded LLM from a user-selected GGUF file.
///
/// Copies the model into `~/.apollia/models/`, adds the `[llm]` section
/// to `apollia.toml`, and returns the path for confirmation.
///
/// This is a first-launch helper — it writes a minimal LLM config block
/// so the onboarding agent can function. Advanced users can edit the
/// TOML directly afterwards.
#[tauri::command]
pub async fn setup_local_llm(gguf_path: String) -> Result<SetupLlmResult, String> {
    let source = PathBuf::from(&gguf_path);

    // Validate the file exists and is a .gguf
    if !source.exists() {
        return Err(format!("file not found: {gguf_path}"));
    }
    if source.extension().and_then(|e| e.to_str()) != Some("gguf") {
        return Err("expected a .gguf file".into());
    }

    // Infer quantization from filename (e.g. "Qwen3-0.6B-Q8_0.gguf" → "q8_0")
    let file_stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model");
    let quantization = infer_quantization(file_stem);

    // Copy into ~/.apollia/models/
    let models_dir = default_config_path()
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("models");
    tokio::fs::create_dir_all(&models_dir)
        .await
        .map_err(|e| format!("failed to create models directory: {e}"))?;

    let file_name = source
        .file_name()
        .ok_or("invalid file name")?
        .to_string_lossy()
        .to_string();
    let dest = models_dir.join(&file_name);

    // Only copy if not already there
    if dest != source {
        tokio::fs::copy(&source, &dest)
            .await
            .map_err(|e| format!("failed to copy model: {e}"))?;
    }

    let model_path_str = format!("~/.apollia/models/{file_name}");

    // Append [llm] section to apollia.toml if not already present
    let config_path = default_config_path();
    append_llm_config(&config_path, &model_path_str, &quantization).await?;

    tracing::info!(
        model = %model_path_str,
        quantization = %quantization,
        "local LLM configured via onboarding setup"
    );

    Ok(SetupLlmResult {
        model_path: dest.display().to_string(),
        quantization,
    })
}

/// Hot-reloads the LLM router from `apollia.toml`.
///
/// Re-reads the TOML config, builds a new `LlmRouter`, and injects it
/// into the `ChatSessionManager` so the new model is available immediately
/// without restarting the application.
#[tauri::command]
pub async fn reload_llm(
    state: tauri::State<'_, apollia_runtime::embedded::RuntimeHandle>,
) -> Result<bool, String> {
    // 1. Re-read apollia.toml
    let config_path = default_config_path();
    if !config_path.exists() {
        return Err("apollia.toml not found".into());
    }
    let content = tokio::fs::read_to_string(&config_path)
        .await
        .map_err(|e| format!("failed to read apollia.toml: {e}"))?;

    // 2. Parse the [llm] section
    #[derive(serde::Deserialize)]
    struct LlmSection {
        llm: Option<apollia_llm::LlmConfig>,
    }
    let llm_config = toml::from_str::<LlmSection>(&content)
        .map_err(|e| format!("failed to parse apollia.toml: {e}"))?
        .llm;

    let Some(config) = llm_config else {
        return Err("no [llm] section found in apollia.toml".into());
    };

    // 3. Build a new LlmRouter
    let router = apollia_llm::LlmRouter::from_config(&config)
        .await
        .map_err(|e| format!("failed to load LLM: {e}"))?;

    tracing::info!("LLM router reloaded from apollia.toml");

    // 4. Inject into the ChatSessionManager
    if let Some(ref manager) = state.chat_manager {
        manager
            .reload_llm(Some(std::sync::Arc::new(router)))
            .await;
    }

    Ok(true)
}

/// Infers the quantization type from a GGUF filename.
///
/// Looks for common patterns like `Q8_0`, `Q4_K_M`, `Q5_K_S`, etc.
/// Returns `"q4_k_m"` as a safe default if nothing is detected.
fn infer_quantization(stem: &str) -> String {
    let upper = stem.to_uppercase();
    // Common GGUF quantization suffixes (ordered by specificity)
    let patterns = [
        "Q8_0", "Q6_K", "Q5_K_M", "Q5_K_S", "Q5_0", "Q4_K_M", "Q4_K_S", "Q4_0", "Q3_K_M",
        "Q3_K_S", "Q2_K", "IQ4_XS", "IQ3_M", "IQ2_S", "F16", "F32",
    ];
    for p in &patterns {
        if upper.contains(p) {
            return p.to_lowercase();
        }
    }
    "q4_k_m".to_string()
}

/// Appends a minimal `[llm]` configuration block to `apollia.toml`.
///
/// Skips if the file already contains a `[llm]` section.
async fn append_llm_config(
    config_path: &std::path::Path,
    model_path: &str,
    quantization: &str,
) -> Result<(), String> {
    let existing = if config_path.exists() {
        tokio::fs::read_to_string(config_path)
            .await
            .unwrap_or_default()
    } else {
        String::new()
    };

    // Don't overwrite an existing [llm] section
    if existing.contains("[llm]") {
        return Ok(());
    }

    let device = if cfg!(target_os = "macos") {
        "metal"
    } else {
        "cpu"
    };

    let llm_block = format!(
        r#"

# ─────────────────────────────────────────────
# LLM — configured automatically during onboarding
# ─────────────────────────────────────────────
[llm]
default = "local"

[llm.observability]
log_token_usage  = true
log_latency      = true
log_cost         = false
debug_log_prompt = false

[[llm.backends]]
type         = "embedded"
name         = "local"
model_path   = "{model_path}"
device       = "{device}"
quantization = "{quantization}"
"#
    );

    let mut content = existing;
    content.push_str(&llm_block);

    if let Some(parent) = config_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create config directory: {e}"))?;
    }
    tokio::fs::write(config_path, &content)
        .await
        .map_err(|e| format!("failed to write apollia.toml: {e}"))?;

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
    fn test_onboarded_flag_path_ends_with_onboarded() {
        // GIVEN the onboarded flag path
        let path = onboarded_flag_path();
        // THEN it ends with .onboarded inside .apollia directory
        assert!(path.ends_with(".onboarded"));
        assert!(path.to_string_lossy().contains(".apollia"));
    }

    #[test]
    fn test_build_config_view_without_toml() {
        // GIVEN no TOML content
        // WHEN building the config view
        let view = build_config_view(None, "/fake/path.toml", false);

        // THEN all sections are present with defaults
        assert!(!view.config_exists);
        assert_eq!(view.sections.len(), 6);

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
    fn test_build_config_view_llm_redirect() {
        // GIVEN a config view
        let view = build_config_view(None, "/fake.toml", false);

        // THEN llm section redirects to dedicated view
        let llm = view
            .sections
            .iter()
            .find(|s| s.name == "llm")
            .expect("llm section");
        assert_eq!(llm.redirect_route, Some("llm".to_string()));
        assert!(llm.entries.is_empty());

        // AND triggers section is absent (migrated to SQLite CRUD)
        let triggers = view.sections.iter().find(|s| s.name == "triggers");
        assert!(triggers.is_none());
    }

    #[tokio::test]
    async fn test_list_available_agents_returns_sorted_results() {
        // GIVEN the agents/ directory exists in the workspace
        // WHEN listing available agents
        let result = list_available_agents().await;

        // THEN the command succeeds
        assert!(result.is_ok());
        let agents = result.expect("list_available_agents should succeed");

        // AND results are sorted alphabetically by id
        for window in agents.windows(2) {
            assert!(
                window[0].id <= window[1].id,
                "agents should be sorted: {} <= {}",
                window[0].id,
                window[1].id
            );
        }

        // AND base class files are excluded
        assert!(
            !agents.iter().any(|a| a.id.contains("_base")),
            "base class files should be excluded"
        );

        // AND all paths are absolute and end with .py
        for agent in &agents {
            assert!(agent.path.ends_with(".py"), "path should end with .py");
        }
    }

    #[tokio::test]
    async fn test_get_system_info_returns_valid_data() {
        // GIVEN the get_system_info command
        // WHEN called
        let result = get_system_info().await;

        // THEN it succeeds with valid fields
        let info = result.expect("get_system_info should succeed");
        assert!(!info.version.is_empty(), "version should not be empty");
        assert!(!info.os.is_empty(), "os should not be empty");
        assert!(
            info.os.contains(std::env::consts::OS),
            "os should contain the current OS"
        );
        assert!(
            info.os.contains(std::env::consts::ARCH),
            "os should contain the current architecture"
        );
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

    #[test]
    fn test_infer_quantization_common_patterns() {
        // GIVEN various GGUF filenames
        // THEN the quantization is correctly inferred
        assert_eq!(infer_quantization("Qwen3-0.6B-Q8_0"), "q8_0");
        assert_eq!(infer_quantization("llama-3-8b-Q4_K_M"), "q4_k_m");
        assert_eq!(infer_quantization("mistral-7b-Q5_K_S"), "q5_k_s");
        assert_eq!(infer_quantization("phi-3-mini-F16"), "f16");
        assert_eq!(infer_quantization("model-Q3_K_M"), "q3_k_m");
    }

    #[test]
    fn test_infer_quantization_fallback() {
        // GIVEN a filename with no recognizable quantization
        // THEN the default is returned
        assert_eq!(infer_quantization("some-random-model"), "q4_k_m");
    }

    #[tokio::test]
    async fn test_append_llm_config_skips_if_already_present() {
        // GIVEN a config file that already has [llm]
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("apollia.toml");
        tokio::fs::write(&config_path, "[llm]\ndefault = \"existing\"\n")
            .await
            .expect("write");

        // WHEN appending LLM config
        append_llm_config(&config_path, "~/.apollia/models/test.gguf", "q8_0")
            .await
            .expect("append");

        // THEN the existing content is unchanged
        let content = tokio::fs::read_to_string(&config_path)
            .await
            .expect("read");
        assert!(content.contains("existing"));
        assert!(!content.contains("embedded"));
    }

    #[tokio::test]
    async fn test_append_llm_config_writes_block() {
        // GIVEN an empty config file
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("apollia.toml");
        tokio::fs::write(&config_path, "[runtime]\nport = 7771\n")
            .await
            .expect("write");

        // WHEN appending LLM config
        append_llm_config(&config_path, "~/.apollia/models/test.gguf", "q8_0")
            .await
            .expect("append");

        // THEN the LLM block is appended
        let content = tokio::fs::read_to_string(&config_path)
            .await
            .expect("read");
        assert!(content.contains("[llm]"));
        assert!(content.contains("test.gguf"));
        assert!(content.contains("q8_0"));
        assert!(content.contains("[runtime]")); // original preserved
    }
}
