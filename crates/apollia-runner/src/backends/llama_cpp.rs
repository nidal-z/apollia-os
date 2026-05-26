//! Wrap llama-cpp-2 pour le runner.
//!
//! Migration STORY-004 depuis `apollia-llm::backends::embedded`. Scope simplifié
//! pour cette première version :
//!
//! - Mono-fichier `.gguf` uniquement (les custom splits FFI seront restaurés en
//!   STORY-011 lors du nettoyage final).
//! - Tooling et grammar GBNF non portés : non requis par le protocole IPC actuel.
//! - Pas d'embeddings : retourne `UnsupportedOperation` (à câbler ultérieurement
//!   quand un backend d'embeddings local sera ajouté).
//!
//! Le code reste compatible avec le protocole IPC v1 défini dans
//! `docs/internal/architecture/IPC-PROTOCOL.md` §3.4-3.7.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures::Stream;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;

use crate::ipc::{
    ChatMessage, CompleteData, CompleteParams, ErrorBody, ErrorCode, FinishReason, LoadModelData,
    LoadModelParams, Role, StreamChunk, Timing, TokenUsage,
};

/// Fallback de sampling — repris à l'identique du backend embedded historique
/// (cf. `apollia-llm::backends::embedded` constantes `DEFAULT_TEMPERATURE` etc.).
const DEFAULT_TOP_P: f32 = 0.95;
const DEFAULT_TOP_K: i32 = 40;
/// Plancher du n_ctx pour qu'un prompt court ait quand même de la marge.
const MIN_CTX_SIZE: u32 = 1024;

/// Entrée d'un modèle chargé en VRAM/RAM.
struct LoadedModel {
    /// Modèle llama.cpp prêt à servir.
    model: Arc<LlamaModel>,
    /// Backend llama.cpp partagé entre tous les modèles (singleton).
    backend: Arc<LlamaBackend>,
    /// `n_ctx` configuré au chargement (informatif, repris dans la réponse).
    context_size: u32,
    /// Estimation grossière de la VRAM utilisée (taille du fichier en MiB).
    memory_used_mb: u32,
}

// SAFETY: `LlamaModel` / `LlamaBackend` du crate llama-cpp-2 sont thread-safe ;
// la création de `LlamaContext` est faite à chaque requête, jamais partagée.
unsafe impl Send for LoadedModel {}
unsafe impl Sync for LoadedModel {}

/// Backend d'inférence in-process via llama.cpp.
///
/// Détient un cache `model_id -> LoadedModel` partagé entre les handlers axum
/// via un `Arc`. Le `LlamaBackend` global est initialisé paresseusement la
/// première fois qu'un modèle est chargé.
pub struct LlamaCppBackend {
    /// Cache des modèles actuellement chargés, indexé par `model_id`.
    models: Mutex<HashMap<String, Arc<LoadedModel>>>,
}

impl Default for LlamaCppBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Singleton global pour `LlamaBackend` : `LlamaBackend::init()` ne peut être
/// appelé qu'une seule fois par process (atomic interne).
static LLAMA_BACKEND: Mutex<Option<Arc<LlamaBackend>>> = Mutex::new(None);

/// Acquiert ou initialise le `LlamaBackend` global (à appeler depuis un thread
/// bloquant uniquement : `LlamaBackend::init()` n'est pas async-safe).
fn acquire_backend() -> Result<Arc<LlamaBackend>, ErrorBody> {
    let mut guard = LLAMA_BACKEND
        .lock()
        .map_err(|e| internal(format!("LLAMA_BACKEND poisoned: {e}")))?;
    if let Some(b) = guard.as_ref() {
        return Ok(b.clone());
    }
    let backend = Arc::new(
        LlamaBackend::init()
            .map_err(|e| internal(format!("llama backend init failed: {e}")))?,
    );
    *guard = Some(backend.clone());
    Ok(backend)
}

fn internal(msg: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::Internal, msg)
}

fn load_failed(msg: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::ModelLoadFailed, msg)
}

fn inference_failed(msg: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorCode::InferenceFailed, msg)
}

impl LlamaCppBackend {
    /// Crée un backend vide : aucun modèle chargé tant que `/llm/load_model`
    /// n'est pas appelé.
    pub fn new() -> Self {
        Self {
            models: Mutex::new(HashMap::new()),
        }
    }

    /// Liste les `model_id` actuellement chargés (utile pour `/health`).
    pub fn loaded_ids(&self) -> Vec<String> {
        let guard = self.models.lock().expect("llama models lock poisoned");
        guard.keys().cloned().collect()
    }

    /// Mémoire totale (MiB) reportée par les modèles chargés.
    pub fn total_memory_mb(&self) -> u32 {
        let guard = self.models.lock().expect("llama models lock poisoned");
        guard.values().map(|m| m.memory_used_mb).sum()
    }

    /// Décharge un modèle. Retourne `true` s'il était chargé.
    pub fn unload(&self, model_id: &str) -> bool {
        let mut guard = self.models.lock().expect("llama models lock poisoned");
        guard.remove(model_id).is_some()
    }

    fn get_model(&self, model_id: &str) -> Option<Arc<LoadedModel>> {
        let guard = self.models.lock().expect("llama models lock poisoned");
        guard.get(model_id).cloned()
    }

    /// Charge un fichier GGUF depuis le disque (`POST /llm/load_model`).
    ///
    /// Le chargement est exécuté dans un `spawn_blocking` car
    /// `LlamaModel::load_from_file` est synchrone et coûteux (mmap + parse).
    pub async fn load_model(&self, params: LoadModelParams) -> Result<LoadModelData, ErrorBody> {
        let started = Instant::now();

        let model_path = params.model_path.clone();
        // Convention IPC : `n_gpu_layers = -1` signifie "toutes les couches
        // sur GPU". On le mappe vers le sentinelle 999 historique utilisé par
        // le backend embedded ; toute valeur >= 0 est conservée.
        let n_gpu_layers: u32 = if params.n_gpu_layers < 0 {
            999
        } else {
            params.n_gpu_layers as u32
        };
        let requested_n_ctx = params.n_ctx.max(MIN_CTX_SIZE);
        let use_mmap = params.use_mmap;
        let use_mlock = params.use_mlock;

        let memory_used_mb = std::fs::metadata(&model_path)
            .map(|m| (m.len() / (1024 * 1024)) as u32)
            .unwrap_or(0);

        let (backend, model) = tokio::task::spawn_blocking(move || -> Result<_, ErrorBody> {
            let backend = acquire_backend()?;

            let mut model_params = LlamaModelParams::default().with_n_gpu_layers(n_gpu_layers);
            model_params = model_params.with_use_mmap(use_mmap);
            model_params = model_params.with_use_mlock(use_mlock);

            let model = LlamaModel::load_from_file(&backend, &model_path, &model_params)
                .map_err(|e| load_failed(format!("model load failed: {e}")))?;

            Ok::<_, ErrorBody>((backend, model))
        })
        .await
        .map_err(|e| internal(format!("model load task panicked: {e}")))??;

        let context_size = clamp_ctx_size(&model, requested_n_ctx);

        let entry = Arc::new(LoadedModel {
            model: Arc::new(model),
            backend,
            context_size,
            memory_used_mb,
        });

        {
            let mut guard = self.models.lock().expect("llama models lock poisoned");
            guard.insert(params.model_id.clone(), entry);
        }

        tracing::info!(
            model_id = %params.model_id,
            load_time_ms = started.elapsed().as_millis() as u64,
            context_size,
            "llama model loaded"
        );

        Ok(LoadModelData {
            model_id: params.model_id,
            load_time_ms: started.elapsed().as_millis() as u64,
            context_size,
            memory_used_mb,
        })
    }

    /// Inférence non-streaming (`POST /llm/complete`).
    pub async fn complete(&self, params: CompleteParams) -> Result<CompleteData, ErrorBody> {
        let entry = self.get_model(&params.model_id).ok_or_else(|| {
            ErrorBody::new(
                ErrorCode::ModelNotLoaded,
                format!("model '{}' not loaded", params.model_id),
            )
        })?;

        let started = Instant::now();
        let max_tokens = params.max_tokens.max(1);
        let temperature = params.temperature;
        let top_p = if params.top_p > 0.0 {
            params.top_p
        } else {
            DEFAULT_TOP_P
        };
        let top_k = if params.top_k > 0 {
            params.top_k as i32
        } else {
            DEFAULT_TOP_K
        };
        let seed = params.seed;
        let messages = params.messages.clone();

        let model = entry.model.clone();
        let backend = entry.backend.clone();
        let context_size = entry.context_size;

        let result = tokio::task::spawn_blocking(move || {
            run_complete(
                &model,
                &backend,
                &messages,
                context_size,
                max_tokens,
                temperature,
                top_p,
                top_k,
                seed,
            )
        })
        .await
        .map_err(|e| internal(format!("complete task panicked: {e}")))??;

        let total_ms = started.elapsed().as_millis() as u64;
        let CompleteRaw {
            text,
            prompt_tokens,
            completion_tokens,
            finish_reason,
            prefill_ms,
            decode_ms,
        } = result;

        Ok(CompleteData {
            text,
            finish_reason,
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
            timing: Timing {
                queue_ms: 0,
                prefill_ms,
                decode_ms,
                total_ms,
            },
        })
    }

    /// Streaming token-by-token (`POST /llm/stream`).
    ///
    /// Renvoie un `Stream` qui fournit :
    /// - `Ok(StreamChunk)` pour chaque token décodé (texte partiel).
    /// - `Ok(StreamChunk { finish_reason: Some(...) })` final pour clôturer.
    /// - `Err(ErrorBody)` si l'inférence échoue.
    pub async fn stream(
        &self,
        params: CompleteParams,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<StreamChunk, ErrorBody>> + Send>>,
        ErrorBody,
    > {
        let entry = self.get_model(&params.model_id).ok_or_else(|| {
            ErrorBody::new(
                ErrorCode::ModelNotLoaded,
                format!("model '{}' not loaded", params.model_id),
            )
        })?;

        let max_tokens = params.max_tokens.max(1);
        let temperature = params.temperature;
        let top_p = if params.top_p > 0.0 {
            params.top_p
        } else {
            DEFAULT_TOP_P
        };
        let top_k = if params.top_k > 0 {
            params.top_k as i32
        } else {
            DEFAULT_TOP_K
        };
        let seed = params.seed;
        let messages = params.messages.clone();

        let model = entry.model.clone();
        let backend = entry.backend.clone();
        let context_size = entry.context_size;

        let (tx, rx) = tokio::sync::mpsc::channel::<Result<StreamChunk, ErrorBody>>(32);

        tokio::task::spawn_blocking(move || {
            let result = run_stream(
                &model,
                &backend,
                &messages,
                context_size,
                max_tokens,
                temperature,
                top_p,
                top_k,
                seed,
                |piece| tx.blocking_send(Ok(StreamChunk {
                    text: piece,
                    finish_reason: None,
                })),
            );

            match result {
                Ok(finish_reason) => {
                    // Emit final chunk with finish_reason set.
                    let _ = tx.blocking_send(Ok(StreamChunk {
                        text: String::new(),
                        finish_reason: Some(finish_reason),
                    }));
                }
                Err(err) => {
                    let _ = tx.blocking_send(Err(err));
                }
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}

/// Borne `n_ctx` entre [`MIN_CTX_SIZE`, `n_ctx_train`] (quand connu).
fn clamp_ctx_size(model: &LlamaModel, requested: u32) -> u32 {
    let trained = model.n_ctx_train();
    let upper = if trained > 0 { trained } else { u32::MAX };
    requested.max(MIN_CTX_SIZE).min(upper)
}

/// Construit le prompt formaté via le template embarqué dans le GGUF (fallback
/// chatml). Retourne aussi le nombre de tokens du prompt.
fn build_prompt(
    model: &LlamaModel,
    messages: &[ChatMessage],
) -> Result<(String, u32), ErrorBody> {
    let template = model.chat_template(None).unwrap_or_else(|_| {
        LlamaChatTemplate::new("chatml").expect("chatml template must be valid")
    });

    let chat_msgs: Vec<LlamaChatMessage> = messages
        .iter()
        .map(|m| {
            let role = match m.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            LlamaChatMessage::new(role.to_string(), m.content.clone())
                .map_err(|e| inference_failed(format!("invalid chat message: {e}")))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let result = model
        .apply_chat_template_with_tools_oaicompat(&template, &chat_msgs, None, None, true)
        .map_err(|e| inference_failed(format!("chat template failed: {e}")))?;

    let prompt = result.prompt;
    let tokens = model
        .str_to_token(&prompt, AddBos::Always)
        .map_err(|e| inference_failed(format!("tokenization failed: {e}")))?;

    Ok((prompt, tokens.len() as u32))
}

/// Construit le sampler de tail (greedy si `temperature <= 0`, sinon stochastique).
fn build_sampler(temperature: f32, top_p: f32, top_k: i32, seed: Option<u64>) -> LlamaSampler {
    if temperature <= 0.0 {
        return LlamaSampler::greedy();
    }
    let resolved_seed = seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    });
    let dist_seed = ((resolved_seed >> 32) as u32) ^ (resolved_seed as u32);

    LlamaSampler::chain_simple([
        LlamaSampler::top_k(top_k),
        LlamaSampler::top_p(top_p, 1),
        LlamaSampler::temp(temperature),
        LlamaSampler::dist(dist_seed),
    ])
}

/// Résultat brut d'une inférence non-streaming.
struct CompleteRaw {
    text: String,
    prompt_tokens: u32,
    completion_tokens: u32,
    finish_reason: FinishReason,
    prefill_ms: u64,
    decode_ms: u64,
}

/// Inférence synchrone complète (appelée depuis `spawn_blocking`).
#[allow(clippy::too_many_arguments)]
fn run_complete(
    model: &LlamaModel,
    backend: &LlamaBackend,
    messages: &[ChatMessage],
    context_size: u32,
    max_tokens: u32,
    temperature: f32,
    top_p: f32,
    top_k: i32,
    seed: Option<u64>,
) -> Result<CompleteRaw, ErrorBody> {
    let (prompt, prompt_tokens) = build_prompt(model, messages)?;
    let tokens = model
        .str_to_token(&prompt, AddBos::Always)
        .map_err(|e| inference_failed(format!("tokenization failed: {e}")))?;

    let needed = prompt_tokens.saturating_add(max_tokens).max(context_size);
    let n_ctx = clamp_ctx_size(model, needed);

    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(n_ctx))
        .with_n_batch(n_ctx);

    let mut ctx = model
        .new_context(backend, ctx_params)
        .map_err(|e| inference_failed(format!("context creation failed: {e}")))?;

    let prefill_start = Instant::now();
    let mut batch = LlamaBatch::new(n_ctx as usize, 1);
    let last_index = tokens.len().saturating_sub(1) as i32;
    for (i, token) in (0_i32..).zip(tokens.into_iter()) {
        batch
            .add(token, i, &[0], i == last_index)
            .map_err(|e| inference_failed(format!("batch add failed: {e}")))?;
    }
    ctx.decode(&mut batch)
        .map_err(|e| inference_failed(format!("initial decode failed: {e}")))?;
    let prefill_ms = prefill_start.elapsed().as_millis() as u64;

    let decode_start = Instant::now();
    let mut n_cur = batch.n_tokens();
    let n_max = n_cur + max_tokens as i32;
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut sampler = build_sampler(temperature, top_p, top_k, seed);
    let mut generated = String::new();
    let mut completion_tokens: u32 = 0;
    let mut finish_reason = FinishReason::Length;

    while n_cur < n_max {
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(token);

        if model.is_eog_token(token) {
            finish_reason = FinishReason::Eos;
            break;
        }

        let piece = model
            .token_to_piece(token, &mut decoder, true, None)
            .unwrap_or_default();
        generated.push_str(&piece);
        completion_tokens += 1;

        batch.clear();
        batch
            .add(token, n_cur, &[0], true)
            .map_err(|e| inference_failed(format!("batch add failed: {e}")))?;
        n_cur += 1;

        ctx.decode(&mut batch)
            .map_err(|e| inference_failed(format!("decode failed: {e}")))?;
    }

    if completion_tokens >= max_tokens {
        finish_reason = FinishReason::Length;
    }

    let decode_ms = decode_start.elapsed().as_millis() as u64;

    Ok(CompleteRaw {
        text: generated,
        prompt_tokens,
        completion_tokens,
        finish_reason,
        prefill_ms,
        decode_ms,
    })
}

/// Inférence streaming : appelle `on_piece` à chaque token décodé, retourne le
/// `FinishReason` final.
#[allow(clippy::too_many_arguments)]
fn run_stream<F>(
    model: &LlamaModel,
    backend: &LlamaBackend,
    messages: &[ChatMessage],
    context_size: u32,
    max_tokens: u32,
    temperature: f32,
    top_p: f32,
    top_k: i32,
    seed: Option<u64>,
    mut on_piece: F,
) -> Result<FinishReason, ErrorBody>
where
    F: FnMut(String) -> Result<(), tokio::sync::mpsc::error::SendError<Result<StreamChunk, ErrorBody>>>,
{
    let (prompt, prompt_tokens) = build_prompt(model, messages)?;
    let tokens = model
        .str_to_token(&prompt, AddBos::Always)
        .map_err(|e| inference_failed(format!("tokenization failed: {e}")))?;

    let needed = prompt_tokens.saturating_add(max_tokens).max(context_size);
    let n_ctx = clamp_ctx_size(model, needed);

    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(n_ctx))
        .with_n_batch(n_ctx);
    let mut ctx = model
        .new_context(backend, ctx_params)
        .map_err(|e| inference_failed(format!("context creation failed: {e}")))?;

    let mut batch = LlamaBatch::new(n_ctx as usize, 1);
    let last_index = tokens.len().saturating_sub(1) as i32;
    for (i, token) in (0_i32..).zip(tokens.into_iter()) {
        batch
            .add(token, i, &[0], i == last_index)
            .map_err(|e| inference_failed(format!("batch add failed: {e}")))?;
    }
    ctx.decode(&mut batch)
        .map_err(|e| inference_failed(format!("initial decode failed: {e}")))?;

    let mut n_cur = batch.n_tokens();
    let n_max = n_cur + max_tokens as i32;
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut sampler = build_sampler(temperature, top_p, top_k, seed);
    let mut completion_tokens: u32 = 0;
    let mut finish_reason = FinishReason::Length;

    while n_cur < n_max {
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(token);

        if model.is_eog_token(token) {
            finish_reason = FinishReason::Eos;
            break;
        }

        let piece = model
            .token_to_piece(token, &mut decoder, true, None)
            .unwrap_or_default();

        if on_piece(piece).is_err() {
            // Receiver dropped : client a abandonné.
            finish_reason = FinishReason::Abort;
            break;
        }
        completion_tokens += 1;

        batch.clear();
        batch
            .add(token, n_cur, &[0], true)
            .map_err(|e| inference_failed(format!("batch add failed: {e}")))?;
        n_cur += 1;

        ctx.decode(&mut batch)
            .map_err(|e| inference_failed(format!("decode failed: {e}")))?;
    }

    if completion_tokens >= max_tokens {
        finish_reason = FinishReason::Length;
    }

    // Silence unused warning on prompt_tokens : conservé pour symétrie avec
    // run_complete (futur : émettre les stats d'usage en fin de stream).
    let _ = prompt_tokens;

    Ok(finish_reason)
}

/// Vérifie qu'un chemin GGUF existe et termine bien par `.gguf`.
pub fn validate_model_path(path: &Path) -> Result<(), ErrorBody> {
    if !path.is_absolute() {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("model_path must be absolute: {}", path.display()),
        ));
    }
    if !path.exists() {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("model_path does not exist: {}", path.display()),
        ));
    }
    match path.extension().and_then(|s| s.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("gguf") => Ok(()),
        _ => Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("model_path must end with .gguf: {}", path.display()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn validate_rejects_relative_path() {
        let p = PathBuf::from("relative.gguf");
        let err = validate_model_path(&p).expect_err("relative path must be rejected");
        assert_eq!(err.code, ErrorCode::BadRequest);
    }

    #[test]
    fn validate_rejects_missing_extension() {
        let p = PathBuf::from("/tmp/foo.bin");
        let err = validate_model_path(&p).expect_err("non-gguf path must be rejected");
        assert_eq!(err.code, ErrorCode::BadRequest);
    }

    #[test]
    fn validate_rejects_nonexistent_path() {
        let p = PathBuf::from("/tmp/does-not-exist-apollia-test.gguf");
        let err = validate_model_path(&p).expect_err("missing file must be rejected");
        assert_eq!(err.code, ErrorCode::BadRequest);
    }

    #[test]
    fn loaded_ids_starts_empty() {
        let backend = LlamaCppBackend::new();
        assert!(backend.loaded_ids().is_empty());
        assert_eq!(backend.total_memory_mb(), 0);
    }
}
