//! `LlmRouter` — dispatche les requêtes vers le bon backend par nom.
//!
//! Construit au démarrage du Supervisor (position 5, avant `TaskRouter`)
//! via [`LlmRouter::from_config`]. Partageable via `Arc<LlmRouter>` grâce
//! à `Clone + Send + Sync`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use apollia_core::events::{EventBusSender, RuntimeEvent};

use crate::types::{BackendInfo, CompletionModel, CompletionRequest, CompletionResponse, LlmError};

#[cfg(feature = "local")]
use crate::backends::embedded::{EmbeddedBackend, EmbeddedBackendConfig};

#[cfg(feature = "cloud")]
use crate::backends::anthropic::AnthropicClient;

#[cfg(feature = "cloud")]
use crate::backends::openai::{ApiBackendConfig, OpenAICompatibleClient};

// ─────────────────────────────────────────────
// Configuration
// ─────────────────────────────────────────────

/// Configuration LLM désérialisée depuis la section `[llm]` de `apollia.toml`.
///
/// Passée à [`LlmRouter::from_config`] au démarrage du Supervisor.
/// Le champ `default` désigne le backend utilisé quand `get(None)` est appelé.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct LlmConfig {
    /// Nom du backend par défaut (doit exister dans `backends`).
    pub default: String,
    /// Liste des backends à instancier dans `[[llm.backends]]`.
    pub backends: Vec<BackendConfig>,
    /// Paramètres d'observabilité (tokens, latence, coût, prompt debug).
    #[serde(default)]
    pub observability: ObservabilityConfig,
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
    fn name(&self) -> &str {
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
            BackendKind::Embedded(cfg) => cfg.model_path.to_string_lossy().into_owned(),
            #[cfg(feature = "cloud")]
            BackendKind::Api(cfg) => cfg.api_url.clone(),
        }
    }
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
/// `LlmRouter` est `Clone + Send + Sync` — partageable via `Arc<LlmRouter>`
/// entre les composants du runtime (agit comme un catalogue en lecture seule).
///
/// `Debug` est implémenté manuellement : `Arc<dyn CompletionModel>` n'implémente
/// pas `Debug` (le trait objet ne l'exporte pas).
#[derive(Clone)]
pub struct LlmRouter {
    backends: HashMap<String, Arc<dyn CompletionModel>>,
    default: String,
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
                            Arc::new(AnthropicClient::new(cfg, key))
                        } else {
                            Arc::new(OpenAICompatibleClient::new(cfg, key))
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

        // AC-6 : le backend par défaut doit être disponible après la boucle.
        if !backends.contains_key(&config.default) {
            return Err(LlmError::BackendUnavailable {
                backend: config.default.clone(),
                reason: "not configured".to_owned(),
            });
        }

        Ok(Self {
            backends,
            default: config.default.clone(),
        })
    }

    /// Construit le router depuis la configuration avec observabilité EventBus.
    ///
    /// Variante de [`from_config`](Self::from_config) à utiliser par le Supervisor (STORY-060).
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
        let mut backends: HashMap<String, Arc<dyn CompletionModel>> = HashMap::new();

        for backend_cfg in &config.backends {
            let name = backend_cfg.name().to_owned();
            let model_path = backend_cfg.model_path_hint();

            // AC-2 : émettre LlmModelLoading avant chaque tentative de chargement.
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
                            Arc::new(AnthropicClient::new(cfg, key))
                        } else {
                            Arc::new(OpenAICompatibleClient::new(cfg, key))
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
                    // AC-2 : émettre LlmModelReady après succès.
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
                    // AC-3 : émettre LlmModelFailed — backend ignoré, pas de crash.
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

        if !backends.contains_key(&config.default) {
            return Err(LlmError::BackendUnavailable {
                backend: config.default.clone(),
                reason: "not configured".to_owned(),
            });
        }

        Ok(Self {
            backends,
            default: config.default.clone(),
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

        // AC-4 : log du prompt uniquement à TRACE — jamais à INFO.
        if obs.debug_log_prompt {
            tracing::trace!(prompt = ?req.messages, "llm prompt");
        }

        let started = Instant::now();
        let response = backend.complete(req).await?;
        let latency_ms = started.elapsed().as_millis() as u64;

        // AC-1 : émission fire-and-forget — erreurs send() silencieusement ignorées.
        if let Some(b) = bus {
            let _ = b.send(RuntimeEvent::LlmCallCompleted {
                backend: backend_key.to_owned(),
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

    /// Retourne le backend par nom, ou le backend défaut si `name` est `None`.
    ///
    /// Retourne `None` si le backend demandé n'est pas dans le router.
    pub fn get(&self, name: Option<&str>) -> Option<Arc<dyn CompletionModel>> {
        let key = name.unwrap_or(&self.default);
        self.backends.get(key).cloned()
    }

    /// Crée un `LlmRouter` vide sans aucun backend — pour les tests unitaires.
    ///
    /// Utilisé par STORY-059 pour tester les chemins de dégradation :
    /// `ctx.llm = None` et `AgentDegraded` sur l'EventBus.
    pub fn empty() -> Self {
        Self {
            backends: HashMap::new(),
            default: String::new(),
        }
    }

    /// Retourne le nom du backend par défaut configuré dans `apollia.toml`.
    pub fn default_name(&self) -> &str {
        &self.default
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
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use std::pin::Pin;

    use futures::Stream;

    use crate::types::{CompletionRequest, CompletionResponse, FinishReason, TokenUsage};

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
                },
                finish_reason: FinishReason::Stop,
                latency_ms: 1,
            })
        }

        async fn stream(
            &self,
            _req: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError>
        {
            Ok(Box::pin(futures::stream::once(async {
                Ok("mock chunk".to_owned())
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

    // ── Tests AC-3/AC-4/AC-5 + list + clone + AC-6 ───────────────────────────

    // GIVEN un LlmRouter avec default = "local" et un backend "local"
    // WHEN on appelle get(None)
    // THEN Some(backend) avec backend_name() == "local" est retourné
    #[tokio::test]
    async fn test_ac3_get_none_returns_default() {
        // GIVEN
        let mut backends = HashMap::new();
        backends.insert("local".into(), make_mock_backend("local"));
        let router = LlmRouter {
            backends,
            default: "local".into(),
        };

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
        let router = LlmRouter {
            backends,
            default: "anthropic".into(),
        };

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
        let router = LlmRouter {
            backends,
            default: "local".into(),
        };

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
        let router = LlmRouter {
            backends,
            default: "a".into(),
        };

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
        let router = LlmRouter {
            backends,
            default: "local".into(),
        };

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

    // ── Tests observabilité (STORY-056) ──────────────────────────────────────

    // GIVEN un LlmRouter avec un mock backend et un EventBusSender
    // WHEN on appelle complete_with_observability(None, req, Some(&tx), &obs)
    // THEN un événement LlmCallCompleted est reçu sur le bus avec backend == "mock"
    // AC-1
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
        let router = LlmRouter {
            backends,
            default: "mock".into(),
        };
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
    // AC-4
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
        let router = LlmRouter {
            backends,
            default: "mock".into(),
        };

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
    // AC-3 (variante sans feature "local" : vérifie que le router ne crash pas)
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
}
