//! `LlmRouter` — dispatche les requêtes vers le bon backend par nom.
//!
//! Construit au démarrage du Supervisor (position 5, avant `TaskRouter`)
//! via [`LlmRouter::from_config`]. Partageable via `Arc<LlmRouter>` grâce
//! à `Clone + Send + Sync`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio_util::sync::CancellationToken;

use crate::pricing::PricingTier;

use apollia_core::events::{EventBusSender, RuntimeEvent};
use apollia_core::token_budget::TokenBudget;

use crate::token_budget::SessionBudgetTracker;
use apollia_core::{
    LlmBackendConfig, LlmBackendRepository, LlmProvider, LlmRoutingConfig, VertexConfig,
};

use crate::types::{BackendInfo, CompletionModel, CompletionRequest, CompletionResponse, LlmError};

#[cfg(feature = "local")]
use crate::backends::embedded::{EmbeddedBackend, EmbeddedBackendConfig};

#[cfg(feature = "cloud")]
use crate::backends::anthropic::AnthropicClient;

#[cfg(feature = "cloud")]
use crate::backends::openai::{ApiBackendConfig, OpenAICompatibleClient};

#[cfg(feature = "cloud")]
use crate::backends::vertex::VertexClient;

// ─────────────────────────────────────────────
// Configuration
// ─────────────────────────────────────────────

/// Configuration LLM désérialisée depuis la section `[llm]` de `apollia.toml`.
///
/// Passée à [`LlmRouter::from_config`] au démarrage du Supervisor.
/// Le champ `default` désigne le backend utilisé quand `get(None)` est appelé.
///
/// La section `[llm.routing]` est **obligatoire** — son absence provoque
/// [`LlmError::RoutingConfigMissing`] au démarrage (Principe #4 — Fail fast).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LlmConfig {
    /// Nom du backend par défaut (doit exister dans `backends`).
    pub default: String,
    /// Liste des backends à instancier dans `[[llm.backends]]`.
    pub backends: Vec<BackendConfig>,
    /// Paramètres d'observabilité (tokens, latence, coût, prompt debug).
    #[serde(default)]
    pub observability: ObservabilityConfig,
    /// Routing LLM par niveau de précision (section `[llm.routing]`).
    ///
    /// Obligatoire — déclenche [`LlmError::RoutingConfigMissing`] si absent.
    /// Voir [`LlmRoutingConfig`] pour les champs `precise` et `fast`.
    pub routing: Option<LlmRoutingConfig>,
    /// Surcharges de pricing opérateur (section `[llm.pricing_overrides]`).
    ///
    /// Les entrées ici ont priorité sur la table interne de [`crate::pricing::default_pricing`].
    /// Permet d'ajouter des modèles custom ou de corriger les prix sans mise à jour du code.
    ///
    /// Exemple `apollia.toml` :
    /// ```toml
    /// [llm.pricing_overrides]
    /// "custom-local-model" = { input_per_mtok = 0.0, output_per_mtok = 0.0 }
    /// "claude-sonnet-4-5"  = { input_per_mtok = 2.5, output_per_mtok = 12.0 }
    /// ```
    #[serde(default)]
    pub pricing_overrides: HashMap<String, PricingTier>,
    /// Seuil de coût en USD au-delà duquel [`RuntimeEvent::TokenBudgetUpdated`]
    /// est émis avec `threshold_exceeded = true`.
    ///
    /// `None` (défaut) désactive les alertes de seuil.
    ///
    /// Exemple `apollia.toml` :
    /// ```toml
    /// [llm]
    /// cost_alert_threshold_usd = 0.50
    /// ```
    #[serde(default)]
    pub cost_alert_threshold_usd: Option<f64>,
    /// Configuration optionnelle du backend Google Vertex AI (`[llm.vertex]`).
    ///
    /// Si absent ou `enabled = false`, le backend n'est pas instancié.
    ///
    /// Exemple `apollia.toml` :
    /// ```toml
    /// [llm.vertex]
    /// enabled    = true
    /// project_id = "my-gcp-project"
    /// location   = "us-east5"
    /// model_id   = "claude-sonnet-4-6@20251001"
    /// ```
    #[serde(default)]
    pub vertex: Option<VertexConfig>,
}

impl LlmConfig {
    /// Converts TOML-parsed backends to [`LlmBackendConfig`] entries for `system.db`.
    ///
    /// Used by the Supervisor at startup to migrate from `apollia.toml` to `system.db`
    /// when no backends are found in the database (first boot, or manual TOML edits).
    pub fn to_db_configs(&self) -> Vec<LlmBackendConfig> {
        self.backends
            .iter()
            .map(|b| backend_config_to_db(b, b.name() == self.default))
            .collect()
    }
}

/// Converts a TOML [`BackendConfig`] to a [`LlmBackendConfig`] for `system.db`.
fn backend_config_to_db(cfg: &BackendConfig, is_default: bool) -> LlmBackendConfig {
    match &cfg.kind {
        #[cfg(feature = "local")]
        BackendKind::Embedded(embedded) => LlmBackendConfig {
            name: embedded.name.clone(),
            provider: LlmProvider::LlamaCpp,
            model: embedded_model_descriptor(embedded),
            config_json: serde_json::to_value(embedded).unwrap_or_default(),
            enabled: true,
            is_default,
        },
        #[cfg(feature = "cloud")]
        BackendKind::Api(api) => LlmBackendConfig {
            name: api.name.clone(),
            provider: infer_api_provider_from_url(&api.api_url),
            model: api.model.clone(),
            config_json: serde_json::json!({
                "api_url": api.api_url,
                "api_key": format!("${{{}}}", api.api_key_env),
            }),
            enabled: true,
            is_default,
        },
    }
}

/// Infers a [`LlmProvider`] from the API base URL.
#[cfg(feature = "cloud")]
fn infer_api_provider_from_url(api_url: &str) -> LlmProvider {
    if api_url.contains("anthropic.com") {
        LlmProvider::Anthropic
    } else if api_url.contains("mistral.ai") {
        LlmProvider::Mistral
    } else if api_url.contains("localhost:11434") || api_url.contains("ollama") {
        LlmProvider::Ollama
    } else {
        LlmProvider::OpenAi
    }
}

/// Paramètres d'observabilité pour le router LLM.
///
/// Les champs `log_token_usage` et `log_latency` sont actifs par défaut.
/// `log_cost` et `debug_log_prompt` sont désactivés par défaut.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ObservabilityConfig {
    /// Log le nombre de tokens consommés après chaque appel.
    #[serde(default = "default_true")]
    pub log_token_usage: bool,
    /// Log la latence totale de chaque appel.
    #[serde(default = "default_true")]
    pub log_latency: bool,
    /// Log le coût estimé en USD (backends cloud uniquement).
    #[serde(default)]
    pub log_cost: bool,
    /// Log le prompt complet au niveau `TRACE` (uniquement en debug).
    #[serde(default)]
    pub debug_log_prompt: bool,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            log_token_usage: true,
            log_latency: true,
            log_cost: false,
            debug_log_prompt: false,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Entrée de configuration pour un backend individuel dans `[[llm.backends]]`.
///
/// Le nom logique du backend est défini dans la config interne
/// (`EmbeddedBackendConfig.name` ou `ApiBackendConfig.name`).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BackendConfig {
    /// Type et paramètres du backend — discriminé par le champ TOML `type`.
    #[serde(flatten)]
    pub kind: BackendKind,
}

impl BackendConfig {
    /// Retourne le nom logique du backend depuis la config interne.
    pub fn name(&self) -> &str {
        match &self.kind {
            #[cfg(feature = "local")]
            BackendKind::Embedded(cfg) => &cfg.name,
            #[cfg(feature = "cloud")]
            BackendKind::Api(cfg) => &cfg.name,
        }
    }

    /// Retourne un hint de chemin/URL pour l'événement `LlmModelLoading`.
    ///
    /// - Backend local : chemin vers le fichier `.gguf`
    /// - Backend cloud : URL de l'API
    fn model_path_hint(&self) -> String {
        match &self.kind {
            #[cfg(feature = "local")]
            BackendKind::Embedded(cfg) => embedded_model_descriptor(cfg),
            #[cfg(feature = "cloud")]
            BackendKind::Api(cfg) => cfg.api_url.clone(),
        }
    }
}

/// Résumé humain de l'emplacement du modèle pour un backend embarqué.
///
/// Utilisé à la fois pour :
/// - `LlmBackendConfig::model` (colonne SQLite, clé d'identification)
/// - `RuntimeEvent::LlmModelLoading.model_path` (événement d'observabilité)
///
/// Retourne le premier chemin renseigné (mono-fichier, premier shard standard,
/// ou premier shard d'une liste explicite). Chaîne vide si la config est
/// incohérente — le défaut sera bloqué par [`EmbeddedBackend::load`] au
/// démarrage, qui retourne [`LlmError::ConfigConflict`] (Principe #4).
#[cfg(feature = "local")]
fn embedded_model_descriptor(cfg: &EmbeddedBackendConfig) -> String {
    if let Some(p) = cfg.model_path.as_ref() {
        return p.to_string_lossy().into_owned();
    }
    if let Some(paths) = cfg.model_paths.as_ref() {
        if let Some(first) = paths.first() {
            return first.to_string_lossy().into_owned();
        }
    }
    String::new()
}

/// Discriminant de type de backend dans `[[llm.backends]]`.
///
/// - `type = "embedded"` → [`EmbeddedBackendConfig`] (feature `"local"`)
/// - `type = "api"` → [`ApiBackendConfig`] (feature `"cloud"`)
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BackendKind {
    /// Backend d'inférence embarqué in-process via `mistralrs` (feature `"local"`).
    #[cfg(feature = "local")]
    Embedded(EmbeddedBackendConfig),
    /// Backend HTTP cloud compatible OpenAI ou Anthropic (feature `"cloud"`).
    #[cfg(feature = "cloud")]
    Api(ApiBackendConfig),
}

// ─────────────────────────────────────────────
// Router
// ─────────────────────────────────────────────

/// Point d'entrée unique pour toute la couche LLM d'Apollia OS.
///
/// Instancié par le Supervisor au démarrage (position 5) via
/// [`LlmRouter::from_config`]. Dispatche les requêtes vers le bon backend
/// par nom via [`get`](Self::get), avec fallback sur le backend `default`.
///
/// Les méthodes [`route_precise`](Self::route_precise) et [`route_fast`](Self::route_fast)
/// sélectionnent le backend selon le niveau de précision requis (config `[llm.routing]`).
///
/// `LlmRouter` est `Clone + Send + Sync` — partageable via `Arc<LlmRouter>`
/// entre les composants du runtime (agit comme un catalogue en lecture seule).
///
/// `Debug` est implémenté manuellement : `Arc<dyn CompletionModel>` n'implémente
/// pas `Debug` (le trait objet ne l'exporte pas).
///
/// Le `CancellationToken` de session permet à `ORIAEngine::abort()` d'annuler
/// tous les appels LLM en cours et leurs délais de retry via [`cancellation_token`](Self::cancellation_token).
#[derive(Clone)]
pub struct LlmRouter {
    backends: HashMap<String, Arc<dyn CompletionModel>>,
    default: String,
    /// Routing LLM par niveau de précision — `None` pour les routers construits
    /// via `from_repository` ou `with_backends` (pas de config TOML).
    routing: Option<LlmRoutingConfig>,
    /// Token d'annulation partagé par tous les backends de ce router.
    cancellation_token: CancellationToken,
    /// Budget de session cumulé avec émission d'événements temps réel.
    ///
    /// Protégé par un `Mutex` standard (verrou court, jamais tenu pendant
    /// un appel async) pour permettre le `Clone` sans `Arc` supplémentaire.
    session_budget: Arc<Mutex<SessionBudgetTracker>>,
}

impl LlmRouter {
    /// Construit le router depuis la configuration — appelé par le Supervisor au démarrage.
    ///
    /// Itère sur `config.backends` et tente d'instancier chaque backend :
    /// - `Embedded` → [`EmbeddedBackend::load`] ; erreur fatale propagée si échoue.
    /// - `Api` → résout la clé API ; si absente : `tracing::warn!` + backend ignoré.
    ///
    /// Après la boucle, vérifie que `config.default` est présent dans le map.
    /// Si absent (non configuré ou ignoré) → retourne [`LlmError::BackendUnavailable`].
    ///
    /// # Erreurs
    ///
    /// - [`LlmError::ModelNotFound`] / [`LlmError::InferenceError`] — chargement `.gguf` échoué.
    /// - [`LlmError::BackendUnavailable`] — le backend par défaut est introuvable ou indisponible.
    pub async fn from_config(config: &LlmConfig) -> Result<Self, LlmError> {
        let cancellation_token = CancellationToken::new();
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();

        for backend_cfg in &config.backends {
            let name = backend_cfg.name().to_owned();

            let backend: Arc<dyn CompletionModel> = match &backend_cfg.kind {
                #[cfg(feature = "local")]
                BackendKind::Embedded(cfg) => Arc::new(EmbeddedBackend::load(cfg).await?),

                #[cfg(feature = "cloud")]
                BackendKind::Api(cfg) => match cfg.resolve_api_key() {
                    Ok(key) => {
                        // Heuristique : API Anthropic → AnthropicClient,
                        // tout autre fournisseur → OpenAICompatibleClient.
                        if cfg.api_url.contains("anthropic.com") {
                            Arc::new(AnthropicClient::new(
                                cfg,
                                key,
                                config.pricing_overrides.clone(),
                                cancellation_token.clone(),
                            ))
                        } else {
                            Arc::new(OpenAICompatibleClient::new(
                                cfg,
                                key,
                                cancellation_token.clone(),
                            ))
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            backend = %name,
                            error = %e,
                            "backend ignoré : clé API absente"
                        );
                        continue;
                    }
                },
            };

            backends.insert(name, backend);
        }

        // Vertex AI — instancié séparément depuis [llm.vertex] si enabled = true.
        #[cfg(feature = "cloud")]
        if let Some(vertex_cfg) = &config.vertex {
            if vertex_cfg.enabled {
                match VertexClient::new(vertex_cfg, cancellation_token.clone()) {
                    Ok(client) => {
                        backends.insert(
                            "vertex".to_owned(),
                            Arc::new(client) as Arc<dyn CompletionModel>,
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "Vertex AI backend ignoré : ADC absent ou invalide"
                        );
                    }
                }
            }
        }

        // : le backend par défaut doit être disponible après la boucle.
        if !backends.contains_key(&config.default) {
            return Err(LlmError::BackendUnavailable {
                backend: config.default.clone(),
                reason: "not configured".to_owned(),
            });
        }

        // : [llm.routing] obligatoire — erreur fatale si absent (Principe #4).
        let routing = config
            .routing
            .as_ref()
            .ok_or(LlmError::RoutingConfigMissing)?;

        // : les backends nommés dans routing doivent exister.
        if !backends.contains_key(&routing.precise) {
            return Err(LlmError::BackendNotFound(routing.precise.clone()));
        }
        if !backends.contains_key(&routing.fast) {
            return Err(LlmError::BackendNotFound(routing.fast.clone()));
        }

        Ok(Self {
            backends,
            default: config.default.clone(),
            routing: config.routing.clone(),
            cancellation_token,
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        })
    }

    /// Construit le router depuis la configuration avec observabilité EventBus.
    ///
    /// Variante de [`from_config`](Self::from_config) à utiliser par le Supervisor.
    /// Émet sur le bus pour chaque backend :
    /// - [`RuntimeEvent::LlmModelLoading`] — avant le chargement
    /// - [`RuntimeEvent::LlmModelReady`] — si le chargement réussit
    /// - [`RuntimeEvent::LlmModelFailed`] — si le chargement échoue (backend ignoré, pas de crash)
    ///
    /// L'`EventBusSender` est optionnel — `None` désactive l'émission d'événements
    /// sans changer le comportement fonctionnel.
    ///
    /// Contrairement à [`from_config`](Self::from_config), les erreurs de chargement
    /// par backend (`.gguf` absent, etc.) sont loggées + émises comme `LlmModelFailed`
    /// mais ne propagent pas d'erreur — le router continue avec les backends disponibles.
    ///
    /// # Erreurs
    ///
    /// - [`LlmError::BackendUnavailable`] — le backend par défaut est introuvable
    ///   après que tous les backends aient été tentés.
    pub async fn from_config_with_bus(
        config: &LlmConfig,
        bus: Option<EventBusSender>,
    ) -> Result<Self, LlmError> {
        let cancellation_token = CancellationToken::new();
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();

        for backend_cfg in &config.backends {
            let name = backend_cfg.name().to_owned();
            let model_path = backend_cfg.model_path_hint();

            // : émettre LlmModelLoading avant chaque tentative de chargement.
            if let Some(ref b) = bus {
                let _ = b.send(RuntimeEvent::LlmModelLoading {
                    backend: name.clone(),
                    model_path,
                });
            }

            let result: Result<Arc<dyn CompletionModel>, LlmError> = match &backend_cfg.kind {
                #[cfg(feature = "local")]
                BackendKind::Embedded(cfg) => EmbeddedBackend::load(cfg)
                    .await
                    .map(|b| Arc::new(b) as Arc<dyn CompletionModel>),

                #[cfg(feature = "cloud")]
                BackendKind::Api(cfg) => match cfg.resolve_api_key() {
                    Ok(key) => {
                        let b: Arc<dyn CompletionModel> = if cfg.api_url.contains("anthropic.com") {
                            Arc::new(AnthropicClient::new(
                                cfg,
                                key,
                                config.pricing_overrides.clone(),
                                cancellation_token.clone(),
                            ))
                        } else {
                            Arc::new(OpenAICompatibleClient::new(
                                cfg,
                                key,
                                cancellation_token.clone(),
                            ))
                        };
                        Ok(b)
                    }
                    Err(e) => Err(LlmError::BackendUnavailable {
                        backend: name.clone(),
                        reason: e.to_string(),
                    }),
                },
            };

            match result {
                Ok(backend) => {
                    // : émettre LlmModelReady après succès.
                    if let Some(ref b) = bus {
                        let _ = b.send(RuntimeEvent::LlmModelReady {
                            backend: name.clone(),
                            model_id: backend.model_id().to_owned(),
                        });
                    }
                    backends.insert(name, backend);
                }
                Err(e) => {
                    tracing::warn!(
                        backend = %name,
                        error = %e,
                        "backend ignoré : chargement échoué"
                    );
                    // : émettre LlmModelFailed — backend ignoré, pas de crash.
                    if let Some(ref b) = bus {
                        let _ = b.send(RuntimeEvent::LlmModelFailed {
                            backend: name.clone(),
                            reason: e.to_string(),
                        });
                    }
                    // Continue — on tente les backends suivants.
                }
            }
        }

        // Vertex AI — instancié séparément depuis [llm.vertex] si enabled = true.
        #[cfg(feature = "cloud")]
        if let Some(vertex_cfg) = &config.vertex {
            if vertex_cfg.enabled {
                let vertex_name = "vertex".to_owned();
                if let Some(ref b) = bus {
                    let _ = b.send(RuntimeEvent::LlmModelLoading {
                        backend: vertex_name.clone(),
                        model_path: vertex_cfg.model_id.clone(),
                    });
                }
                match VertexClient::new(vertex_cfg, cancellation_token.clone()) {
                    Ok(client) => {
                        let model_id = client.model_id().to_owned();
                        if let Some(ref b) = bus {
                            let _ = b.send(RuntimeEvent::LlmModelReady {
                                backend: vertex_name.clone(),
                                model_id,
                            });
                        }
                        backends.insert(vertex_name, Arc::new(client) as Arc<dyn CompletionModel>);
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "Vertex AI backend ignoré : ADC absent ou invalide"
                        );
                        if let Some(ref b) = bus {
                            let _ = b.send(RuntimeEvent::LlmModelFailed {
                                backend: vertex_name,
                                reason: e.to_string(),
                            });
                        }
                    }
                }
            }
        }

        if !backends.contains_key(&config.default) {
            return Err(LlmError::BackendUnavailable {
                backend: config.default.clone(),
                reason: "not configured".to_owned(),
            });
        }

        // : [llm.routing] obligatoire — erreur fatale si absent (Principe #4).
        let routing = config
            .routing
            .as_ref()
            .ok_or(LlmError::RoutingConfigMissing)?;

        if !backends.contains_key(&routing.precise) {
            return Err(LlmError::BackendNotFound(routing.precise.clone()));
        }
        if !backends.contains_key(&routing.fast) {
            return Err(LlmError::BackendNotFound(routing.fast.clone()));
        }

        Ok(Self {
            backends,
            default: config.default.clone(),
            routing: config.routing.clone(),
            cancellation_token,
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::new(
                bus,
                config.cost_alert_threshold_usd,
            ))),
        })
    }

    /// Appelle le backend et émet automatiquement [`RuntimeEvent::LlmCallCompleted`].
    ///
    /// Séquence d'exécution :
    /// 1. Log le prompt au niveau `TRACE` si `obs.debug_log_prompt` est actif.
    /// 2. Appelle `backend.complete(req)`.
    /// 3. Émet `LlmCallCompleted` fire-and-forget sur le bus (si présent).
    /// 4. Log tokens/latence au niveau `INFO` selon les flags `obs`.
    /// 5. Retourne `Ok(response)`.
    ///
    /// L'`EventBusSender` est optionnel — `None` désactive l'émission sans changer
    /// le comportement fonctionnel. Les erreurs `send()` sont silencieusement ignorées.
    ///
    /// # Erreurs
    ///
    /// - [`LlmError::BackendUnavailable`] — le backend demandé n'est pas dans le router.
    /// - Toute erreur propagée par `backend.complete()`.
    pub async fn complete_with_observability(
        &self,
        backend_name: Option<&str>,
        req: CompletionRequest,
        bus: Option<&EventBusSender>,
        obs: &ObservabilityConfig,
    ) -> Result<CompletionResponse, LlmError> {
        let backend_key = backend_name.unwrap_or(&self.default);

        let backend =
            self.backends
                .get(backend_key)
                .ok_or_else(|| LlmError::BackendUnavailable {
                    backend: backend_key.to_owned(),
                    reason: "not found in router".to_owned(),
                })?;

        // : log du prompt uniquement à TRACE — jamais à INFO.
        if obs.debug_log_prompt {
            tracing::trace!(prompt = ?req.messages, "llm prompt");
        }

        let started = Instant::now();
        let response = backend.complete(req).await?;
        let latency_ms = started.elapsed().as_millis() as u64;

        // Accumulate into the session budget and emit TokenBudgetUpdated.
        // Lock held only for the duration of record_usage() — never across awaits.
        if let Ok(mut tracker) = self.session_budget.lock() {
            tracker.record_usage(&response.usage, latency_ms, response.ttft_ms);
        }

        // : émission fire-and-forget — erreurs send() silencieusement ignorées.
        if let Some(b) = bus {
            let _ = b.send(RuntimeEvent::LlmCallCompleted {
                backend: backend_key.to_owned(),
                model: backend.model_id().to_owned(),
                task_id: None,
                step_id: None,
                prompt_tokens: response.usage.prompt_tokens,
                completion_tokens: response.usage.completion_tokens,
                latency_ms,
                cost_usd: response.usage.cost_usd,
            });
        }

        if obs.log_token_usage {
            tracing::info!(
                backend = backend_key,
                prompt_tokens = response.usage.prompt_tokens,
                completion_tokens = response.usage.completion_tokens,
                "llm token usage"
            );
        }

        if obs.log_latency {
            tracing::info!(
                backend = backend_key,
                latency_ms = latency_ms,
                "llm latency"
            );
        }

        Ok(response)
    }

    /// Invoque le backend primaire puis, en cas d'échec non récupérable,
    /// bascule sur le premier backend secondaire disponible (US-SP42-040).
    ///
    /// Émet [`RuntimeEvent::LlmFallbackTriggered`] sur le bus à chaque bascule
    /// réussie. Le basculement est silencieux du point de vue fonctionnel —
    /// l'appelant reçoit soit la réponse du primaire, soit la réponse du
    /// premier fallback qui répond, soit la dernière erreur observée.
    pub async fn complete_with_fallback(
        &self,
        primary: &str,
        fallbacks: &[&str],
        req: CompletionRequest,
        bus: Option<&EventBusSender>,
        obs: &ObservabilityConfig,
    ) -> Result<CompletionResponse, LlmError> {
        let primary_result = self
            .complete_with_observability(Some(primary), req.clone(), bus, obs)
            .await;

        let primary_err = match primary_result {
            Ok(response) => return Ok(response),
            Err(e) => e,
        };

        let mut last_err = primary_err;
        for &candidate in fallbacks {
            if candidate == primary || !self.backends.contains_key(candidate) {
                continue;
            }
            if let Some(b) = bus {
                let _ = b.send(RuntimeEvent::LlmFallbackTriggered {
                    from_provider: primary.to_string(),
                    to_provider: candidate.to_string(),
                    reason: last_err.to_string(),
                });
            }
            tracing::warn!(
                from = %primary,
                to = %candidate,
                reason = %last_err,
                "LLM primary failed, attempting fallback"
            );
            match self
                .complete_with_observability(Some(candidate), req.clone(), bus, obs)
                .await
            {
                Ok(response) => return Ok(response),
                Err(e) => {
                    last_err = e;
                }
            }
        }

        Err(last_err)
    }

    /// Ouvre un stream de [`StreamChunk`]s depuis le backend résolu.
    ///
    /// Résout le backend (par nom ou défaut), appelle `backend.stream(req)`,
    /// et retourne le stream brut. L'appelant est responsable de l'émission
    /// de l'événement `LlmCallCompleted` une fois le stream consommé.
    ///
    /// # Erreurs
    ///
    /// - [`LlmError::BackendUnavailable`] — le backend demandé n'est pas dans le router.
    /// - Toute erreur propagée par `backend.stream()`.
    pub async fn stream_with_observability(
        &self,
        backend_name: Option<&str>,
        req: CompletionRequest,
        obs: &ObservabilityConfig,
    ) -> Result<
        std::pin::Pin<
            Box<dyn futures::Stream<Item = Result<crate::types::StreamChunk, LlmError>> + Send>,
        >,
        LlmError,
    > {
        let backend_key = backend_name.unwrap_or(&self.default);

        let backend =
            self.backends
                .get(backend_key)
                .ok_or_else(|| LlmError::BackendUnavailable {
                    backend: backend_key.to_owned(),
                    reason: "not found in router".to_owned(),
                })?;

        if obs.debug_log_prompt {
            tracing::trace!(prompt = ?req.messages, "llm stream prompt");
        }

        let stream = backend.stream(req).await?;
        Ok(stream)
    }

    /// Retourne le backend par nom, ou le backend défaut si `name` est `None`.
    ///
    /// Retourne `None` si le backend demandé n'est pas dans le router.
    pub fn get(&self, name: Option<&str>) -> Option<Arc<dyn CompletionModel>> {
        let key = name.unwrap_or(&self.default);
        self.backends.get(key).cloned()
    }

    /// Construit un `LlmRouter` avec des backends déjà instanciés.
    ///
    /// Utilisé dans les tests d'intégration pour injecter des mocks [`CompletionModel`]
    /// sans passer par la configuration TOML.
    /// Le routing LLM n'est pas configuré sur ce router — `route_precise()` et
    /// `route_fast()` retourneront [`LlmError::RoutingConfigMissing`].
    ///
    /// # Panics
    ///
    /// Panique si `default` n'est pas présent dans `backends`.
    pub fn with_backends(
        backends: HashMap<String, Arc<dyn CompletionModel>>,
        default: impl Into<String>,
    ) -> Self {
        let default = default.into();
        assert!(
            backends.contains_key(&default),
            "LlmRouter::with_backends — backend '{default}' must be present in backends map"
        );
        Self {
            backends,
            default,
            routing: None,
            cancellation_token: CancellationToken::new(),
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        }
    }

    /// Construit le router depuis un [`LlmBackendRepository`] SQLite.
    ///
    /// Charge tous les backends `enabled = true`. Le backend `is_default = true`
    /// devient le backend par défaut. Les backends qui échouent à l'instanciation
    /// sont loggués avec `tracing::warn!` et ignorés (dégradation non fatale).
    ///
    /// # Errors
    ///
    /// - [`LlmError::BackendUnavailable`] si aucun backend n'est marqué `is_default = true`
    ///   dans `system.db`, ou si le backend par défaut échoue à l'instanciation.
    pub async fn from_repository(repo: &LlmBackendRepository) -> Result<Self, LlmError> {
        let all = repo.list().map_err(|e| LlmError::BackendUnavailable {
            backend: "system.db".to_string(),
            reason: e.to_string(),
        })?;

        let default_name = repo
            .find_default()
            .map_err(|e| LlmError::BackendUnavailable {
                backend: "system.db".to_string(),
                reason: e.to_string(),
            })?
            .ok_or_else(|| LlmError::BackendUnavailable {
                backend: "default".to_string(),
                reason: "no default LLM backend in system.db — configure one with is_default=true"
                    .to_string(),
            })?
            .name;

        let cancellation_token = CancellationToken::new();
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();

        for cfg in all.into_iter().filter(|c| c.enabled) {
            let name = cfg.name.clone();
            match instantiate_from_config(&cfg, cancellation_token.clone()).await {
                Ok(backend) => {
                    backends.insert(name, backend);
                }
                Err(e) => {
                    tracing::warn!(
                        backend = %name,
                        error = %e,
                        "LLM backend skipped during repository load"
                    );
                }
            }
        }

        if !backends.contains_key(&default_name) {
            return Err(LlmError::BackendUnavailable {
                backend: default_name,
                reason: "default backend failed to instantiate".to_string(),
            });
        }

        Ok(Self {
            backends,
            default: default_name,
            routing: None,
            cancellation_token,
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        })
    }

    /// Retourne le backend pour `llm_backend`, ou le backend par défaut si `None` / inconnu.
    ///
    /// Émet `tracing::warn!` si le backend nommé est absent du router (fallback silencieux
    /// sauf pour le log structuré).
    ///
    /// # Panics
    ///
    /// Panique si le router ne contient aucun backend. Ne pas appeler `route()` sur un
    /// router construit via [`LlmRouter::empty()`].
    pub fn route(&self, llm_backend: Option<&str>) -> Arc<dyn CompletionModel> {
        match llm_backend {
            None => self
                .backends
                .get(&self.default)
                .expect("LlmRouter invariant: default backend must be present")
                .clone(),
            Some(name) => {
                if let Some(b) = self.backends.get(name) {
                    b.clone()
                } else {
                    tracing::warn!(
                        backend = %name,
                        fallback = %self.default,
                        "unknown LLM backend requested, falling back to default"
                    );
                    self.backends
                        .get(&self.default)
                        .expect("LlmRouter invariant: default backend must be present")
                        .clone()
                }
            }
        }
    }

    /// Retourne les noms de tous les backends chargés dans le router.
    pub fn backend_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.backends.keys().cloned().collect();
        names.sort();
        names
    }

    /// Crée un `LlmRouter` vide sans aucun backend — pour les tests unitaires.
    ///
    /// Utilisé pour tester les chemins de dégradation :
    /// `ctx.llm = None` et `AgentDegraded` sur l'EventBus.
    pub fn empty() -> Self {
        Self {
            backends: HashMap::new(),
            default: String::new(),
            routing: None,
            cancellation_token: CancellationToken::new(),
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        }
    }

    /// Retourne le nom du backend par défaut configuré dans `apollia.toml`.
    pub fn default_name(&self) -> &str {
        &self.default
    }

    /// Retourne le seuil d'alerte de coût LLM configuré en USD, ou `None` si non configuré.
    ///
    /// Correspond à `[llm] cost_alert_threshold_usd` dans `apollia.toml`.
    pub fn cost_alert_threshold_usd(&self) -> Option<f64> {
        self.session_budget.lock().ok()?.threshold_usd()
    }

    /// Retourne le backend configuré pour les tâches de raisonnement profond.
    ///
    /// Sélectionne le backend nommé dans `[llm.routing] precise` de `apollia.toml`.
    /// Utilisé par les composants nécessitant une qualité de raisonnement maximale
    /// (planification ORIA, analyse complexe, jugement).
    ///
    /// # Erreurs
    ///
    /// - [`LlmError::RoutingConfigMissing`] — router construit sans config `[llm.routing]`.
    /// - [`LlmError::BackendNotFound`] — backend nommé absent du router.
    pub fn route_precise(&self) -> Result<Arc<dyn CompletionModel>, LlmError> {
        let routing = self
            .routing
            .as_ref()
            .ok_or(LlmError::RoutingConfigMissing)?;
        self.backends
            .get(&routing.precise)
            .cloned()
            .ok_or_else(|| LlmError::BackendNotFound(routing.precise.clone()))
    }

    /// Retourne le backend configuré pour les tâches d'extraction légère.
    ///
    /// Sélectionne le backend nommé dans `[llm.routing] fast` de `apollia.toml`.
    /// Utilisé par les composants effectuant des extractions déterministes
    /// (métadonnées, résumés courts, classification, parsing de paths).
    ///
    /// # Erreurs
    ///
    /// - [`LlmError::RoutingConfigMissing`] — router construit sans config `[llm.routing]`.
    /// - [`LlmError::BackendNotFound`] — backend nommé absent du router.
    pub fn route_fast(&self) -> Result<Arc<dyn CompletionModel>, LlmError> {
        let routing = self
            .routing
            .as_ref()
            .ok_or(LlmError::RoutingConfigMissing)?;
        self.backends
            .get(&routing.fast)
            .cloned()
            .ok_or_else(|| LlmError::BackendNotFound(routing.fast.clone()))
    }

    /// Retourne la taille estimée de la fenêtre de contexte en tokens.
    ///
    /// Utilisé par `ContextManager` pour calculer le taux d'utilisation et décider
    /// si un compactage est nécessaire. La valeur `200_000` correspond à la fenêtre
    /// de `claude-sonnet` — conservatrice et valide pour tous les backends cloud.
    /// Les backends locaux disposent généralement de fenêtres plus petites ; le
    /// compactage précoce est préférable à un `context_length_exceeded`.
    pub fn context_limit(&self) -> usize {
        200_000
    }

    /// Retourne le `CancellationToken` de session pour annuler les appels en cours.
    ///
    /// Appelé par `ORIAEngine::abort()` pour interrompre tous les appels LLM
    /// et délais de retry en cours sur l'ensemble des backends du router.
    ///
    /// Le token est `Clone` — chaque backend en possède un clone,
    /// tous annulés simultanément par `token.cancel()`.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }

    /// Retourne un snapshot du budget de tokens cumulé depuis le dernier reset.
    ///
    /// Appelé par `ORIAEngine` en fin de tâche pour persister le coût dans
    /// `~/.apollia/session_costs.jsonl`.
    pub fn session_budget(&self) -> TokenBudget {
        self.session_budget
            .lock()
            .map(|t| t.to_token_budget())
            .unwrap_or_default()
    }

    /// Remet à zéro les compteurs de session.
    ///
    /// Appelé par `ORIAEngine` au début de chaque tâche pour isoler
    /// les compteurs par exécution. La configuration du tracker (bus, seuil) est préservée.
    pub fn reset_session_budget(&self) {
        if let Ok(mut tracker) = self.session_budget.lock() {
            tracker.reset();
        }
    }

    /// Liste tous les backends disponibles avec leurs informations synthétiques.
    pub fn list(&self) -> Vec<BackendInfo> {
        self.backends
            .values()
            .map(|b| BackendInfo {
                name: b.backend_name().to_string(),
                model_id: b.model_id().to_string(),
                available: b.is_available(),
            })
            .collect()
    }
}

// ─────────────────────────────────────────────
// Backend instantiation helpers
// ─────────────────────────────────────────────

/// Instancie un [`CompletionModel`] depuis une [`LlmBackendConfig`] SQLite.
async fn instantiate_from_config(
    cfg: &LlmBackendConfig,
    cancel: CancellationToken,
) -> Result<Arc<dyn CompletionModel>, LlmError> {
    match &cfg.provider {
        LlmProvider::LlamaCpp => instantiate_embedded_backend(cfg).await,
        provider => instantiate_cloud_backend(cfg, provider, cancel).await,
    }
}

/// Instancie le backend llama-cpp embarqué depuis la config SQLite.
#[cfg(feature = "local")]
async fn instantiate_embedded_backend(
    cfg: &LlmBackendConfig,
) -> Result<Arc<dyn CompletionModel>, LlmError> {
    use crate::backends::embedded::AcceleratorDevice;
    use std::path::PathBuf;

    let model_path = cfg
        .config_json
        .get("model_path")
        .and_then(|v| v.as_str())
        .map(PathBuf::from);

    let model_paths = cfg
        .config_json
        .get("model_paths")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(PathBuf::from))
                .collect::<Vec<_>>()
        });

    if model_path.is_none() && model_paths.is_none() {
        return Err(LlmError::BackendUnavailable {
            backend: cfg.name.clone(),
            reason: "config_json missing 'model_path' or 'model_paths' for llama-cpp backend"
                .to_string(),
        });
    }

    let device: AcceleratorDevice = cfg
        .config_json
        .get("device")
        .and_then(|v| v.as_str())
        .and_then(|s| serde_json::from_value(serde_json::Value::String(s.to_string())).ok())
        .unwrap_or_default();

    let embedded_cfg = EmbeddedBackendConfig {
        name: cfg.name.clone(),
        model_path,
        model_paths,
        quantization: String::new(),
        device,
    };

    let backend = EmbeddedBackend::load(&embedded_cfg).await?;
    Ok(Arc::new(backend) as Arc<dyn CompletionModel>)
}

/// Retourne `BackendUnavailable` quand la feature `"local"` n'est pas compilée.
#[cfg(not(feature = "local"))]
async fn instantiate_embedded_backend(
    cfg: &LlmBackendConfig,
) -> Result<Arc<dyn CompletionModel>, LlmError> {
    Err(LlmError::BackendUnavailable {
        backend: cfg.name.clone(),
        reason: "provider 'llama-cpp' requires feature 'local'".to_string(),
    })
}

/// Instancie un backend cloud (OpenAI-compatible ou Anthropic) depuis la config SQLite.
///
/// Résout la clé API depuis `config_json["api_key"]` si présente.
#[cfg(feature = "cloud")]
async fn instantiate_cloud_backend(
    cfg: &LlmBackendConfig,
    provider: &LlmProvider,
    cancel: CancellationToken,
) -> Result<Arc<dyn CompletionModel>, LlmError> {
    let api_key = extract_api_key_value(cfg)?;

    let default_url = match provider {
        LlmProvider::OpenAi => "https://api.openai.com/v1",
        LlmProvider::Mistral => "https://api.mistral.ai/v1",
        LlmProvider::Ollama => "http://localhost:11434/v1",
        LlmProvider::Anthropic => "https://api.anthropic.com",
        LlmProvider::LlamaCpp => unreachable!("LlamaCpp handled by instantiate_embedded_backend"),
    };

    let base_url = extract_base_url(cfg, default_url);

    let api_cfg = ApiBackendConfig {
        name: cfg.name.clone(),
        api_url: base_url,
        api_key_env: String::new(), // clé déjà résolue
        model: cfg.model.clone(),
    };

    if matches!(provider, LlmProvider::Anthropic) {
        return Ok(Arc::new(AnthropicClient::new(
            &api_cfg,
            api_key,
            HashMap::new(),
            cancel,
        )) as Arc<dyn CompletionModel>);
    }

    Ok(
        Arc::new(OpenAICompatibleClient::new(&api_cfg, api_key, cancel))
            as Arc<dyn CompletionModel>,
    )
}

/// Retourne `BackendUnavailable` quand la feature `"cloud"` n'est pas compilée.
#[cfg(not(feature = "cloud"))]
async fn instantiate_cloud_backend(
    cfg: &LlmBackendConfig,
    provider: &LlmProvider,
    _cancel: CancellationToken,
) -> Result<Arc<dyn CompletionModel>, LlmError> {
    Err(LlmError::BackendUnavailable {
        backend: cfg.name.clone(),
        reason: format!("provider '{}' requires feature 'cloud'", provider),
    })
}

/// Extrait et résout la clé API depuis `config_json["api_key"]`.
///
/// - Absent → `Ok("")` (Ollama-style, pas d'authentification)
/// - `"${VAR}"` → résout via `std::env::var(VAR)` ; erreur si la variable est absente
/// - Valeur littérale → retournée telle quelle
#[cfg(feature = "cloud")]
fn extract_api_key_value(cfg: &LlmBackendConfig) -> Result<String, LlmError> {
    let raw = match cfg.config_json.get("api_key").and_then(|v| v.as_str()) {
        None => return Ok(String::new()),
        Some(s) => s.to_string(),
    };

    if let Some(var_name) = raw.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        std::env::var(var_name).map_err(|_| LlmError::ApiKeyMissing {
            var: var_name.to_string(),
        })
    } else {
        Ok(raw)
    }
}

/// Extrait l'URL de base depuis `config_json["base_url"]`, ou retourne `default`.
#[cfg(feature = "cloud")]
fn extract_base_url(cfg: &LlmBackendConfig, default: &str) -> String {
    cfg.config_json
        .get("base_url")
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use std::pin::Pin;

    use futures::Stream;

    use crate::types::{
        CompletionRequest, CompletionResponse, FinishReason, StreamChunk, TokenUsage,
    };

    // ── Mock ─────────────────────────────────────────────────────────────────

    struct MockCompletionModel {
        name: String,
    }

    impl Default for MockCompletionModel {
        fn default() -> Self {
            Self {
                name: "mock".to_owned(),
            }
        }
    }

    #[async_trait::async_trait]
    impl CompletionModel for MockCompletionModel {
        async fn complete(&self, _req: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            Ok(CompletionResponse {
                content: "mock response".to_owned(),
                tool_calls: vec![],
                usage: TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    cost_usd: None,
                    ..Default::default()
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 1,
                ttft_ms: None,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
        {
            Ok(Box::pin(futures::stream::once(async {
                Ok(StreamChunk::Text("mock chunk".to_owned()))
            })))
        }

        fn is_available(&self) -> bool {
            true
        }

        fn backend_name(&self) -> &str {
            &self.name
        }

        fn model_id(&self) -> &str {
            &self.name
        }
    }

    fn make_mock_backend(name: &str) -> Arc<dyn CompletionModel> {
        Arc::new(MockCompletionModel {
            name: name.to_owned(),
        })
    }

    fn make_test_router(
        backends: HashMap<String, Arc<dyn CompletionModel>>,
        default: &str,
    ) -> LlmRouter {
        LlmRouter {
            backends,
            default: default.into(),
            routing: None,
            cancellation_token: CancellationToken::new(),
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        }
    }

    fn make_routing_router(precise: &str, fast: &str) -> LlmRouter {
        let mut backends = HashMap::new();
        backends.insert(precise.to_owned(), make_mock_backend(precise));
        if fast != precise {
            backends.insert(fast.to_owned(), make_mock_backend(fast));
        }
        let routing = Some(LlmRoutingConfig {
            precise: precise.to_owned(),
            fast: fast.to_owned(),
        });
        LlmRouter {
            default: precise.to_owned(),
            backends,
            routing,
            cancellation_token: CancellationToken::new(),
            session_budget: Arc::new(Mutex::new(SessionBudgetTracker::default())),
        }
    }

    // ── Tests route() ────────────────────────────────────────────────────────

    // GIVEN router with "local-code" and "mistral-small", default = "mistral-small"
    // WHEN route(Some("local-code"))
    // THEN the "local-code" backend is returned
    #[test]
    fn test_ac1_route_to_explicit_backend() {
        let mut backends = HashMap::new();
        backends.insert("local-code".into(), make_mock_backend("local-code"));
        backends.insert("mistral-small".into(), make_mock_backend("mistral-small"));
        let router = make_test_router(backends, "mistral-small");

        let backend = router.route(Some("local-code"));
        assert_eq!(backend.backend_name(), "local-code");
    }

    // GIVEN router with default = "local-code"
    // WHEN route(None)
    // THEN the default backend is returned
    #[test]
    fn test_ac2_route_none_returns_default() {
        let mut backends = HashMap::new();
        backends.insert("local-code".into(), make_mock_backend("local-code"));
        let router = make_test_router(backends, "local-code");

        let backend = router.route(None);
        assert_eq!(backend.backend_name(), "local-code");
    }

    // GIVEN router without "phantom"
    // WHEN route(Some("phantom"))
    // THEN the default backend is returned (warning emitted)
    #[test]
    fn test_ac3_unknown_backend_falls_back_to_default() {
        let mut backends = HashMap::new();
        backends.insert("local-code".into(), make_mock_backend("local-code"));
        let router = make_test_router(backends, "local-code");

        let backend = router.route(Some("phantom"));
        assert_eq!(backend.backend_name(), "local-code");
    }

    // GIVEN router with 2 backends
    // WHEN backend_names()
    // THEN sorted list of names returned
    #[test]
    fn test_backend_names_sorted() {
        let mut backends = HashMap::new();
        backends.insert("z-backend".into(), make_mock_backend("z-backend"));
        backends.insert("a-backend".into(), make_mock_backend("a-backend"));
        let router = make_test_router(backends, "a-backend");

        let names = router.backend_names();
        assert_eq!(names, vec!["a-backend", "z-backend"]);
    }

    // GIVEN a LlmBackendRepository with 2 enabled + 1 disabled Ollama backend
    // WHEN from_repository(&repo).await
    // THEN the router contains exactly 2 backends (the disabled one is excluded)
    #[cfg(feature = "cloud")]
    #[tokio::test]
    async fn test_ac4_from_repository_loads_only_enabled() {
        use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmProvider};
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();

        let make_ollama = |name: &str, enabled: bool, is_default: bool| LlmBackendConfig {
            name: name.to_string(),
            provider: LlmProvider::Ollama,
            model: "llama3".to_string(),
            config_json: serde_json::json!({ "base_url": "http://localhost:11434/v1" }),
            enabled,
            is_default,
        };

        repo.save(&make_ollama("ollama-default", true, true))
            .unwrap();
        repo.save(&make_ollama("ollama-extra", true, false))
            .unwrap();
        repo.save(&make_ollama("ollama-disabled", false, false))
            .unwrap();

        let router = LlmRouter::from_repository(&repo)
            .await
            .expect("from_repository should succeed");

        let names = router.backend_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"ollama-default".to_string()));
        assert!(names.contains(&"ollama-extra".to_string()));
        assert!(!names.contains(&"ollama-disabled".to_string()));
    }

    // GIVEN a repository with no default backend
    // WHEN from_repository(&repo).await
    // THEN BackendUnavailable is returned
    #[tokio::test]
    async fn test_from_repository_no_default_returns_error() {
        use apollia_core::{LlmBackendConfig, LlmBackendRepository, LlmProvider};
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let repo = LlmBackendRepository::open(&dir.path().join("system.db")).unwrap();

        // empty repo — no default
        let result = LlmRouter::from_repository(&repo).await;
        assert!(matches!(result, Err(LlmError::BackendUnavailable { .. })));

        // backend with is_default=false — still no default
        repo.save(&LlmBackendConfig {
            name: "orphan".to_string(),
            provider: LlmProvider::Ollama,
            model: "llama3".to_string(),
            config_json: serde_json::json!({}),
            enabled: true,
            is_default: false,
        })
        .unwrap();

        let result2 = LlmRouter::from_repository(&repo).await;
        assert!(matches!(result2, Err(LlmError::BackendUnavailable { .. })));
    }

    // ── Tests : get, list, clone, error cases ────────────────────────────────

    // GIVEN un LlmRouter avec default = "local" et un backend "local"
    // WHEN on appelle get(None)
    // THEN Some(backend) avec backend_name() == "local" est retourné
    #[tokio::test]
    async fn test_ac3_get_none_returns_default() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("local".into(), make_mock_backend("local"));
        let router = make_test_router(backends, "local");

        // WHEN
        let result = router.get(None);

        // THEN
        assert!(
            result.is_some(),
            "get(None) doit retourner Some pour le backend défaut"
        );
        assert_eq!(
            result.unwrap().backend_name(),
            "local",
            "le backend retourné doit être le backend défaut"
        );
    }

    // GIVEN un LlmRouter avec un backend "anthropic"
    // WHEN on appelle get(Some("anthropic"))
    // THEN Some(arc) est retourné avec backend_name() == "anthropic"
    #[tokio::test]
    async fn test_ac4_get_named_backend() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("anthropic".into(), make_mock_backend("anthropic"));
        let router = make_test_router(backends, "anthropic");

        // WHEN
        let result = router.get(Some("anthropic"));

        // THEN
        assert!(
            result.is_some(),
            "get(Some(\"anthropic\")) doit retourner Some"
        );
        assert_eq!(result.unwrap().backend_name(), "anthropic");
    }

    // GIVEN un LlmRouter sans backend "inexistant"
    // WHEN on appelle get(Some("inexistant"))
    // THEN None est retourné
    #[tokio::test]
    async fn test_ac5_get_unknown_returns_none() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("local".into(), make_mock_backend("local"));
        let router = make_test_router(backends, "local");

        // WHEN / THEN
        assert!(
            router.get(Some("inexistant")).is_none(),
            "get(Some(\"inexistant\")) doit retourner None pour un backend inconnu"
        );
    }

    // GIVEN un LlmRouter avec 2 backends ("a" et "b")
    // WHEN on appelle list()
    // THEN un Vec de longueur 2 est retourné
    #[tokio::test]
    async fn test_router_list_returns_all_backends() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("a".into(), make_mock_backend("a"));
        backends.insert("b".into(), make_mock_backend("b"));
        let router = make_test_router(backends, "a");

        // WHEN
        let list = router.list();

        // THEN
        assert_eq!(
            list.len(),
            2,
            "list() doit retourner autant d'entrées que de backends"
        );
    }

    // GIVEN un LlmRouter cloné
    // WHEN on interroge le clone
    // THEN il partage les mêmes backends via Arc (refcount)
    #[tokio::test]
    async fn test_router_clone_shares_backends() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("local".into(), make_mock_backend("local"));
        let router = make_test_router(backends, "local");

        // WHEN
        let cloned = router.clone();

        // THEN
        assert!(
            cloned.get(None).is_some(),
            "le clone doit avoir accès aux mêmes backends"
        );
        assert_eq!(cloned.list().len(), 1);
    }

    // GIVEN un LlmConfig avec default = "local" mais backends vide
    // WHEN on appelle LlmRouter::from_config(&config).await
    // THEN Err(LlmError::BackendUnavailable { backend: "local", .. }) est retourné
    #[tokio::test]
    async fn test_ac6_from_config_errors_if_default_missing() {
        // GIVEN
        let config = LlmConfig {
            default: "local".to_owned(),
            backends: vec![],
            observability: ObservabilityConfig::default(),
            routing: None,
            pricing_overrides: HashMap::new(),
            cost_alert_threshold_usd: None,
            vertex: None,
        };

        // WHEN
        let result = LlmRouter::from_config(&config).await;

        // THEN
        assert!(
            matches!(
                result,
                Err(LlmError::BackendUnavailable { ref backend, .. }) if backend == "local"
            ),
            "from_config doit retourner BackendUnavailable si le backend défaut est absent"
        );
    }

    // ── Tests observabilité ────────────────────────────────────────────────────

    // GIVEN un LlmRouter avec un mock backend et un EventBusSender
    // WHEN on appelle complete_with_observability(None, req, Some(&tx), &obs)
    // THEN un événement LlmCallCompleted est reçu sur le bus avec backend == "mock"
    #[tokio::test]
    async fn test_ac1_llm_call_completed_emitted() {
        use apollia_core::events::RuntimeEvent;
        use tokio::sync::broadcast;

        // GIVEN
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);
        let mut backends = HashMap::new();
        backends.insert(
            "mock".into(),
            Arc::new(MockCompletionModel::default()) as Arc<dyn CompletionModel>,
        );
        let router = make_test_router(backends, "mock");
        let req = CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("test")],
            ..Default::default()
        };
        let obs = ObservabilityConfig::default();

        // WHEN
        router
            .complete_with_observability(None, req, Some(&tx), &obs)
            .await
            .expect("complete_with_observability ne doit pas échouer avec un mock valide");

        // THEN
        let event = rx
            .try_recv()
            .expect("un événement doit être présent dans le bus");
        assert!(
            matches!(
                event,
                RuntimeEvent::LlmCallCompleted { ref backend, .. } if backend == "mock"
            ),
            "l'événement reçu doit être LlmCallCompleted avec backend == \"mock\", obtenu: {event:?}"
        );
    }

    // GIVEN un router avec debug_log_prompt = false
    // WHEN on appelle complete_with_observability() avec un message "secret_payload_xyz"
    // THEN la fonction ne panic pas et retourne Ok — le prompt n'est pas loggué à INFO
    #[tokio::test]
    async fn test_ac4_prompt_not_logged_at_info_without_debug_flag() {
        // GIVEN
        let obs = ObservabilityConfig {
            debug_log_prompt: false,
            ..Default::default()
        };
        let req = CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("secret_payload_xyz")],
            ..Default::default()
        };
        let mut backends = HashMap::new();
        backends.insert(
            "mock".into(),
            Arc::new(MockCompletionModel::default()) as Arc<dyn CompletionModel>,
        );
        let router = make_test_router(backends, "mock");

        // WHEN — ne doit pas panic, bus absent est acceptable (Option::None)
        let result = router
            .complete_with_observability(None, req, None, &obs)
            .await;

        // THEN
        assert!(
            result.is_ok(),
            "complete_with_observability doit retourner Ok même sans bus : {result:?}"
        );
    }

    // GIVEN un LlmRouter avec EventBusSender et backends vide (default absent)
    // WHEN on appelle from_config_with_bus
    // THEN Err(LlmError::BackendUnavailable) est retourné sans crash
    // (variante sans feature "local" : vérifie que le router ne crash pas)
    #[tokio::test]
    async fn test_ac3_from_config_with_bus_no_backends_returns_error() {
        use apollia_core::events::RuntimeEvent;
        use tokio::sync::broadcast;

        // GIVEN
        let (tx, _rx) = broadcast::channel::<RuntimeEvent>(16);
        let config = LlmConfig {
            default: "local".to_owned(),
            backends: vec![],
            observability: ObservabilityConfig::default(),
            routing: None,
            pricing_overrides: HashMap::new(),
            cost_alert_threshold_usd: None,
            vertex: None,
        };

        // WHEN
        let result = LlmRouter::from_config_with_bus(&config, Some(tx)).await;

        // THEN — erreur propre, pas de crash
        assert!(
            matches!(
                result,
                Err(LlmError::BackendUnavailable { ref backend, .. }) if backend == "local"
            ),
            "from_config_with_bus doit retourner BackendUnavailable si aucun backend n'est disponible"
        );
    }

    // ── Tests routing ────────────────────────────────────────────────────────

    // GIVEN routing config { precise: "claude-opus-4-6", fast: "claude-haiku-4-5-20251001" }
    // WHEN route_precise()
    // THEN backend "claude-opus-4-6" est sélectionné
    #[tokio::test]
    async fn router_precise_selects_configured_backend() {
        let router = make_routing_router("claude-opus-4-6", "claude-haiku-4-5-20251001");

        let backend = router
            .route_precise()
            .expect("route_precise should succeed");
        assert_eq!(backend.backend_name(), "claude-opus-4-6");
    }

    // GIVEN routing config { precise: "claude-opus-4-6", fast: "claude-haiku-4-5-20251001" }
    // WHEN route_fast()
    // THEN backend "claude-haiku-4-5-20251001" est sélectionné
    #[tokio::test]
    async fn router_fast_selects_configured_backend() {
        let router = make_routing_router("claude-opus-4-6", "claude-haiku-4-5-20251001");

        let backend = router.route_fast().expect("route_fast should succeed");
        assert_eq!(backend.backend_name(), "claude-haiku-4-5-20251001");
    }

    // GIVEN pas de section [llm.routing] (routing: None)
    // WHEN route_precise() est appelé
    // THEN Err(RoutingConfigMissing)
    #[tokio::test]
    async fn router_errors_on_missing_routing_config() {
        let mut backends = HashMap::new();
        backends.insert("default".to_owned(), make_mock_backend("default"));
        let router = make_test_router(backends, "default");

        assert!(
            matches!(router.route_precise(), Err(LlmError::RoutingConfigMissing)),
            "route_precise() must return RoutingConfigMissing when routing is None"
        );
        assert!(
            matches!(router.route_fast(), Err(LlmError::RoutingConfigMissing)),
            "route_fast() must return RoutingConfigMissing when routing is None"
        );
    }

    // US-SP42-040 — primary fails, secondary succeeds, LlmFallbackTriggered emitted
    #[tokio::test]
    async fn router_emits_fallback_event_on_primary_failure() {
        use apollia_core::events::RuntimeEvent;
        use tokio::sync::broadcast;

        struct FailingBackend {
            name: String,
        }
        #[async_trait::async_trait]
        impl CompletionModel for FailingBackend {
            async fn complete(
                &self,
                _req: CompletionRequest,
            ) -> Result<CompletionResponse, LlmError> {
                Err(LlmError::InferenceError("primary down".to_string()))
            }
            async fn stream(
                &self,
                _req: CompletionRequest,
            ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk, LlmError>> + Send>>, LlmError>
            {
                Err(LlmError::InferenceError("primary down".to_string()))
            }
            fn is_available(&self) -> bool {
                true
            }
            fn backend_name(&self) -> &str {
                &self.name
            }
            fn model_id(&self) -> &str {
                &self.name
            }
        }

        // GIVEN a router with a failing primary and a healthy secondary
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();
        backends.insert(
            "primary".into(),
            Arc::new(FailingBackend {
                name: "primary".into(),
            }),
        );
        backends.insert("secondary".into(), make_mock_backend("secondary"));
        let router = make_test_router(backends, "primary");
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);
        let req = CompletionRequest {
            messages: vec![crate::types::ChatMessage::user("hi")],
            ..Default::default()
        };

        // WHEN complete_with_fallback
        let response = router
            .complete_with_fallback(
                "primary",
                &["secondary"],
                req,
                Some(&tx),
                &ObservabilityConfig::default(),
            )
            .await
            .expect("fallback should succeed");

        // THEN response comes from secondary
        assert_eq!(response.content, "mock response");

        // AND LlmFallbackTriggered was emitted
        let mut saw_fallback = false;
        while let Ok(evt) = rx.try_recv() {
            if let RuntimeEvent::LlmFallbackTriggered {
                from_provider,
                to_provider,
                ..
            } = evt
            {
                assert_eq!(from_provider, "primary");
                assert_eq!(to_provider, "secondary");
                saw_fallback = true;
            }
        }
        assert!(saw_fallback, "LlmFallbackTriggered should have been emitted");
    }

    // GIVEN routing config { precise: "claude-opus-4-6", fast: "claude-opus-4-6" }
    // WHEN route_precise() et route_fast()
    // THEN le même backend "claude-opus-4-6" est retourné dans les deux cas
    #[tokio::test]
    async fn router_same_backend_for_precise_and_fast_when_identical() {
        let router = make_routing_router("claude-opus-4-6", "claude-opus-4-6");

        let precise = router
            .route_precise()
            .expect("route_precise should succeed");
        let fast = router.route_fast().expect("route_fast should succeed");

        assert_eq!(precise.backend_name(), "claude-opus-4-6");
        assert_eq!(fast.backend_name(), "claude-opus-4-6");
        assert_eq!(precise.backend_name(), fast.backend_name());
    }
}
