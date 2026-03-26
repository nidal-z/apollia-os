//! Backend d'inférence embarqué — inférence in-process via `llama.cpp`.
//!
//! Ce module est compilé uniquement avec `feature = "local"`.
//!
//! # Architecture (ADR-042)
//!
//! ```text
//! apollia-llm [feature = "local"]
//!   └── EmbeddedBackend : CompletionModel
//!         ├── load()     — charge le .gguf, configure le device, initialise le moteur
//!         ├── complete() — inférence via llama.cpp (batch → decode → sample)
//!         └── stream()   — streaming token-by-token natif
//! ```
//!
//! # Devices supportés
//!
//! | Feature          | Device            | API                                  |
//! |------------------|-------------------|--------------------------------------|
//! | `local` / `local-cpu` | CPU          | défaut, aucune configuration         |
//! | `local-metal`    | Apple Silicon GPU  | `llama-cpp-2/metal`                  |
//! | `local-cuda`     | GPU NVIDIA         | `llama-cpp-2/cuda`                   |
//!
//! # Migration depuis mistralrs
//!
//! Remplace `mistralrs::GgufModelBuilder` par `llama_cpp_2::model::LlamaModel`.
//! Le trait `CompletionModel` et l'API publique sont inchangés.

use std::fmt;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use futures::Stream;

use crate::types::{
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError,
    MessageContent, Role, StreamChunk, TokenUsage, ToolCall, ToolSpec,
};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

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

    /// Retourne le nombre de couches à offloader sur GPU (999 = toutes).
    fn gpu_layers(&self) -> u32 {
        match self {
            Self::Cpu => 0,
            Self::Cuda | Self::Metal => 999,
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
/// model_path   = "~/.apollia/models/Qwen3-8B-Q5_K_M.gguf"
/// quantization = "q5_k_m"
/// device       = "metal"      # "cpu" | "cuda" | "metal"  (défaut: "cpu")
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddedBackendConfig {
    /// Nom logique du backend, utilisé comme clé dans le `LlmRouter`.
    pub name: String,
    /// Chemin complet vers le fichier `.gguf` du modèle sur le disque local.
    pub model_path: PathBuf,
    /// Quantisation du modèle — informatif uniquement, baked in the GGUF filename.
    pub quantization: String,
    /// Accélérateur matériel à utiliser pour l'inférence.
    #[serde(default)]
    pub device: AcceleratorDevice,
}

// ─────────────────────────────────────────────
// Backend
// ─────────────────────────────────────────────

/// Backend d'inférence in-process via llama.cpp — ADR-042.
///
/// Charge un modèle `.gguf` depuis `~/.apollia/models/` via `llama_cpp_2::LlamaModel`
/// et exécute l'inférence directement dans le processus Apollia OS, sans requête HTTP
/// ni processus externe.
///
/// Le fichier `.gguf` est une donnée externe (comme une base SQLite) : il n'est
/// jamais compilé dans le binaire. Le moteur d'inférence (`llama.cpp`), lui,
/// est lié statiquement au binaire lors du build avec `feature = "local"`.
pub struct EmbeddedBackend {
    /// Modèle llama.cpp chargé en mémoire.
    model: Arc<LlamaModel>,
    /// Backend llama.cpp (initialisation globale).
    backend: Arc<LlamaBackend>,
    /// Nom logique du backend, transmis depuis la config.
    name: String,
    /// Identifiant du modèle déduit du nom du fichier `.gguf` (sans extension).
    model_id: String,
    /// Device configuré pour le context (conservé pour les logs de diagnostic).
    #[allow(dead_code)]
    device: AcceleratorDevice,
}

/// `Debug` implémenté manuellement : `LlamaModel` n'implémente pas `Debug`.
impl fmt::Debug for EmbeddedBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddedBackend")
            .field("name", &self.name)
            .field("model_id", &self.model_id)
            .finish_non_exhaustive()
    }
}

// Send + Sync safety: LlamaModel and LlamaBackend are thread-safe.
// The context is created per-request in complete(), not shared.
unsafe impl Send for EmbeddedBackend {}
unsafe impl Sync for EmbeddedBackend {}

/// Expands a leading `~` to `$HOME`.
fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(format!("{}{}", home, &s[1..]))
    } else {
        path.to_path_buf()
    }
}

/// Nombre maximum de tokens à générer par défaut.
const DEFAULT_MAX_TOKENS: u32 = 2048;

/// Taille du contexte par défaut.
const DEFAULT_CTX_SIZE: u32 = 4096;

impl EmbeddedBackend {
    /// Charge le modèle depuis le fichier `.gguf` indiqué dans la config.
    ///
    /// # Erreurs
    ///
    /// - [`LlmError::ModelNotFound`] — `config.model_path` est introuvable sur le disque.
    /// - [`LlmError::DeviceNotAvailable`] — accélérateur non compilé.
    /// - [`LlmError::InferenceError`] — initialisation du moteur échouée.
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

        let expanded_path = expand_tilde(&config.model_path);

        let canonical = expanded_path
            .canonicalize()
            .map_err(|_| LlmError::ModelNotFound {
                path: expanded_path.clone(),
            })?;

        let model_id = canonical
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        let gpu_layers = config.device.gpu_layers();
        let name = config.name.clone();
        let device = config.device.clone();

        // Chargement bloquant dans un thread dédié (llama.cpp est synchrone).
        let (backend, model) = tokio::task::spawn_blocking(move || {
            let backend = LlamaBackend::init()
                .map_err(|e| LlmError::InferenceError(format!("llama backend init failed: {e}")))?;

            let model_params = LlamaModelParams::default().with_n_gpu_layers(gpu_layers);

            let model = LlamaModel::load_from_file(&backend, &canonical, &model_params)
                .map_err(|e| LlmError::InferenceError(format!("model load failed: {e}")))?;

            Ok::<_, LlmError>((backend, model))
        })
        .await
        .map_err(|e| LlmError::InferenceError(format!("model load task failed: {e}")))??;

        tracing::info!(
            backend = %name,
            model_id = %model_id,
            device = %device.label(),
            "modèle local prêt"
        );

        Ok(Self {
            model: Arc::new(model),
            backend: Arc::new(backend),
            name,
            model_id,
            device,
        })
    }

    /// Construit le prompt formaté à partir des messages et outils.
    fn build_prompt(
        model: &LlamaModel,
        req: &CompletionRequest,
    ) -> Result<(String, Option<String>), LlmError> {
        let template = model
            .chat_template(None)
            .unwrap_or_else(|_| {
                LlamaChatTemplate::new("chatml").expect("chatml template must be valid")
            });

        let messages: Vec<LlamaChatMessage> = req
            .messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                };
                let content = match &msg.content {
                    MessageContent::Text(t) => t.clone(),
                    MessageContent::ToolResult { content, .. } => content.clone(),
                    MessageContent::WithToolCalls { text, .. } => text.clone(),
                };
                LlamaChatMessage::new(role.to_string(), content)
                    .map_err(|e| LlmError::InferenceError(format!("invalid chat message: {e}")))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Si des outils sont définis, utiliser le template avec outils.
        let tools_json = if !req.tools.is_empty() {
            Some(build_tools_json(&req.tools))
        } else {
            None
        };

        let result = model
            .apply_chat_template_with_tools_oaicompat(
                &template,
                &messages,
                tools_json.as_deref(),
                None,
                true,
            )
            .map_err(|e| LlmError::InferenceError(format!("chat template failed: {e}")))?;

        Ok((result.prompt, result.grammar))
    }

    /// Exécute l'inférence et retourne le texte généré.
    fn run_inference(
        model: &LlamaModel,
        backend: &LlamaBackend,
        prompt: &str,
        _grammar: Option<&str>,
        max_tokens: u32,
    ) -> Result<String, LlmError> {
        let tokens = model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| LlmError::InferenceError(format!("tokenization failed: {e}")))?;

        let prompt_token_count = tokens.len() as u32;
        let n_ctx = DEFAULT_CTX_SIZE.max(prompt_token_count + max_tokens);

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(n_ctx))
            .with_n_batch(n_ctx);

        let mut ctx = model
            .new_context(backend, ctx_params)
            .map_err(|e| LlmError::InferenceError(format!("context creation failed: {e}")))?;

        let mut batch = LlamaBatch::new(n_ctx as usize, 1);

        // Ajouter les tokens du prompt au batch.
        let last_index = tokens.len().saturating_sub(1) as i32;
        for (i, token) in (0_i32..).zip(tokens.into_iter()) {
            let is_last = i == last_index;
            batch
                .add(token, i, &[0], is_last)
                .map_err(|e| LlmError::InferenceError(format!("batch add failed: {e}")))?;
        }

        ctx.decode(&mut batch)
            .map_err(|e| LlmError::InferenceError(format!("initial decode failed: {e}")))?;

        // Boucle de génération token-by-token.
        let mut n_cur = batch.n_tokens();
        let n_max = n_cur + max_tokens as i32;
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut sampler = LlamaSampler::greedy();
        let mut generated = String::new();

        while n_cur < n_max {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if model.is_eog_token(token) {
                break;
            }

            let piece = model
                .token_to_piece(token, &mut decoder, true, None)
                .unwrap_or_default();
            generated.push_str(&piece);

            batch.clear();
            batch
                .add(token, n_cur, &[0], true)
                .map_err(|e| LlmError::InferenceError(format!("batch add failed: {e}")))?;
            n_cur += 1;

            ctx.decode(&mut batch)
                .map_err(|e| LlmError::InferenceError(format!("decode failed: {e}")))?;
        }

        Ok(generated)
    }
}

/// Convertit les `ToolSpec` en JSON compatible OpenAI tools format.
fn build_tools_json(tools: &[ToolSpec]) -> String {
    let tools_array: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters,
                }
            })
        })
        .collect();
    serde_json::to_string(&tools_array).unwrap_or_else(|_| "[]".to_string())
}

/// Tente de parser des tool calls depuis la réponse générée.
///
/// Les modèles retournent les tool calls dans des formats variés. On cherche
/// un JSON array ou object contenant `name` + `arguments`.
fn parse_tool_calls(text: &str) -> Vec<ToolCall> {
    // Cherche un bloc JSON dans la réponse (entre { et } ou [ et ]).
    let trimmed = text.trim();

    // Essai 1 : la réponse entière est un JSON
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return extract_tool_calls_from_json(&value);
    }

    // Essai 2 : chercher un bloc JSON dans le texte
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&trimmed[start..=end]) {
                return extract_tool_calls_from_json(&value);
            }
        }
    }

    Vec::new()
}

/// Extrait les tool calls depuis une valeur JSON parsée.
fn extract_tool_calls_from_json(value: &serde_json::Value) -> Vec<ToolCall> {
    let mut calls = Vec::new();

    // Format OpenAI : {"tool_calls": [{"function": {"name": ..., "arguments": ...}}]}
    if let Some(arr) = value.get("tool_calls").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(func) = item.get("function") {
                if let (Some(name), Some(args)) = (
                    func.get("name").and_then(|n| n.as_str()),
                    func.get("arguments"),
                ) {
                    let arguments = if args.is_string() {
                        serde_json::from_str(args.as_str().unwrap_or("{}"))
                            .unwrap_or(serde_json::Value::Object(Default::default()))
                    } else {
                        args.clone()
                    };
                    calls.push(ToolCall {
                        id: item
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("call_0")
                            .to_string(),
                        name: name.to_string(),
                        arguments,
                    });
                }
            }
        }
    }

    // Format direct : {"name": ..., "arguments": ...}
    if calls.is_empty() {
        if let (Some(name), Some(args)) = (
            value.get("name").and_then(|n| n.as_str()),
            value.get("arguments"),
        ) {
            let arguments = if args.is_string() {
                serde_json::from_str(args.as_str().unwrap_or("{}"))
                    .unwrap_or(serde_json::Value::Object(Default::default()))
            } else {
                args.clone()
            };
            calls.push(ToolCall {
                id: "call_0".to_string(),
                name: name.to_string(),
                arguments,
            });
        }
    }

    calls
}

#[async_trait::async_trait]
impl CompletionModel for EmbeddedBackend {
    /// Exécute l'inférence in-process et retourne la réponse complète.
    ///
    /// `usage.cost_usd` est toujours `None` — l'inférence locale est gratuite.
    async fn complete(&self, req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let started = Instant::now();
        let max_tokens = req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
        let model = Arc::clone(&self.model);
        let backend = Arc::clone(&self.backend);
        let has_tools = !req.tools.is_empty();

        // Build prompt on the current thread (fast, no I/O).
        let (prompt, grammar) = Self::build_prompt(&model, &req)?;

        let prompt_tokens = model
            .str_to_token(&prompt, AddBos::Always)
            .map_err(|e| LlmError::InferenceError(format!("tokenization failed: {e}")))?
            .len() as u32;

        // Run inference in a blocking thread (llama.cpp is synchronous).
        let generated = tokio::task::spawn_blocking(move || {
            Self::run_inference(&model, &backend, &prompt, grammar.as_deref(), max_tokens)
        })
        .await
        .map_err(|e| LlmError::InferenceError(format!("inference task failed: {e}")))??;

        let completion_tokens = generated.len() as u32 / 4; // Approximation

        // Parse tool calls si le modèle a des outils configurés.
        let tool_calls = if has_tools {
            parse_tool_calls(&generated)
        } else {
            Vec::new()
        };

        let finish_reason = if !tool_calls.is_empty() {
            FinishReason::ToolCalls
        } else {
            FinishReason::Stop
        };

        Ok(CompletionResponse {
            content: generated,
            tool_calls,
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
                cost_usd: None,
            },
            finish_reason,
            latency_ms: started.elapsed().as_millis() as u64,
        })
    }

    /// Retourne un stream de chunks texte — streaming token-by-token natif.
    ///
    /// # Implémentation
    ///
    /// Utilise un `tokio::sync::mpsc` channel pour bridger le décodage synchrone
    /// de llama.cpp vers un `futures::Stream` asynchrone. Chaque token décodé
    /// est envoyé dans le channel dès qu'il est disponible.
    async fn stream(
        &self,
        req: CompletionRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError> {
        let max_tokens = req.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
        let model = Arc::clone(&self.model);
        let backend = Arc::clone(&self.backend);

        let (prompt, _grammar) = Self::build_prompt(&model, &req)?;

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<StreamChunk, LlmError>>(32);

        // Spawn blocking inference in a dedicated thread.
        tokio::task::spawn_blocking(move || {
            let result = (|| -> Result<(), LlmError> {
                let tokens = model
                    .str_to_token(&prompt, AddBos::Always)
                    .map_err(|e| LlmError::InferenceError(format!("tokenization failed: {e}")))?;

                let prompt_token_count = tokens.len() as u32;
                let n_ctx = DEFAULT_CTX_SIZE.max(prompt_token_count + max_tokens);
                let ctx_params = LlamaContextParams::default()
                    .with_n_ctx(NonZeroU32::new(n_ctx))
                    .with_n_batch(n_ctx);

                let mut ctx = model
                    .new_context(&backend, ctx_params)
                    .map_err(|e| {
                        LlmError::InferenceError(format!("context creation failed: {e}"))
                    })?;

                let mut batch = LlamaBatch::new(n_ctx as usize, 1);
                let last_index = tokens.len().saturating_sub(1) as i32;
                for (i, token) in (0_i32..).zip(tokens.into_iter()) {
                    batch
                        .add(token, i, &[0], i == last_index)
                        .map_err(|e| {
                            LlmError::InferenceError(format!("batch add failed: {e}"))
                        })?;
                }

                ctx.decode(&mut batch).map_err(|e| {
                    LlmError::InferenceError(format!("initial decode failed: {e}"))
                })?;

                let mut n_cur = batch.n_tokens();
                let n_max = n_cur + max_tokens as i32;
                let mut decoder = encoding_rs::UTF_8.new_decoder();
                let mut sampler = LlamaSampler::greedy();

                while n_cur < n_max {
                    let token = sampler.sample(&ctx, batch.n_tokens() - 1);
                    sampler.accept(token);

                    if model.is_eog_token(token) {
                        break;
                    }

                    let piece = model
                        .token_to_piece(token, &mut decoder, true, None)
                        .unwrap_or_default();

                    if tx.blocking_send(Ok(StreamChunk::Text(piece))).is_err() {
                        break; // Receiver dropped
                    }

                    batch.clear();
                    batch.add(token, n_cur, &[0], true).map_err(|e| {
                        LlmError::InferenceError(format!("batch add failed: {e}"))
                    })?;
                    n_cur += 1;

                    ctx.decode(&mut batch).map_err(|e| {
                        LlmError::InferenceError(format!("decode failed: {e}"))
                    })?;
                }

                Ok(())
            })();

            if let Err(e) = result {
                let _ = tx.blocking_send(Err(e));
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    /// Retourne `true` : le moteur est chargé en mémoire et prêt.
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

    #[test]
    fn test_accelerator_device_default_is_cpu() {
        assert_eq!(AcceleratorDevice::default(), AcceleratorDevice::Cpu);
    }

    #[test]
    fn test_accelerator_device_serde_roundtrip() {
        let cases = [
            (AcceleratorDevice::Cpu, "\"cpu\""),
            (AcceleratorDevice::Cuda, "\"cuda\""),
            (AcceleratorDevice::Metal, "\"metal\""),
        ];
        for (device, expected_json) in &cases {
            let json = serde_json::to_string(device).expect("sérialisation doit réussir");
            assert_eq!(&json, expected_json);
            let back: AcceleratorDevice = serde_json::from_str(&json).expect("désérialisation");
            assert_eq!(&back, device);
        }
    }

    #[test]
    fn test_accelerator_device_cpu_always_available() {
        assert!(AcceleratorDevice::Cpu.check_compiled().is_ok());
    }

    #[cfg(not(feature = "local-cuda"))]
    #[test]
    fn test_accelerator_device_cuda_not_compiled() {
        let result = AcceleratorDevice::Cuda.check_compiled();
        assert!(matches!(
            result,
            Err(LlmError::DeviceNotAvailable { ref device, .. }) if device == "cuda"
        ));
    }

    #[cfg(feature = "local-metal")]
    #[test]
    fn test_accelerator_device_metal_compiled() {
        assert!(AcceleratorDevice::Metal.check_compiled().is_ok());
    }

    #[cfg(not(feature = "local-metal"))]
    #[test]
    fn test_accelerator_device_metal_not_compiled() {
        let result = AcceleratorDevice::Metal.check_compiled();
        assert!(matches!(
            result,
            Err(LlmError::DeviceNotAvailable { ref device, .. }) if device == "metal"
        ));
    }

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
        assert_eq!(config.device, AcceleratorDevice::Cpu);
    }

    #[test]
    fn test_gpu_layers() {
        assert_eq!(AcceleratorDevice::Cpu.gpu_layers(), 0);
        assert_eq!(AcceleratorDevice::Metal.gpu_layers(), 999);
        assert_eq!(AcceleratorDevice::Cuda.gpu_layers(), 999);
    }

    #[test]
    fn test_parse_tool_calls_empty() {
        assert!(parse_tool_calls("Hello world").is_empty());
    }

    #[test]
    fn test_parse_tool_calls_direct_format() {
        let json = r#"{"name": "get_weather", "arguments": {"city": "Paris"}}"#;
        let calls = parse_tool_calls(json);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments["city"], "Paris");
    }

    #[test]
    fn test_parse_tool_calls_openai_format() {
        let json = r#"{"tool_calls": [{"id": "call_1", "function": {"name": "file_io", "arguments": "{\"path\": \"/tmp\"}"}}]}"#;
        let calls = parse_tool_calls(json);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "file_io");
        assert_eq!(calls[0].id, "call_1");
    }

    #[test]
    fn test_build_tools_json() {
        let tools = vec![ToolSpec {
            name: "test_tool".into(),
            description: "A test".into(),
            parameters: serde_json::json!({"type": "object"}),
        }];
        let json = build_tools_json(&tools);
        assert!(json.contains("test_tool"));
        assert!(json.contains("function"));
    }

    #[tokio::test]
    async fn test_load_returns_model_not_found() {
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
}
