//! Routes REST pour la gestion des triggers.
//!
//! Expose les opérations CRUD sur les triggers via l'API REST du runtime :
//! - `GET  /api/v1/triggers`          — liste de tous les triggers (STORY-074)
//! - `GET  /api/v1/triggers/:id`       — statut détaillé d'un trigger (STORY-074)
//! - `POST /api/v1/triggers/:id/fire`  — déclenchement immédiat (STORY-074)
//! - `POST /api/v1/triggers/:id/enable`  — activation (STORY-074)
//! - `POST /api/v1/triggers/:id/disable` — désactivation (STORY-074)
//! - `GET  /api/v1/triggers/:id/logs`  — historique SQLite (STORY-074)
//! - `POST /api/v1/triggers/reload`    — hot reload (STORY-073)
//!
//! **Codes de retour partagés :**
//! - `503` — `TriggerEngine` non disponible.
//! - `404` — trigger inconnu.
//! - `200` — succès.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use apollia_triggers::{
    parse_triggers_from_toml_str, TriggerEngineError, TriggerHistoryEntry, TriggerTomlError,
};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

// ─── Response types ────────────────────────────────────────────────────────

/// Corps de réponse en cas de succès pour le rechargement.
#[derive(Serialize)]
pub struct ReloadResponse {
    /// Nombre de triggers actifs après rechargement.
    pub reloaded: usize,
}

/// Corps de réponse en cas d'erreur.
#[derive(Serialize)]
struct ErrorResponse {
    /// Description de l'erreur.
    error: String,
}

/// Réponse pour `GET /api/v1/triggers/:id` — statut détaillé.
#[derive(Serialize)]
pub struct TriggerDetailResponse {
    /// Identifiant du trigger.
    pub id: String,
    /// Nom de l'agent cible.
    pub agent: String,
    /// Type de source.
    pub source_kind: String,
    /// Détail de la configuration source (ex : expression cron, intervalle).
    pub source_detail: String,
    /// Politique quand l'agent est occupé.
    pub on_busy: String,
    /// Trigger actif ou non.
    pub enabled: bool,
    /// Nombre de fires réussis.
    pub fire_count: u64,
    /// Nombre de skips.
    pub skip_count: u64,
    /// Horodatage du dernier fire (RFC3339) ou `null`.
    pub last_fired: Option<String>,
}

/// Réponse pour `POST /api/v1/triggers/:id/fire`.
#[derive(Serialize)]
pub struct FireResponse {
    /// Identifiant de la tâche soumise.
    pub task_id: String,
}

/// Réponse pour enable/disable.
#[derive(Serialize)]
pub struct OkResponse {
    /// Message de confirmation.
    pub ok: bool,
}

/// Réponse pour `GET /api/v1/triggers/:id/logs`.
#[derive(Serialize)]
pub struct LogsResponse {
    /// Entrées d'historique.
    pub entries: Vec<TriggerHistoryEntry>,
}

/// Paramètres de query pour `GET /api/v1/triggers/:id/logs`.
#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    /// Nombre maximum d'entrées à retourner (défaut : 20).
    #[serde(default = "default_last")]
    pub last: usize,
}

fn default_last() -> usize {
    20
}

// ─── Handlers STORY-074 ────────────────────────────────────────────────────

/// `GET /api/v1/triggers` — liste de tous les triggers avec leur statut.
pub async fn list_triggers<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> impl IntoResponse {
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"error": "TriggerEngine not available"})),
            )
                .into_response();
        }
    };

    let statuses = engine.list().await;
    Json(serde_json::json!({ "triggers": statuses })).into_response()
}

/// `GET /api/v1/triggers/:id` — statut détaillé d'un trigger.
pub async fn get_trigger<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "TriggerEngine not available".into(),
                }),
            )
                .into_response();
        }
    };

    // Récupère la définition complète pour les champs non présents dans TriggerStatus.
    let def = match engine.get_definition(&id).await {
        Some(d) => d,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("trigger '{id}' not found"),
                }),
            )
                .into_response();
        }
    };

    // Récupère le statut (fire_count, skip_count, last_fired) depuis la liste.
    let status = engine
        .list()
        .await
        .into_iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| apollia_triggers::TriggerStatus {
            id: def.id.clone(),
            agent: def.agent.clone(),
            source_kind: source_detail_kind(&def.source),
            enabled: def.enabled,
            fire_count: 0,
            skip_count: 0,
            last_fired: None,
        });

    let (source_kind, source_detail) = source_kind_and_detail(&def.source);
    let on_busy = match def.on_busy {
        apollia_triggers::OnBusyPolicy::Queue => "queue",
        apollia_triggers::OnBusyPolicy::Drop => "drop",
    };

    let detail = TriggerDetailResponse {
        id: status.id,
        agent: status.agent,
        source_kind,
        source_detail,
        on_busy: on_busy.to_string(),
        enabled: status.enabled,
        fire_count: status.fire_count,
        skip_count: status.skip_count,
        last_fired: status.last_fired.map(|dt| dt.to_rfc3339()),
    };

    Json(detail).into_response()
}

/// `POST /api/v1/triggers/:id/fire` — déclenchement immédiat.
pub async fn fire_trigger<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "TriggerEngine not available".into(),
                }),
            )
                .into_response();
        }
    };

    match engine.fire_now(&id).await {
        Ok(task_id) => Json(FireResponse {
            task_id: task_id.to_string(),
        })
        .into_response(),
        Err(TriggerEngineError::NotFound { .. }) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("trigger '{id}' not found"),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// `POST /api/v1/triggers/:id/enable` — active un trigger désactivé.
pub async fn enable_trigger<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "TriggerEngine not available".into(),
                }),
            )
                .into_response();
        }
    };

    match engine.enable(&id).await {
        Ok(()) => Json(OkResponse { ok: true }).into_response(),
        Err(TriggerEngineError::NotFound { .. }) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("trigger '{id}' not found"),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// `POST /api/v1/triggers/:id/disable` — désactive un trigger actif.
pub async fn disable_trigger<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "TriggerEngine not available".into(),
                }),
            )
                .into_response();
        }
    };

    match engine.disable(&id).await {
        Ok(()) => Json(OkResponse { ok: true }).into_response(),
        Err(TriggerEngineError::NotFound { .. }) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("trigger '{id}' not found"),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
            .into_response(),
    }
}

/// `GET /api/v1/triggers/:id/logs` — historique des déclenchements depuis SQLite.
///
/// Le paramètre de query `?last=N` contrôle le nombre d'entrées (défaut : 20).
pub async fn get_trigger_logs<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<String>,
    Query(params): Query<LogsQuery>,
) -> impl IntoResponse {
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: "TriggerEngine not available".into(),
                }),
            )
                .into_response();
        }
    };

    let entries = engine.query_history(&id, params.last).await;
    Json(LogsResponse { entries }).into_response()
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Retourne la chaîne de type de source pour usage dans `TriggerStatus` fallback.
fn source_detail_kind(source: &apollia_triggers::TriggerSourceConfig) -> String {
    source_kind_and_detail(source).0
}

/// Retourne `(kind, detail)` depuis une [`TriggerSourceConfig`].
fn source_kind_and_detail(source: &apollia_triggers::TriggerSourceConfig) -> (String, String) {
    use apollia_triggers::TriggerSourceConfig;
    match source {
        TriggerSourceConfig::Cron { schedule } => ("cron".into(), schedule.clone()),
        TriggerSourceConfig::Interval { every } => ("interval".into(), every.clone()),
        TriggerSourceConfig::Oneshot { fire_at } => ("oneshot".into(), fire_at.to_rfc3339()),
        TriggerSourceConfig::FileWatch { path, events } => {
            let evts: Vec<_> = events.iter().map(|e| e.to_string()).collect();
            (
                "file_watch".into(),
                format!("{} [{}]", path.display(), evts.join(",")),
            )
        }
        TriggerSourceConfig::Webhook { .. } => ("webhook".into(), String::new()),
    }
}

// ─── Handler ──────────────────────────────────────────────────────────────

/// Handler axum pour `POST /api/v1/triggers/reload`.
///
/// 1. Vérifie que le `TriggerEngine` est disponible (`503` sinon).
/// 2. Vérifie que `config_path` est connu dans [`AppState`] (`503` sinon).
/// 3. Relit `apollia.toml` depuis le disque (`422` si illisible).
/// 4. Parse et valide la section `[[triggers]]` (`422` si invalide).
/// 5. Appelle [`TriggerEngineHandle::reload`] — les compteurs SQLite sont préservés (AC-2).
/// 6. Retourne `200 { "reloaded": N }`.
pub async fn reload_triggers<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> impl IntoResponse {
    // 0. TriggerEngine disponible ?
    let engine = match &state.trigger_engine {
        Some(e) => e.clone(),
        None => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };

    // 1. config_path connu ?
    let config_path = match &state.config_path {
        Some(p) => p.clone(),
        None => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };

    // 2. Lire apollia.toml depuis le disque (AC-3).
    let content = match std::fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ErrorResponse {
                    error: format!("cannot read config file: {e}"),
                }),
            )
                .into_response();
        }
    };

    // 3. Parser et valider — 422 si invalide, triggers actuels non interrompus (AC-5).
    let definitions = match parse_triggers_from_toml_str(&content) {
        Ok(defs) => defs,
        Err(TriggerTomlError::Parse(e)) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ErrorResponse {
                    error: format!("invalid TOML: {e}"),
                }),
            )
                .into_response();
        }
        Err(TriggerTomlError::InvalidTrigger { id, reason }) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ErrorResponse {
                    error: format!("invalid trigger '{id}': {reason}"),
                }),
            )
                .into_response();
        }
    };

    let count = definitions.iter().filter(|d| d.enabled).count();

    // 4. Recharger le moteur — compteurs SQLite préservés (AC-2).
    engine.reload(definitions).await;

    // 5. Répondre 200 avec le nombre de triggers actifs (AC-3).
    Json(ReloadResponse { reloaded: count }).into_response()
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::server::AppState;
    use crate::coordinator::ExecutionBackend;
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::{AIPInput, AIPResult, AIPTask, TaskId, TaskStatus};
    use apollia_triggers::{TaskSubmitter, TriggerEngineHandle};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::post;
    use axum::Router;
    use std::future::Future;
    use std::path::PathBuf;
    use std::pin::Pin;
    use std::sync::Arc;
    use tower::ServiceExt;

    #[derive(Clone)]
    struct MockBackend;

    impl ExecutionBackend for MockBackend {
        fn execute(
            &self,
            _task: AIPTask,
        ) -> Pin<Box<dyn Future<Output = Result<AIPResult, String>> + Send>> {
            Box::pin(async {
                Ok(AIPResult {
                    task_id: String::new(),
                    status: TaskStatus::Completed,
                    output: Vec::new(),
                    error: None,
                    artifacts: Vec::new(),
                    input_required_data: None,
                })
            })
        }
    }

    /// Mock minimal du `TaskSubmitter` pour les tests de routes.
    struct MockSubmitter;

    impl TaskSubmitter for MockSubmitter {
        fn submit<'a>(
            &'a self,
            _agent: &'a str,
            _input: AIPInput,
        ) -> Pin<Box<dyn Future<Output = Result<TaskId, String>> + Send + 'a>> {
            Box::pin(async { Ok(TaskId::new_v4()) })
        }

        fn pending_count<'a>(
            &'a self,
            _agent: &'a str,
        ) -> Pin<Box<dyn Future<Output = usize> + Send + 'a>> {
            Box::pin(async { 0 })
        }
    }

    /// Construit un `AppState` minimal pour les tests.
    async fn make_state(with_engine: bool, config_path: Option<PathBuf>) -> AppState<MockBackend> {
        let (event_tx, _) = EventBus::new();
        let registry = AgentRegistry::spawn(event_tx.clone());
        let router: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry.clone(), event_tx.clone(), 64);

        let trigger_engine = if with_engine {
            Some(TriggerEngineHandle::spawn(vec![], MockSubmitter, event_tx.clone(), None).await)
        } else {
            None
        };

        AppState {
            router_handle: router,
            registry_handle: registry,
            event_sender: event_tx,
            agent_loader: Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: None,
            trigger_engine,
            config_path,
            task_repository: None,
            pending_approvals: None,
        }
    }

    fn make_router(state: AppState<MockBackend>) -> Router {
        Router::new()
            .route(
                "/api/v1/triggers/reload",
                post(reload_triggers::<MockBackend>),
            )
            .with_state(state)
    }

    /// AC : 503 si TriggerEngine absent.
    #[tokio::test]
    async fn test_reload_503_when_no_trigger_engine() {
        // GIVEN state sans TriggerEngine
        let state = make_state(false, None).await;
        let router = make_router(state);

        // WHEN POST /api/v1/triggers/reload
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/triggers/reload")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 503
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// AC-5 : 422 si le fichier de config est introuvable — triggers actuels non interrompus.
    #[tokio::test]
    async fn test_reload_422_when_config_file_not_found() {
        // GIVEN TriggerEngine actif mais config_path inexistant
        let bad_path = PathBuf::from("/tmp/apollia-test-nonexistent-73.toml");
        let state = make_state(true, Some(bad_path)).await;
        let router = make_router(state);

        // WHEN POST /api/v1/triggers/reload
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/triggers/reload")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN 422
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
