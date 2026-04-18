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
    CompletionModel, CompletionRequest, CompletionResponse, FinishReason, LlmError, MessageContent,
    Role, StreamChunk, TokenUsage, ToolCall, ToolSpec,
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
/// Exactement un des deux champs [`Self::model_path`] ou [`Self::model_paths`]
/// doit être renseigné. Les deux à la fois ou aucun des deux provoquent une
/// [`LlmError::ConfigConflict`] lors de [`EmbeddedBackend::load`].
///
/// # Exemples TOML
///
/// Mono-fichier ou split standard (`<prefix>-NNNNN-of-NNNNN.gguf`) :
///
/// ```toml
/// [[llm.backends]]
/// name         = "local"
/// type         = "embedded"
/// model_path   = "~/.apollia/models/Qwen3-8B-Q5_K_M.gguf"
/// quantization = "q5_k_m"
/// device       = "metal"      # "cpu" | "cuda" | "metal"  (défaut: "cpu")
/// ```
///
/// Naming scheme custom — liste ordonnée, chargée via
/// `llama_model_load_from_splits` :
///
/// ```toml
/// [[llm.backends]]
/// name         = "custom-split"
/// type         = "embedded"
/// quantization = "q4_k_m"
/// device       = "cpu"
/// model_paths  = [
///   "~/models/foo_part1.gguf",
///   "~/models/foo_part2.gguf",
///   "~/models/foo_part3.gguf",
/// ]
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbeddedBackendConfig {
    /// Nom logique du backend, utilisé comme clé dans le `LlmRouter`.
    pub name: String,
    /// Chemin complet vers le fichier `.gguf` mono-fichier OU le premier shard
    /// d'un split standard `<prefix>-NNNNN-of-NNNNN.gguf`.
    ///
    /// Mutuellement exclusif avec [`Self::model_paths`]. Au moins un des deux
    /// doit être renseigné — une config sans ni l'un ni l'autre est refusée
    /// par [`EmbeddedBackend::load`] avec [`LlmError::ConfigConflict`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_path: Option<PathBuf>,
    /// Liste explicite et ordonnée de chemins de shards GGUF — utilisée quand le
    /// naming scheme ne suit pas le pattern standard `<prefix>-NNNNN-of-NNNNN.gguf`.
    ///
    /// Transmise à `llama_model_load_from_splits` via FFI direct. Les chemins
    /// sont canonisés individuellement ; ils doivent tous exister. L'ordre est
    /// conservé (il correspond à l'ordre logique des shards dans le modèle).
    ///
    /// Mutuellement exclusif avec [`Self::model_path`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_paths: Option<Vec<PathBuf>>,
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

/// Longueur du suffixe `NNNNN-of-NNNNN` (sans le tiret de séparation ni le `.gguf`).
const SHARD_SUFFIX_LEN: usize = 14;

/// Métadonnées extraites d'un nom de fichier GGUF shard standard.
///
/// Pattern reconnu : `<prefix>-NNNNN-of-MMMMM.gguf` (5 chiffres zero-padded).
/// `prefix` ne contient PAS le suffixe `-NNNNN-of-MMMMM`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SplitInfo {
    /// Préfixe commun utilisé comme `model_id` et pour reconstruire les chemins.
    prefix: String,
    /// Nombre total de shards (N dans `-of-NNNNN`).
    total: u32,
}

/// Parse le nom de fichier brut pour y détecter le pattern shard standard
/// `<prefix>-NNNNN-of-MMMMM.gguf`.
///
/// Retourne `None` pour tout autre format (mono-fichier, pattern non zero-padded,
/// extension absente, etc.). Ne touche pas le filesystem.
fn parse_shard_file_name(file_name: &str) -> Option<(String, u32, u32)> {
    let stem = file_name.strip_suffix(".gguf")?;
    if stem.len() < SHARD_SUFFIX_LEN + 2 {
        return None;
    }
    let split_at = stem.len() - SHARD_SUFFIX_LEN;
    let (prefix_with_dash, trailing) = stem.split_at(split_at);
    let prefix = prefix_with_dash.strip_suffix('-')?;
    if prefix.is_empty() {
        return None;
    }
    let (idx_str, rest) = trailing.split_at(5);
    let total_str = rest.strip_prefix("-of-")?;
    if total_str.len() != 5 {
        return None;
    }
    let index = idx_str.parse::<u32>().ok()?;
    let total = total_str.parse::<u32>().ok()?;
    Some((prefix.to_string(), index, total))
}

/// Détecte si `path` correspond au premier shard d'un modèle GGUF split standard
/// et vérifie que tous les shards attendus existent dans le même dossier.
///
/// Retours :
/// - `Ok(None)` → `path` ne matche pas le pattern (mono-fichier, comportement legacy).
/// - `Ok(Some(SplitInfo))` → tous les shards existent, llama.cpp peut charger.
/// - `Err(LlmError::ShardIndexNotFirst)` → `path` matche mais ne pointe pas sur le shard 00001.
/// - `Err(LlmError::ModelShardMissing)` → série incomplète.
///
/// Le nom de fichier est parsé manuellement (pas de crate `regex`) pour garder
/// la dépendance zéro. Le pattern équivalent est `^(.+)-(\d{5})-of-(\d{5})\.gguf$`.
fn detect_split_shards(path: &Path) -> Result<Option<SplitInfo>, LlmError> {
    let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
        return Ok(None);
    };
    let Some((prefix, given_index, total)) = parse_shard_file_name(file_name) else {
        return Ok(None);
    };

    if given_index != 1 {
        return Err(LlmError::ShardIndexNotFirst {
            given_index,
            path: path.to_path_buf(),
        });
    }

    let Some(dir) = path.parent() else {
        return Err(LlmError::ModelNotFound {
            path: path.to_path_buf(),
        });
    };

    let mut found = 0usize;
    let mut first_missing: Option<PathBuf> = None;
    for i in 1..=total {
        let shard_name = format!("{prefix}-{i:05}-of-{total:05}.gguf");
        let shard_path = dir.join(&shard_name);
        if shard_path.exists() {
            found += 1;
        } else if first_missing.is_none() {
            first_missing = Some(shard_path);
        }
    }

    if let Some(expected) = first_missing {
        return Err(LlmError::ModelShardMissing {
            prefix,
            expected,
            total: total as usize,
            found,
        });
    }

    Ok(Some(SplitInfo { prefix, total }))
}

/// Nombre maximum de tokens à générer par défaut.
const DEFAULT_MAX_TOKENS: u32 = 2048;

/// Taille du contexte par défaut.
const DEFAULT_CTX_SIZE: u32 = 4096;

/// Singleton global pour le backend llama.cpp.
///
/// `LlamaBackend::init()` ne peut être appelé qu'une seule fois dans le processus
/// (AtomicBool interne). On stocke le résultat dans un `Mutex<Option<>>` pour le
/// réutiliser lors des recharges de modèle (`reload_llm`).
static LLAMA_BACKEND: std::sync::Mutex<Option<Arc<LlamaBackend>>> = std::sync::Mutex::new(None);

/// Acquiert (ou initialise une seule fois) le `LlamaBackend` global.
///
/// Doit être appelé depuis un thread bloquant — `LlamaBackend::init()` lit des
/// ressources système (backends GPU/CPU) et n'est pas conçu pour être exécuté
/// sur une runtime async.
fn acquire_backend() -> Result<Arc<LlamaBackend>, LlmError> {
    let mut guard = LLAMA_BACKEND
        .lock()
        .map_err(|e| LlmError::InferenceError(format!("LLAMA_BACKEND poisoned: {e}")))?;
    if let Some(existing) = guard.as_ref() {
        return Ok(existing.clone());
    }
    let b = Arc::new(
        LlamaBackend::init()
            .map_err(|e| LlmError::InferenceError(format!("llama backend init failed: {e}")))?,
    );
    *guard = Some(b.clone());
    Ok(b)
}

/// Charge un modèle depuis une liste explicite de chemins C via FFI direct
/// vers `llama_cpp_sys_2::llama_model_load_from_splits`.
///
/// # Safety
///
/// - `ptrs` doit contenir des pointeurs non-null vers des chaînes C valides
///   (terminées par `\0`), vivantes pour toute la durée de l'appel.
/// - Les chemins doivent désigner des fichiers GGUF existants et lisibles —
///   la validation est faite par l'appelant avant de construire les pointeurs.
/// - Le pointeur `*mut llama_model` retourné est potentiellement null ;
///   cette fonction le vérifie avant de construire le wrapper.
/// - Le wrapper [`LlamaModel`] est `#[repr(transparent)]` au-dessus d'un
///   `NonNull<llama_cpp_sys_2::llama_model>` (vérifié par un `const` assert
///   ci-dessous). Le `transmute` respecte donc la représentation mémoire et
///   produit un wrapper qui appellera `llama_model_free` à son `Drop`.
fn load_model_from_splits_ffi(
    ptrs: &mut [*const std::os::raw::c_char],
    gpu_layers: u32,
) -> Result<LlamaModel, LlmError> {
    use std::ptr::NonNull;

    const _: () = {
        assert!(
            std::mem::size_of::<LlamaModel>()
                == std::mem::size_of::<NonNull<llama_cpp_sys_2::llama_model>>(),
        );
        assert!(
            std::mem::align_of::<LlamaModel>()
                == std::mem::align_of::<NonNull<llama_cpp_sys_2::llama_model>>(),
        );
    };

    // SAFETY: `llama_model_default_params` is a pure accessor returning a
    // value-type struct — no state, no invariants to uphold.
    let mut params = unsafe { llama_cpp_sys_2::llama_model_default_params() };
    params.n_gpu_layers = gpu_layers as i32;

    // SAFETY:
    // - `ptrs.as_mut_ptr()` points to `ptrs.len()` valid `*const c_char`
    //   entries owned by the caller (backed by `CString`s kept alive for the
    //   duration of the call).
    // - `params` is a POD struct initialised by `llama_model_default_params`
    //   plus a single integer field override.
    let raw_model = unsafe {
        llama_cpp_sys_2::llama_model_load_from_splits(ptrs.as_mut_ptr(), ptrs.len(), params)
    };

    let non_null = NonNull::new(raw_model).ok_or_else(|| {
        LlmError::InferenceError("llama_model_load_from_splits returned a null pointer".into())
    })?;

    // SAFETY: `LlamaModel` is `#[repr(transparent)]` over
    // `NonNull<llama_cpp_sys_2::llama_model>` (both layout-asserted above).
    // The pointer is non-null and ownership transfers to the wrapper, whose
    // `Drop` calls `llama_model_free`.
    let model: LlamaModel = unsafe { std::mem::transmute(non_null) };
    Ok(model)
}

impl EmbeddedBackend {
    /// Charge le modèle selon la config fournie.
    ///
    /// Deux chemins d'exécution :
    /// - [`EmbeddedBackendConfig::model_path`] (mono-fichier ou premier shard d'un
    ///   split standard `<prefix>-NNNNN-of-NNNNN.gguf`) — délégué à
    ///   `llama_cpp_2::LlamaModel::load_from_file`, qui détecte automatiquement
    ///   les shards suivants du pattern standard.
    /// - [`EmbeddedBackendConfig::model_paths`] (liste ordonnée explicite) —
    ///   chaque chemin est vérifié puis transmis à
    ///   `llama_cpp_sys_2::llama_model_load_from_splits` via FFI direct pour
    ///   supporter les naming schemes custom.
    ///
    /// Les deux champs sont mutuellement exclusifs (fail-fast au démarrage).
    ///
    /// # Erreurs
    ///
    /// - [`LlmError::ConfigConflict`] — `model_path` et `model_paths` tous les
    ///   deux renseignés, ni l'un ni l'autre, ou `model_paths` vide.
    /// - [`LlmError::ModelNotFound`] — un chemin est introuvable sur le disque.
    /// - [`LlmError::ModelShardMissing`] / [`LlmError::ShardIndexNotFirst`] —
    ///   série split standard incomplète ou entrée pointant sur un shard autre
    ///   que `-00001-of-NNNNN.gguf`.
    /// - [`LlmError::DeviceNotAvailable`] — accélérateur non compilé.
    /// - [`LlmError::InferenceError`] — initialisation du moteur échouée.
    pub async fn load(config: &EmbeddedBackendConfig) -> Result<Self, LlmError> {
        // Fail-fast : vérifie que l'accélérateur demandé est compilé dans ce binaire.
        config.device.check_compiled()?;

        match (&config.model_path, &config.model_paths) {
            (Some(_), Some(_)) => Err(LlmError::ConfigConflict {
                backend: config.name.clone(),
                reason: "model_path et model_paths sont mutuellement exclusifs".into(),
            }),
            (None, None) => Err(LlmError::ConfigConflict {
                backend: config.name.clone(),
                reason: "model_path OU model_paths est requis".into(),
            }),
            (None, Some(paths)) if paths.is_empty() => Err(LlmError::ConfigConflict {
                backend: config.name.clone(),
                reason: "model_paths ne peut pas être vide".into(),
            }),
            (Some(path), None) => Self::load_single_or_standard_split(config, path).await,
            (None, Some(paths)) => Self::load_custom_splits(config, paths).await,
        }
    }

    /// Chemin mono-fichier ou split standard `<prefix>-NNNNN-of-NNNNN.gguf`.
    async fn load_single_or_standard_split(
        config: &EmbeddedBackendConfig,
        raw_path: &Path,
    ) -> Result<Self, LlmError> {
        tracing::info!(
            backend = %config.name,
            path = %raw_path.display(),
            quantization = %config.quantization,
            device = %config.device.label(),
            "chargement du modèle local"
        );

        let expanded_path = expand_tilde(raw_path);
        let canonical = expanded_path
            .canonicalize()
            .map_err(|_| LlmError::ModelNotFound {
                path: expanded_path.clone(),
            })?;

        let split = detect_split_shards(&canonical)?;

        let model_id = match &split {
            Some(info) => {
                tracing::info!(
                    backend = %config.name,
                    shards = info.total,
                    prefix = %info.prefix,
                    "GGUF split model detected (llama.cpp loads remaining shards automatically)"
                );
                info.prefix.clone()
            }
            None => canonical
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
        };

        let gpu_layers = config.device.gpu_layers();
        let name = config.name.clone();
        let device = config.device.clone();

        // Chargement bloquant dans un thread dédié (llama.cpp est synchrone).
        let (backend, model) = tokio::task::spawn_blocking(move || {
            let backend = acquire_backend()?;

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
            backend,
            name,
            model_id,
            device,
        })
    }

    /// Chemin custom-splits : FFI direct vers `llama_model_load_from_splits`.
    ///
    /// Canonise chaque chemin, vérifie son existence avant tout appel FFI
    /// (Principe #4 — Fail fast), puis exécute le chargement dans un
    /// `spawn_blocking` (llama.cpp est synchrone).
    async fn load_custom_splits(
        config: &EmbeddedBackendConfig,
        raw_paths: &[PathBuf],
    ) -> Result<Self, LlmError> {
        let mut canonical_paths: Vec<PathBuf> = Vec::with_capacity(raw_paths.len());
        for p in raw_paths {
            let expanded = expand_tilde(p);
            let canonical = expanded
                .canonicalize()
                .map_err(|_| LlmError::ModelNotFound {
                    path: expanded.clone(),
                })?;
            canonical_paths.push(canonical);
        }

        let Some(first) = canonical_paths.first().cloned() else {
            return Err(LlmError::ConfigConflict {
                backend: config.name.clone(),
                reason: "model_paths ne peut pas être vide".into(),
            });
        };
        let model_id = first
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        tracing::info!(
            backend = %config.name,
            paths_count = canonical_paths.len(),
            first_path = %first.display(),
            quantization = %config.quantization,
            device = %config.device.label(),
            "chargement modèle GGUF custom splits via FFI llama_model_load_from_splits"
        );

        let gpu_layers = config.device.gpu_layers();
        let name = config.name.clone();
        let device = config.device.clone();

        let (backend, model) = tokio::task::spawn_blocking(move || -> Result<_, LlmError> {
            let backend = acquire_backend()?;

            let c_strings: Vec<std::ffi::CString> = canonical_paths
                .iter()
                .map(|p| {
                    let s = p.to_str().ok_or_else(|| {
                        LlmError::InferenceError(format!("non-UTF-8 path: {}", p.display()))
                    })?;
                    std::ffi::CString::new(s).map_err(|e| {
                        LlmError::InferenceError(format!("CString conversion failed: {e}"))
                    })
                })
                .collect::<Result<_, _>>()?;
            let mut c_ptrs: Vec<*const std::os::raw::c_char> =
                c_strings.iter().map(|c| c.as_ptr()).collect();

            let model = load_model_from_splits_ffi(&mut c_ptrs, gpu_layers)?;
            // Keep `c_strings` alive until the FFI call returns — they back the pointers.
            drop(c_strings);

            Ok::<_, LlmError>((backend, model))
        })
        .await
        .map_err(|e| LlmError::InferenceError(format!("model load task failed: {e}")))??;

        tracing::info!(
            backend = %name,
            model_id = %model_id,
            device = %device.label(),
            "modèle local prêt (custom splits)"
        );

        Ok(Self {
            model: Arc::new(model),
            backend,
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
        let template = model.chat_template(None).unwrap_or_else(|_| {
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
                    // Re-embed the tool call markup so the chat template sees a valid
                    // multi-turn history.  Without this, the assistant turn would be
                    // empty and the template would fail or produce a malformed prompt
                    // on the second (post-tool) LLM call.
                    MessageContent::WithToolCalls { text, tool_calls } => {
                        let mut c = text.clone();
                        for call in tool_calls {
                            let call_json = serde_json::json!({
                                "name": call.name,
                                "arguments": call.arguments,
                            });
                            let json_str = serde_json::to_string(&call_json)
                                .unwrap_or_else(|_| "{}".to_string());
                            if !c.is_empty() && !c.ends_with('\n') {
                                c.push('\n');
                            }
                            c.push_str(&format!("<tool_call>\n{json_str}\n</tool_call>"));
                        }
                        c
                    }
                    // Include the call ID so the template can correlate the result
                    // with its originating tool call.
                    MessageContent::ToolResult {
                        tool_call_id,
                        content,
                    } => serde_json::json!({
                        "tool_call_id": tool_call_id,
                        "content": content,
                    })
                    .to_string(),
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

        // Grammar désactivée systématiquement : les modèles thinking (ex. Qwen3)
        // génèrent des tokens <think>...</think> avant la réponse, que la grammar
        // JSON tool-call rejette immédiatement. Toutes les stacks se vident et
        // llama.cpp crashe avec GGML_ASSERT(!stacks.empty()).
        // Le parsing post-hoc via parse_tool_calls() suffit à extraire les tool calls.
        Ok((result.prompt, None))
    }

    /// Exécute l'inférence et retourne le texte généré.
    ///
    /// Quand `grammar` est `Some`, applique la contrainte GBNF via `LlamaSampler::grammar`
    /// enchaîné avec le sampler glouton, ce qui force le modèle à produire un JSON valide.
    fn run_inference(
        model: &LlamaModel,
        backend: &LlamaBackend,
        prompt: &str,
        grammar: Option<&str>,
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
        let mut sampler = match grammar {
            Some(grammar_str) => {
                let grammar_sampler =
                    LlamaSampler::grammar(model, grammar_str, "root").map_err(|e| {
                        LlmError::InferenceError(format!("grammar sampler init failed: {e}"))
                    })?;
                LlamaSampler::chain_simple([grammar_sampler, LlamaSampler::greedy()])
            }
            None => LlamaSampler::greedy(),
        };
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
                ..Default::default()
            },
            finish_reason,
            latency_ms: started.elapsed().as_millis() as u64,
            ttft_ms: None,
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

        let (prompt, grammar) = Self::build_prompt(&model, &req)?;

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

                let mut ctx = model.new_context(&backend, ctx_params).map_err(|e| {
                    LlmError::InferenceError(format!("context creation failed: {e}"))
                })?;

                let mut batch = LlamaBatch::new(n_ctx as usize, 1);
                let last_index = tokens.len().saturating_sub(1) as i32;
                for (i, token) in (0_i32..).zip(tokens.into_iter()) {
                    batch
                        .add(token, i, &[0], i == last_index)
                        .map_err(|e| LlmError::InferenceError(format!("batch add failed: {e}")))?;
                }

                ctx.decode(&mut batch)
                    .map_err(|e| LlmError::InferenceError(format!("initial decode failed: {e}")))?;

                let mut n_cur = batch.n_tokens();
                let n_max = n_cur + max_tokens as i32;
                let mut decoder = encoding_rs::UTF_8.new_decoder();
                let mut sampler = match grammar.as_deref() {
                    Some(grammar_str) => {
                        let gs =
                            LlamaSampler::grammar(&model, grammar_str, "root").map_err(|e| {
                                LlmError::InferenceError(format!(
                                    "grammar sampler init failed: {e}"
                                ))
                            })?;
                        LlamaSampler::chain_simple([gs, LlamaSampler::greedy()])
                    }
                    None => LlamaSampler::greedy(),
                };

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
                    batch
                        .add(token, n_cur, &[0], true)
                        .map_err(|e| LlmError::InferenceError(format!("batch add failed: {e}")))?;
                    n_cur += 1;

                    ctx.decode(&mut batch)
                        .map_err(|e| LlmError::InferenceError(format!("decode failed: {e}")))?;
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
        assert_eq!(config.model_path, Some(PathBuf::from("/tmp/model.gguf")));
        assert!(config.model_paths.is_none());
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
            model_path: Some(PathBuf::from("/tmp/does-not-exist-xyz-apollia.gguf")),
            model_paths: None,
            quantization: "q4_k_m".into(),
            device: AcceleratorDevice::Cpu,
        };
        let result = EmbeddedBackend::load(&config).await;
        assert!(
            matches!(result, Err(LlmError::ModelNotFound { .. })),
            "expected ModelNotFound, got: {result:?}"
        );
    }

    #[test]
    fn test_parse_shard_file_name_matches_standard_pattern() {
        let parsed = parse_shard_file_name("Llama-70B-Q5_K_M-00001-of-00003.gguf");
        assert_eq!(parsed, Some(("Llama-70B-Q5_K_M".to_string(), 1, 3)));
    }

    #[test]
    fn test_parse_shard_file_name_matches_single_shard_edge_case() {
        let parsed = parse_shard_file_name("Foo-00001-of-00001.gguf");
        assert_eq!(parsed, Some(("Foo".to_string(), 1, 1)));
    }

    #[test]
    fn test_parse_shard_file_name_rejects_non_gguf_extension() {
        assert_eq!(
            parse_shard_file_name("model-00001-of-00003.safetensors"),
            None
        );
    }

    #[test]
    fn test_parse_shard_file_name_rejects_mono_file() {
        assert_eq!(parse_shard_file_name("Qwen3-8B-Q5_K_M.gguf"), None);
    }

    #[test]
    fn test_parse_shard_file_name_rejects_non_zero_padded_numbers() {
        let cases = [
            "model-1-of-3.gguf",
            "model-00001-of-3.gguf",
            "model-1-of-00003.gguf",
        ];
        for name in cases {
            assert_eq!(parse_shard_file_name(name), None, "{name} must not match");
        }
    }

    #[test]
    fn test_parse_shard_file_name_rejects_typos_and_malformed_separators() {
        let cases = [
            "model-00001-0f-00003.gguf",
            "model-00001_of_00003.gguf",
            "model00001-of-00003.gguf",
            "-00001-of-00003.gguf",
        ];
        for name in cases {
            assert_eq!(parse_shard_file_name(name), None, "{name} must not match");
        }
    }

    #[test]
    fn test_parse_shard_file_name_rejects_non_numeric_ranges() {
        assert_eq!(parse_shard_file_name("model-0000a-of-00003.gguf"), None);
        assert_eq!(parse_shard_file_name("model-00001-of-0000b.gguf"), None);
    }

    fn touch_shard(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"fake shard content").expect("write fixture file");
    }

    #[test]
    fn test_detect_split_shards_returns_none_for_mono_file() {
        let dir = tempfile::TempDir::new().expect("create tempdir");
        touch_shard(dir.path(), "Qwen3-8B-Q5_K_M.gguf");
        let result =
            detect_split_shards(&dir.path().join("Qwen3-8B-Q5_K_M.gguf")).expect("detection ok");
        assert_eq!(result, None);
    }

    #[test]
    fn test_detect_split_shards_returns_info_when_all_shards_present() {
        let dir = tempfile::TempDir::new().expect("create tempdir");
        for i in 1..=3 {
            touch_shard(
                dir.path(),
                &format!("Llama-70B-Q5_K_M-{i:05}-of-00003.gguf"),
            );
        }
        let result = detect_split_shards(&dir.path().join("Llama-70B-Q5_K_M-00001-of-00003.gguf"))
            .expect("detection ok");
        let info = result.expect("split must be detected");
        assert_eq!(info.prefix, "Llama-70B-Q5_K_M");
        assert_eq!(info.total, 3);
    }

    #[test]
    fn test_detect_split_shards_reports_missing_shard_with_counts() {
        let dir = tempfile::TempDir::new().expect("create tempdir");
        touch_shard(dir.path(), "Llama-70B-Q5_K_M-00001-of-00003.gguf");
        touch_shard(dir.path(), "Llama-70B-Q5_K_M-00002-of-00003.gguf");

        let err = detect_split_shards(&dir.path().join("Llama-70B-Q5_K_M-00001-of-00003.gguf"))
            .expect_err("must fail when third shard is missing");

        match err {
            LlmError::ModelShardMissing {
                prefix,
                total,
                found,
                expected,
            } => {
                assert_eq!(prefix, "Llama-70B-Q5_K_M");
                assert_eq!(total, 3);
                assert_eq!(found, 2);
                assert!(expected
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s == "Llama-70B-Q5_K_M-00003-of-00003.gguf")
                    .unwrap_or(false));
            }
            other => panic!("expected ModelShardMissing, got {other:?}"),
        }
    }

    #[test]
    fn test_detect_split_shards_rejects_non_first_shard() {
        let dir = tempfile::TempDir::new().expect("create tempdir");
        touch_shard(dir.path(), "model-00002-of-00003.gguf");

        let err = detect_split_shards(&dir.path().join("model-00002-of-00003.gguf"))
            .expect_err("must reject non-first shard");

        match err {
            LlmError::ShardIndexNotFirst { given_index, .. } => assert_eq!(given_index, 2),
            other => panic!("expected ShardIndexNotFirst, got {other:?}"),
        }
    }

    #[test]
    fn test_detect_split_shards_handles_single_shard_edge_case() {
        let dir = tempfile::TempDir::new().expect("create tempdir");
        touch_shard(dir.path(), "Foo-00001-of-00001.gguf");

        let info = detect_split_shards(&dir.path().join("Foo-00001-of-00001.gguf"))
            .expect("detection ok")
            .expect("split must be detected");
        assert_eq!(info.prefix, "Foo");
        assert_eq!(info.total, 1);
    }

    #[tokio::test]
    async fn test_load_returns_model_shard_missing_before_ffi() {
        let dir = tempfile::TempDir::new().expect("create tempdir");
        touch_shard(dir.path(), "model-00001-of-00003.gguf");
        touch_shard(dir.path(), "model-00002-of-00003.gguf");

        let config = EmbeddedBackendConfig {
            name: "local".into(),
            model_path: Some(dir.path().join("model-00001-of-00003.gguf")),
            model_paths: None,
            quantization: "q4_k_m".into(),
            device: AcceleratorDevice::Cpu,
        };
        let started = std::time::Instant::now();
        let result = EmbeddedBackend::load(&config).await;
        let elapsed = started.elapsed();

        match result {
            Err(LlmError::ModelShardMissing { total, found, .. }) => {
                assert_eq!(total, 3);
                assert_eq!(found, 2);
            }
            other => panic!("expected ModelShardMissing, got {other:?}"),
        }
        assert!(
            elapsed.as_millis() < 500,
            "fail-fast violated: validation took {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn test_load_returns_shard_index_not_first() {
        let dir = tempfile::TempDir::new().expect("create tempdir");
        touch_shard(dir.path(), "model-00002-of-00003.gguf");

        let config = EmbeddedBackendConfig {
            name: "local".into(),
            model_path: Some(dir.path().join("model-00002-of-00003.gguf")),
            model_paths: None,
            quantization: "q4_k_m".into(),
            device: AcceleratorDevice::Cpu,
        };
        let result = EmbeddedBackend::load(&config).await;
        match result {
            Err(LlmError::ShardIndexNotFirst { given_index, .. }) => {
                assert_eq!(given_index, 2);
            }
            other => panic!("expected ShardIndexNotFirst, got {other:?}"),
        }
    }

    // ─────────────────────────────────────────────
    // STORY-517 — custom split tests
    // ─────────────────────────────────────────────

    fn cfg_custom(
        model_path: Option<PathBuf>,
        model_paths: Option<Vec<PathBuf>>,
    ) -> EmbeddedBackendConfig {
        EmbeddedBackendConfig {
            name: "custom-split".into(),
            model_path,
            model_paths,
            quantization: "Q4_K_M".into(),
            device: AcceleratorDevice::Cpu,
        }
    }

    #[tokio::test]
    async fn test_load_rejects_both_model_path_and_model_paths() {
        let dir = tempfile::TempDir::new().expect("create tempdir");
        let p = dir.path().join("a.gguf");
        std::fs::write(&p, b"x").expect("write fixture");
        let config = cfg_custom(Some(p.clone()), Some(vec![p]));
        match EmbeddedBackend::load(&config).await {
            Err(LlmError::ConfigConflict { backend, reason }) => {
                assert_eq!(backend, "custom-split");
                assert!(reason.contains("mutuellement exclusifs"));
            }
            other => panic!("expected ConfigConflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_load_rejects_neither_model_path_nor_model_paths() {
        let config = cfg_custom(None, None);
        match EmbeddedBackend::load(&config).await {
            Err(LlmError::ConfigConflict { backend, reason }) => {
                assert_eq!(backend, "custom-split");
                assert!(reason.contains("requis"));
            }
            other => panic!("expected ConfigConflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_load_rejects_empty_model_paths() {
        let config = cfg_custom(None, Some(vec![]));
        match EmbeddedBackend::load(&config).await {
            Err(LlmError::ConfigConflict { backend, reason }) => {
                assert_eq!(backend, "custom-split");
                assert!(reason.contains("vide"));
            }
            other => panic!("expected ConfigConflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_load_reports_missing_path_in_model_paths() {
        let dir = tempfile::TempDir::new().expect("create tempdir");
        let existing = dir.path().join("ok.gguf");
        std::fs::write(&existing, b"x").expect("write fixture");
        let missing = dir.path().join("missing.gguf");
        let config = cfg_custom(None, Some(vec![existing, missing.clone()]));
        match EmbeddedBackend::load(&config).await {
            Err(LlmError::ModelNotFound { path }) => {
                assert!(
                    path.ends_with("missing.gguf"),
                    "ModelNotFound path must end with missing.gguf, got {}",
                    path.display()
                );
            }
            other => panic!("expected ModelNotFound, got {other:?}"),
        }
    }

    #[test]
    fn test_serde_roundtrip_with_model_paths() {
        let cfg_toml = r#"
            name = "custom"
            quantization = "Q4_K_M"
            device = "cpu"
            model_paths = ["/tmp/a.gguf", "/tmp/b.gguf"]
        "#;
        let cfg: EmbeddedBackendConfig = toml::from_str(cfg_toml).expect("parse toml");
        assert!(cfg.model_path.is_none());
        let paths = cfg.model_paths.as_ref().expect("model_paths present");
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], PathBuf::from("/tmp/a.gguf"));
        assert_eq!(paths[1], PathBuf::from("/tmp/b.gguf"));

        let json = serde_json::to_value(&cfg).expect("serialize");
        assert!(
            json.get("model_path").is_none(),
            "model_path must be omitted when None"
        );
        let paths_json = json
            .get("model_paths")
            .and_then(|v| v.as_array())
            .expect("model_paths array");
        assert_eq!(paths_json.len(), 2);
    }
}
