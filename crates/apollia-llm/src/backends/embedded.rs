//! Backend d'inférence embarqué — inférence in-process via `mistralrs`.
//!
//! Ce module est compilé uniquement avec `feature = "local"`.
//!
//! # Architecture
//!
//! ```text
//! apollia-llm [feature = "local"]
//!   └── EmbeddedBackend : CompletionModel
//!         ├── load()     — charge le .gguf, configure le device, initialise le moteur
//!         ├── complete() — inférence synchrone via send_chat_request (AC-3)
//!         └── stream()   — fallback AC-5 : complete() → un seul chunk
//! ```
//!
//! # Devices supportés
//!
//! | Feature          | Device          | API                                    |
//! |------------------|-----------------|----------------------------------------|
//! | `local` / `local-cpu` | CPU        | défaut, aucune configuration           |
//! | `local-metal`    | Apple Silicon GPU | `GgufModelBuilder::with_device(Device::new_metal(0))` |
//! | `local-accelerate` | CPU + BLAS   | `mistralrs/accelerate` (pas de device) |
//! | `local-cuda`     | GPU NVIDIA       | non implémenté (feature déclarée)      |
//!
//! # Build Metal (prérequis)
//!
//! Xcode complet est requis pour compiler les shaders Metal embarqués dans
//! `mistralrs-paged-attn`. Sans Xcode (Command Line Tools seul), passer :
//! ```sh
//! MISTRALRS_METAL_PRECOMPILE=0 cargo build --release --features local-metal
//! ```
//! Les shaders seront compilés à l'exécution par le driver Metal.
//!
//! # Streaming (AC-5)
//!
//! `mistralrs::model::Stream<'a>` porte un lifetime lié au modèle et n'implémente
//! pas `futures::Stream`. La transférer dans un `tokio::spawn` ('static) n'est pas
//! possible sans un bridge par channel asynchrone — complexité injustifiée pour STORY-052.
//! Conformément à AC-5, `stream()` délègue à `complete()` et retourne le contenu
//! complet en un seul chunk. L'implémentation streaming native sera ajoutée en STORY-064
//! si `mistralrs` expose une API `'static`.
//!
//! # Dépendances moteur
//!
//! Utilise `mistralrs` (SDK public) qui embarque `mistralrs-core` (moteur inférence).
//! `GgufModelBuilder` charge le `.gguf` en mémoire — principe #1 local-first.

use std::fmt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use futures::Stream;

use crate::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, MessageContent,
    Role, StreamChunk, TokenUsage, ToolCall,
};

use mistralrs::{GgufModelBuilder, Model, TextMessageRole, TextMessages, ToolCallResponse};

// ─────────────────────────────────────────────
// Configuration
// ─────────────────────────────────────────────

/// Accélérateur matériel pour l'inférence locale.
///
/// Le variant choisi doit correspondre à une feature compilée dans le binaire :
/// - `Cpu` — toujours disponible avec `feature = "local"` ou `"local-cpu"`.
/// - `Cuda` — nécessite `feature = "local-cuda"` (GPU NVIDIA).
/// - `Metal` — nécessite `feature = "local-metal"` (GPU Apple Silicon).
///
/// Si le device demandé n'est pas compilé, `EmbeddedBackend::load()` retourne
/// `Err(LlmError::DeviceNotAvailable)` sans paniquer (fail-fast au démarrage).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AcceleratorDevice {
    /// Inférence CPU — toujours disponible (défaut).
    #[default]
    Cpu,
    /// GPU NVIDIA via CUDA — nécessite `--features local-cuda`.
    Cuda,
    /// GPU Apple Silicon via Metal — nécessite `--features local-metal`.
    Metal,
}

impl AcceleratorDevice {
    /// Valide que ce device est disponible dans les features compilées.
    ///
    /// Retourne `Ok(())` si le device peut être utilisé, ou
    /// `Err(LlmError::DeviceNotAvailable)` avec le hint de recompilation.
    fn check_compiled(&self) -> Result<(), LlmError> {
        match self {
            Self::Cpu => Ok(()),
            Self::Cuda => {
                if cfg!(feature = "local-cuda") {
                    Ok(())
                } else {
                    Err(LlmError::DeviceNotAvailable {
                        device: "cuda".into(),
                        hint: "local-cuda".into(),
                    })
                }
            }
            Self::Metal => {
                if cfg!(feature = "local-metal") {
                    Ok(())
                } else {
                    Err(LlmError::DeviceNotAvailable {
                        device: "metal".into(),
                        hint: "local-metal".into(),
                    })
                }
            }
        }
    }

    /// Retourne le label texte du device pour les logs.
    fn label(&self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Cuda => "cuda",
            Self::Metal => "metal",
        }
    }
}

/// Configuration du backend embarqué, désérialisée depuis la section
/// `[[llm.backends]]` de `apollia.toml` pour les entrées `type = "embedded"`.
///
/// # Exemple TOML
///
/// ```toml
/// [[llm.backends]]
/// name         = "local"
/// type         = "embedded"
/// model_path   = "~/.apollia/models/llama3.2-3b-q4.gguf"
/// quantization = "q4_k_m"
/// device       = "cpu"      # "cpu" | "cuda" | "metal"  (défaut: "cpu")
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddedBackendConfig {
    /// Nom logique du backend, utilisé comme clé dans le `LlmRouter`.
    pub name: String,
    /// Chemin complet vers le fichier `.gguf` du modèle sur le disque local.
    pub model_path: PathBuf,
    /// Quantisation du modèle — informatif uniquement, baked in the GGUF filename.
    ///
    /// Valeurs usuelles : `"q4_0"` | `"q4_k_m"` | `"q8_0"` | `"f16"`.
    pub quantization: String,
    /// Accélérateur matériel à utiliser pour l'inférence.
    ///
    /// Doit correspondre à une feature compilée — `Err(DeviceNotAvailable)` sinon.
    /// Défaut : `AcceleratorDevice::Cpu` (toujours disponible).
    #[serde(default)]
    pub device: AcceleratorDevice,
}

// ─────────────────────────────────────────────
// Backend
// ─────────────────────────────────────────────

/// Backend d'inférence in-process — principe #1 (local-first) et #2 (zéro dépendance externe).
///
/// Charge un modèle `.gguf` depuis `~/.apollia/models/` via `mistralrs::GgufModelBuilder`
/// et exécute l'inférence directement dans le processus Apollia OS, sans requête HTTP
/// ni processus externe.
///
/// Le fichier `.gguf` est une donnée externe (comme une base SQLite) : il n'est
/// jamais compilé dans le binaire. Le moteur d'inférence (`mistralrs-core`), lui,
/// est lié statiquement au binaire lors du build avec `feature = "local"`.
pub struct EmbeddedBackend {
    /// Moteur d'inférence chargé en mémoire — `Arc` pour le clonage du backend.
    ///
    /// `Model` n'implémente pas `Debug` — `EmbeddedBackend::fmt` l'omet volontairement.
    engine: Arc<Model>,
    /// Nom logique du backend, transmis depuis la config.
    name: String,
    /// Identifiant du modèle déduit du nom du fichier `.gguf` (sans extension).
    model_id: String,
}

/// `Debug` implémenté manuellement : `Model` n'implémente pas `Debug`.
impl fmt::Debug for EmbeddedBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddedBackend")
            .field("name", &self.name)
            .field("model_id", &self.model_id)
            .finish_non_exhaustive()
    }
}

/// Expands a leading `~` to `$HOME`.
///
/// Rust's `PathBuf` and `std::fs` do not perform shell tilde expansion.
/// Any `model_path` value coming from `apollia.toml` like `~/...` must be
/// resolved before calling `canonicalize()`.
fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(format!("{}{}", home, &s[1..]))
    } else {
        path.to_path_buf()
    }
}

impl EmbeddedBackend {
    /// Charge le modèle depuis le fichier `.gguf` indiqué dans la config.
    ///
    /// Résout le chemin en chemin canonique absolu — si le fichier est absent,
    /// retourne `Err(LlmError::ModelNotFound)` sans paniquer (AC-2).
    ///
    /// Sur succès, le moteur est en mémoire et `is_available()` retourne `true` (AC-1).
    ///
    /// # Erreurs
    ///
    /// - [`LlmError::ModelNotFound`] — `config.model_path` est introuvable sur le disque.
    /// - [`LlmError::InferenceError`] — initialisation du moteur échouée (GGUF corrompu, etc.).
    pub async fn load(config: &EmbeddedBackendConfig) -> Result<Self, LlmError> {
        // Fail-fast : vérifie que l'accélérateur demandé est compilé dans ce binaire.
        config.device.check_compiled()?;

        tracing::info!(
            backend = %config.name,
            path = %config.model_path.display(),
            quantization = %config.quantization,
            device = %config.device.label(),
            "chargement du modèle local"
        );

        // Expansion du préfixe `~` — PathBuf ne l'expand pas automatiquement.
        let expanded_path = expand_tilde(&config.model_path);

        // Résolution canonique — échoue si le fichier n'existe pas (AC-2).
        let canonical = expanded_path
            .canonicalize()
            .map_err(|_| LlmError::ModelNotFound {
                path: expanded_path.clone(),
            })?;

        // Décomposition : dossier parent + nom de fichier pour GgufModelBuilder.
        let folder = canonical.parent().ok_or_else(|| LlmError::ModelNotFound {
            path: canonical.clone(),
        })?;

        let filename = canonical
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| LlmError::ModelNotFound {
                path: canonical.clone(),
            })?;

        let model_id = canonical
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        // Construction du builder de base.
        let builder = GgufModelBuilder::new(folder.to_string_lossy().as_ref(), vec![filename]);

        // Sélection du device Metal — compilé uniquement avec feature = "local-metal".
        //
        // `check_compiled()` ci-dessus garantit que si `config.device == Metal`,
        // la feature est bien présente. Le bloc `else` préserve le builder CPU par défaut.
        #[cfg(feature = "local-metal")]
        let builder = if config.device == AcceleratorDevice::Metal {
            let device = mistralrs::Device::new_metal(0).map_err(|e| {
                LlmError::InferenceError(format!("échec initialisation Metal GPU : {e}"))
            })?;
            builder.with_device(device)
        } else {
            builder
        };

        // Chargement in-process via mistralrs — zéro HTTP, zéro processus externe.
        let engine = builder
            .build()
            .await
            .map_err(|e| LlmError::InferenceError(e.to_string()))?;

        tracing::info!(
            backend = %config.name,
            model_id = %model_id,
            path = %canonical.display(),
            device = %config.device.label(),
            "modèle local prêt"
        );

        Ok(Self {
            engine: Arc::new(engine),
            name: config.name.clone(),
            model_id,
        })
    }

    /// Convertit un `CompletionRequest` en `TextMessages` pour mistralrs.
    fn build_messages(req: &CompletionRequest) -> TextMessages {
        let mut messages = TextMessages::new();
        for msg in &req.messages {
            let role = match msg.role {
                Role::System => TextMessageRole::System,
                Role::User => TextMessageRole::User,
                Role::Assistant => TextMessageRole::Assistant,
                Role::Tool => TextMessageRole::Tool,
            };
            let content = match &msg.content {
                MessageContent::Text(t) => t.clone(),
                MessageContent::ToolResult { content, .. } => content.clone(),
                MessageContent::WithToolCalls { text, .. } => text.clone(),
            };
            messages = messages.add_message(role, content);
        }
        messages
    }

    /// Convertit un `finish_reason` string mistralrs → notre enum [`FinishReason`].
    fn map_finish_reason(s: &str) -> FinishReason {
        match s {
            "stop" => FinishReason::Stop,
            "length" => FinishReason::Length,
            "tool_calls" => FinishReason::ToolCalls,
            _ => FinishReason::Error,
        }
    }

    /// Convertit un [`ToolCallResponse`] mistralrs → notre [`ToolCall`].
    ///
    /// `arguments` est parsé comme JSON ; si le parsing échoue, la valeur brute
    /// est emballée dans une `Value::String` pour ne pas perdre l'information.
    fn map_tool_call(tc: &ToolCallResponse) -> ToolCall {
        let arguments = serde_json::from_str(&tc.function.arguments)
            .unwrap_or_else(|_| serde_json::Value::String(tc.function.arguments.clone()));
        ToolCall {
            id: tc.id.clone(),
            name: tc.function.name.clone(),
            arguments,
        }
    }
}

#[async_trait::async_trait]
impl CompletionModel for EmbeddedBackend {
    /// Exécute l'inférence in-process et retourne la réponse complète (AC-3).
    ///
    /// `usage.cost_usd` est toujours `None` — l'inférence locale est gratuite (AC-3).
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let started = Instant::now();
        let messages = Self::build_messages(&req);

        let response = self
            .engine
            .send_chat_request(messages)
            .await
            .map_err(|e| LlmError::InferenceError(e.to_string()))?;

        let choice =
            response.choices.into_iter().next().ok_or_else(|| {
                LlmError::ParseError("réponse vide : aucun choix retourné".into())
            })?;

        let content = choice.message.content.unwrap_or_default();
        let finish_reason = Self::map_finish_reason(&choice.finish_reason);
        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .iter()
            .map(Self::map_tool_call)
            .collect();

        Ok(CompletionResponse {
            content,
            tool_calls,
            usage: TokenUsage {
                prompt_tokens: response.usage.prompt_tokens as u32,
                completion_tokens: response.usage.completion_tokens as u32,
                cost_usd: None, // backend local = gratuit (AC-3)
            },
            finish_reason,
            latency_ms: started.elapsed().as_millis() as u64,
        })
    }

    /// Retourne un stream de chunks texte.
    ///
    /// # Fallback AC-5
    ///
    /// `mistralrs::model::Stream<'a>` porte un lifetime et n'implémente pas
    /// `futures::Stream`. Elle ne peut être transférée dans un task `'static`
    /// sans un bridge par channel — complexity injustifiée pour STORY-052.
    ///
    /// Conformément à AC-5 : délègue à `complete()` et retourne le contenu
    /// complet en un seul chunk `Ok(content)`.
    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        let response = self.complete(req).await?;
        let content = response.content;
        let stream = futures::stream::once(async move { Ok(StreamChunk::Text(content)) });
        Ok(Box::pin(stream))
    }

    /// Retourne `true` : le moteur est chargé en mémoire et prêt (AC-1).
    fn is_available(&self) -> bool {
        true
    }

    /// Nom logique du backend tel que configuré dans `apollia.toml`.
    fn backend_name(&self) -> &str {
        &self.name
    }

    /// Identifiant du modèle déduit du nom du fichier `.gguf`.
    fn model_id(&self) -> &str {
        &self.model_id
    }
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatMessage;

    // GIVEN AcceleratorDevice::default()
    // WHEN on lit le variant
    // THEN c'est Cpu
    #[test]
    fn test_accelerator_device_default_is_cpu() {
        assert_eq!(AcceleratorDevice::default(), AcceleratorDevice::Cpu);
    }

    // GIVEN les trois variants sérialisés en JSON
    // WHEN on désérialise
    // THEN les bons variants sont retournés (serde rename_all = "lowercase")
    #[test]
    fn test_accelerator_device_serde_roundtrip() {
        let cases = [
            (AcceleratorDevice::Cpu, "\"cpu\""),
            (AcceleratorDevice::Cuda, "\"cuda\""),
            (AcceleratorDevice::Metal, "\"metal\""),
        ];
        for (device, expected_json) in &cases {
            let json = serde_json::to_string(device).expect("sérialisation doit réussir");
            assert_eq!(&json, expected_json, "sérialisation de {device:?}");
            let back: AcceleratorDevice =
                serde_json::from_str(&json).expect("désérialisation doit réussir");
            assert_eq!(&back, device, "roundtrip de {device:?}");
        }
    }

    // GIVEN AcceleratorDevice::Cpu
    // WHEN on appelle check_compiled()
    // THEN Ok(()) est toujours retourné (CPU toujours disponible)
    #[test]
    fn test_accelerator_device_cpu_always_available() {
        assert!(
            AcceleratorDevice::Cpu.check_compiled().is_ok(),
            "Cpu doit toujours être disponible"
        );
    }

    // GIVEN AcceleratorDevice::Cuda compilé sans feature local-cuda
    // WHEN on appelle check_compiled()
    // THEN Err(LlmError::DeviceNotAvailable { device: "cuda", .. }) est retourné
    #[cfg(not(feature = "local-cuda"))]
    #[test]
    fn test_accelerator_device_cuda_not_compiled() {
        let result = AcceleratorDevice::Cuda.check_compiled();
        assert!(
            matches!(result, Err(LlmError::DeviceNotAvailable { ref device, .. }) if device == "cuda"),
            "attendu DeviceNotAvailable cuda, obtenu: {result:?}"
        );
    }

    // GIVEN AcceleratorDevice::Metal compilé avec feature local-metal
    // WHEN on appelle check_compiled()
    // THEN Ok(()) est retourné — le GPU Apple Silicon est disponible
    #[cfg(feature = "local-metal")]
    #[test]
    fn test_accelerator_device_metal_compiled() {
        assert!(
            AcceleratorDevice::Metal.check_compiled().is_ok(),
            "Metal doit être disponible quand feature local-metal est compilée"
        );
    }

    // GIVEN AcceleratorDevice::Metal compilé sans feature local-metal
    // WHEN on appelle check_compiled()
    // THEN Err(LlmError::DeviceNotAvailable { device: "metal", .. }) est retourné
    #[cfg(not(feature = "local-metal"))]
    #[test]
    fn test_accelerator_device_metal_not_compiled() {
        let result = AcceleratorDevice::Metal.check_compiled();
        assert!(
            matches!(result, Err(LlmError::DeviceNotAvailable { ref device, .. }) if device == "metal"),
            "attendu DeviceNotAvailable metal, obtenu: {result:?}"
        );
    }

    // GIVEN un EmbeddedBackendConfig avec device explicite
    // WHEN on sérialise puis désérialise
    // THEN tous les champs incluant device sont préservés
    #[test]
    fn test_embedded_backend_config_with_device_serde() {
        let original = EmbeddedBackendConfig {
            name: "local-gpu".into(),
            model_path: PathBuf::from("/home/user/.apollia/models/llama3.2-q4.gguf"),
            quantization: "q4_k_m".into(),
            device: AcceleratorDevice::Cuda,
        };

        let json = serde_json::to_string(&original).expect("sérialisation doit réussir");
        let restored: EmbeddedBackendConfig =
            serde_json::from_str(&json).expect("désérialisation doit réussir");

        assert_eq!(restored.name, original.name);
        assert_eq!(restored.model_path, original.model_path);
        assert_eq!(restored.quantization, original.quantization);
        assert_eq!(restored.device, original.device);
    }

    // GIVEN un JSON sans champ device
    // WHEN on désérialise EmbeddedBackendConfig
    // THEN device vaut AcceleratorDevice::Cpu (serde(default))
    #[test]
    fn test_embedded_backend_config_device_defaults_to_cpu() {
        let json = r#"{
            "name": "local",
            "model_path": "/tmp/model.gguf",
            "quantization": "q8_0"
        }"#;

        let config: EmbeddedBackendConfig =
            serde_json::from_str(json).expect("désérialisation doit réussir");

        assert_eq!(
            config.device,
            AcceleratorDevice::Cpu,
            "device absent du JSON doit valoir Cpu par défaut"
        );
    }

    // GIVEN un EmbeddedBackendConfig avec un model_path inexistant
    // WHEN on appelle EmbeddedBackend::load(&config).await
    // THEN Err(LlmError::ModelNotFound { .. }) est retourné sans panique (AC-2)
    #[tokio::test]
    async fn test_ac2_load_returns_model_not_found() {
        let config = EmbeddedBackendConfig {
            name: "local".into(),
            model_path: PathBuf::from("/tmp/does-not-exist-xyz-apollia.gguf"),
            quantization: "q4_k_m".into(),
            device: AcceleratorDevice::Cpu,
        };

        let result = EmbeddedBackend::load(&config).await;

        assert!(
            matches!(result, Err(LlmError::ModelNotFound { .. })),
            "expected ModelNotFound, got: {result:?}"
        );
    }

    // GIVEN un JSON valide représentant un EmbeddedBackendConfig
    // WHEN on désérialise via serde_json
    // THEN les champs sont correctement initialisés
    #[test]
    fn test_embedded_backend_config_serde() {
        let json = r#"{
            "name": "local",
            "model_path": "/tmp/model.gguf",
            "quantization": "q4_k_m"
        }"#;

        let config: EmbeddedBackendConfig =
            serde_json::from_str(json).expect("désérialisation doit réussir");

        assert_eq!(config.name, "local");
        assert_eq!(config.model_path, PathBuf::from("/tmp/model.gguf"));
        assert_eq!(config.quantization, "q4_k_m");
    }

    // GIVEN un EmbeddedBackendConfig sérialisé puis désérialisé
    // WHEN on fait un roundtrip JSON
    // THEN les valeurs sont préservées
    #[test]
    fn test_embedded_backend_config_serde_roundtrip() {
        let original = EmbeddedBackendConfig {
            name: "local".into(),
            model_path: PathBuf::from("/home/user/.apollia/models/llama3.2-q4.gguf"),
            quantization: "q8_0".into(),
            device: AcceleratorDevice::Cpu,
        };

        let json = serde_json::to_string(&original).expect("sérialisation doit réussir");
        let restored: EmbeddedBackendConfig =
            serde_json::from_str(&json).expect("désérialisation doit réussir");

        assert_eq!(restored.name, original.name);
        assert_eq!(restored.model_path, original.model_path);
        assert_eq!(restored.quantization, original.quantization);
    }

    // GIVEN les chaînes finish_reason de mistralrs
    // WHEN on appelle map_finish_reason
    // THEN les variantes correctes sont retournées
    #[test]
    fn test_map_finish_reason() {
        assert_eq!(
            EmbeddedBackend::map_finish_reason("stop"),
            FinishReason::Stop
        );
        assert_eq!(
            EmbeddedBackend::map_finish_reason("length"),
            FinishReason::Length
        );
        assert_eq!(
            EmbeddedBackend::map_finish_reason("tool_calls"),
            FinishReason::ToolCalls
        );
        assert_eq!(
            EmbeddedBackend::map_finish_reason("unknown"),
            FinishReason::Error
        );
    }

    // GIVEN un ToolCallResponse mistralrs avec arguments JSON valide
    // WHEN on appelle map_tool_call
    // THEN les champs id, name et arguments sont correctement mappés
    #[test]
    fn test_map_tool_call_with_json_args() {
        use mistralrs::{CalledFunction, ToolCallResponse, ToolCallType};

        let tc = ToolCallResponse {
            id: "call_01".into(),
            tp: ToolCallType::Function,
            index: 0,
            function: CalledFunction {
                name: "file_io".into(),
                arguments: r#"{"path": "/tmp/test.txt"}"#.into(),
            },
        };

        let mapped = EmbeddedBackend::map_tool_call(&tc);

        assert_eq!(mapped.id, "call_01");
        assert_eq!(mapped.name, "file_io");
        assert_eq!(mapped.arguments["path"], "/tmp/test.txt");
    }

    // GIVEN un ToolCallResponse avec arguments JSON invalide
    // WHEN on appelle map_tool_call
    // THEN les arguments sont conservés en tant que Value::String sans panique
    #[test]
    fn test_map_tool_call_with_invalid_json_args() {
        use mistralrs::{CalledFunction, ToolCallResponse, ToolCallType};

        let tc = ToolCallResponse {
            id: "call_02".into(),
            tp: ToolCallType::Function,
            index: 0,
            function: CalledFunction {
                name: "bash_executor".into(),
                arguments: "not-valid-json".into(),
            },
        };

        let mapped = EmbeddedBackend::map_tool_call(&tc);

        assert_eq!(mapped.id, "call_02");
        assert_eq!(mapped.name, "bash_executor");
        assert!(
            mapped.arguments.is_string(),
            "arguments doit être Value::String en fallback"
        );
    }

    // ─────────────────────────────────────────────
    // Tests d'intégration — modèle réel (lents, exclus du CI)
    // ─────────────────────────────────────────────
    //
    // Nécessite le fichier models/Qwen3.5-0.8B-Q6_K.gguf à la racine du workspace.
    // Si le fichier est absent, le test est ignoré sans échec.
    //
    // Exécution : cargo test -p apollia-llm --features local -- --nocapture

    /// Chemin vers le modèle de test — relatif à la racine du workspace.
    const TEST_MODEL_PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../models/Qwen3-0.6B-Q8_0.gguf"
    );

    // GIVEN le fichier models/Qwen3.5-0.8B-Q6_K.gguf présent dans le workspace
    // WHEN on charge le backend puis on appelle complete() et stream()
    // THEN AC-1 : load() réussit, is_available() = true, model_id est non vide
    // THEN AC-3 : complete() retourne cost_usd=None, latency_ms>0, finish_reason valide, content non vide
    // THEN AC-4 : stream() retourne au moins un chunk non vide
    // THEN AC-5 : fallback — stream retourne exactement 1 chunk (contenu complet d'un seul tenant)
    #[tokio::test]
    async fn test_ac1_ac3_ac4_ac5_with_real_model() {
        if !std::path::Path::new(TEST_MODEL_PATH).exists() {
            eprintln!(
                "skip: modèle absent — déposez un .gguf dans models/ pour activer ce test\n  attendu : {TEST_MODEL_PATH}"
            );
            return;
        }

        // ── AC-1 : chargement réussi ──────────────────────────────────────
        let config = EmbeddedBackendConfig {
            name: "test-local".into(),
            model_path: PathBuf::from(TEST_MODEL_PATH),
            quantization: "q8_0".into(),
            device: AcceleratorDevice::Cpu,
        };

        let backend = EmbeddedBackend::load(&config)
            .await
            .expect("AC-1 : load() doit réussir avec un fichier .gguf valide");

        assert!(
            backend.is_available(),
            "AC-1 : is_available() doit retourner true après load()"
        );
        assert_eq!(backend.backend_name(), "test-local");
        assert!(
            !backend.model_id().is_empty(),
            "AC-1 : model_id ne doit pas être vide"
        );

        // ── AC-3 : complete() cohérent ────────────────────────────────────
        let req = CompletionRequest {
            messages: vec![ChatMessage::user("Dis bonjour en un mot.")],
            ..Default::default()
        };

        let response = backend
            .complete(req.clone())
            .await
            .expect("AC-3 : complete() doit réussir");

        assert!(
            response.usage.cost_usd.is_none(),
            "AC-3 : cost_usd doit être None (backend local = gratuit)"
        );
        assert!(
            response.latency_ms > 0,
            "AC-3 : latency_ms doit être supérieur à 0"
        );
        assert!(
            matches!(
                response.finish_reason,
                FinishReason::Stop | FinishReason::Length | FinishReason::ToolCalls
            ),
            "AC-3 : finish_reason doit être Stop, Length ou ToolCalls — got {:?}",
            response.finish_reason
        );
        assert!(
            !response.content.is_empty(),
            "AC-3 : content ne doit pas être vide"
        );

        // ── AC-4 / AC-5 : stream() ────────────────────────────────────────
        use futures::StreamExt;

        let stream = backend
            .stream(req)
            .await
            .expect("AC-4 : stream() doit réussir");

        let chunks: Vec<String> = stream.filter_map(|r| async { r.ok() }).collect().await;

        assert!(
            !chunks.is_empty(),
            "AC-4 : stream() doit retourner au moins un chunk"
        );
        assert!(
            !chunks.join("").is_empty(),
            "AC-4 : la concaténation des chunks ne doit pas être vide"
        );
        // AC-5 : fallback — le contenu complet est retourné en un seul chunk
        assert_eq!(
            chunks.len(),
            1,
            "AC-5 : fallback stream doit retourner exactement 1 chunk (contenu complet)"
        );
    }
}
