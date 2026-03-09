//! Route `POST /api/v1/triggers/reload` — hot reload depuis `apollia.toml`.
//!
//! Relit la section `[[triggers]]` depuis le fichier de configuration connu
//! (stocké dans [`AppState::config_path`]), valide les nouvelles définitions,
//! et appelle [`TriggerEngineHandle::reload`] si tout est valide.
//!
//! **Codes de retour :**
//! - `503` — `TriggerEngine` non disponible ou `config_path` inconnu dans [`AppState`].
//! - `422` — TOML malformé ou trigger avec configuration invalide ; les triggers
//!   actuels continuent de fonctionner sans interruption (AC-5).
//! - `200` — Rechargement réussi avec `{ "reloaded": N }`.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

use apollia_triggers::{parse_triggers_from_toml_str, TriggerTomlError};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

// ─── Response types ────────────────────────────────────────────────────────

/// Corps de réponse en cas de succès.
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
