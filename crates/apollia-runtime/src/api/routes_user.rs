//! REST routes for user profile and memory management.
//!
//! Endpoints:
//! - `GET    /api/v1/user/profile`       — aggregated user profile
//! - `PUT    /api/v1/user/profile`       — upsert profile fields
//! - `GET    /api/v1/user/memory`        — list memory entries (optional `category` + `limit`)
//! - `DELETE /api/v1/user/memory/:key`   — forget a single memory entry

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use apollia_core::user::{
    UpdateProfileRequest, UserMemoryEntryResponse, UserMemoryResponse, UserProfile,
};
use apollia_memory::user_memory::{UserMemoryCategory, UserMemorySource};

use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;

/// Default maximum number of entries returned per category.
const DEFAULT_RECALL_LIMIT: usize = 100;

/// Standard error response body (same shape as `routes_tasks::ErrorResponse`).
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Human-readable error description.
    pub error: String,
}

/// Query parameters for `GET /api/v1/user/memory`.
#[derive(Debug, Deserialize)]
pub struct MemoryQueryParams {
    /// Filter by category (`preferences`, `habits`, `context`).
    pub category: Option<String>,
    /// Maximum number of entries to return (default: 100).
    pub limit: Option<usize>,
}

// ─── Handlers ───────────────────────────────────────────────────────────

/// `GET /api/v1/user/profile` — returns the aggregated user profile.
///
/// Reads all entries from the three categories (preferences, habits, context)
/// and assembles them into a [`UserProfile`].
pub async fn get_profile<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<UserProfile>, (StatusCode, Json<ErrorResponse>)> {
    let repo_arc = state.user_memory.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "user memory not configured".into(),
            }),
        )
    })?;
    let repo = repo_arc
        .lock()
        .map_err(|e| storage_error(format!("mutex poisoned: {e}")))?;

    let preferences = repo
        .recall(UserMemoryCategory::Preferences, DEFAULT_RECALL_LIMIT)
        .map_err(|e| storage_error(e.to_string()))?;
    let habits = repo
        .recall(UserMemoryCategory::Habits, DEFAULT_RECALL_LIMIT)
        .map_err(|e| storage_error(e.to_string()))?;
    let context = repo
        .recall(UserMemoryCategory::Context, DEFAULT_RECALL_LIMIT)
        .map_err(|e| storage_error(e.to_string()))?;

    let name = preferences
        .iter()
        .find(|e| e.key == "name")
        .map(|e| e.value.clone());

    let profile = UserProfile {
        name,
        preferences: entries_to_map(&preferences),
        habits: entries_to_map(&habits),
        context: entries_to_map(&context),
    };

    Ok(Json(profile))
}

/// `PUT /api/v1/user/profile` — upserts profile fields.
///
/// Each provided map is merged into the corresponding category (upsert semantics).
/// Missing fields in the request body are left untouched.
pub async fn update_profile<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(req): Json<UpdateProfileRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let repo_arc = state.user_memory.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "user memory not configured".into(),
            }),
        )
    })?;
    let repo = repo_arc
        .lock()
        .map_err(|e| storage_error(format!("mutex poisoned: {e}")))?;

    if let Some(name) = &req.name {
        repo.store(
            UserMemoryCategory::Preferences,
            "name",
            name,
            UserMemorySource::UserExplicit,
        )
        .map_err(|e| storage_error(e.to_string()))?;
    }

    if let Some(prefs) = &req.preferences {
        upsert_map(&repo, UserMemoryCategory::Preferences, prefs)?;
    }
    if let Some(habits) = &req.habits {
        upsert_map(&repo, UserMemoryCategory::Habits, habits)?;
    }
    if let Some(ctx) = &req.context {
        upsert_map(&repo, UserMemoryCategory::Context, ctx)?;
    }

    Ok(StatusCode::OK)
}

/// `GET /api/v1/user/memory` — lists memory entries with optional filters.
///
/// Query parameters:
/// - `category`: filter by category name (returns 422 if invalid).
/// - `limit`: max entries (default: 100).
pub async fn get_memory<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(params): Query<MemoryQueryParams>,
) -> Result<Json<UserMemoryResponse>, (StatusCode, Json<ErrorResponse>)> {
    let repo_arc = state.user_memory.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "user memory not configured".into(),
            }),
        )
    })?;
    let repo = repo_arc
        .lock()
        .map_err(|e| storage_error(format!("mutex poisoned: {e}")))?;

    let limit = params.limit.unwrap_or(DEFAULT_RECALL_LIMIT);

    let categories = match &params.category {
        Some(cat_str) => {
            let cat = parse_category(cat_str)?;
            vec![cat]
        }
        None => vec![
            UserMemoryCategory::Preferences,
            UserMemoryCategory::Habits,
            UserMemoryCategory::Context,
        ],
    };

    let mut entries = Vec::new();
    for cat in categories {
        let recalled = repo
            .recall(cat, limit)
            .map_err(|e| storage_error(e.to_string()))?;

        for entry in recalled {
            entries.push(UserMemoryEntryResponse {
                key: entry.key,
                value: entry.value,
                source: entry.source.to_string(),
                updated_at: entry.updated_at,
            });
        }
    }

    Ok(Json(UserMemoryResponse { entries }))
}

/// `DELETE /api/v1/user/memory/:key` — forgets a single memory entry.
///
/// Scans all categories for the key and removes it.
/// Returns 204 on success, 404 if not found.
pub async fn forget_memory<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(key): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let repo_arc = state.user_memory.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "user memory not configured".into(),
            }),
        )
    })?;
    let repo = repo_arc
        .lock()
        .map_err(|e| storage_error(format!("mutex poisoned: {e}")))?;

    repo.forget(&key).map_err(|e| {
        use apollia_memory::user_memory::UserMemoryError;
        match e {
            UserMemoryError::NotFound(_) => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("memory entry not found: {key}"),
                }),
            ),
            _ => storage_error(e.to_string()),
        }
    })?;

    Ok(StatusCode::NO_CONTENT)
}

// ─── Helpers ────────────────────────────────────────────────────────────

/// Converts a list of [`UserMemoryEntry`] into a `HashMap<String, String>`.
fn entries_to_map(
    entries: &[apollia_memory::user_memory::UserMemoryEntry],
) -> std::collections::HashMap<String, String> {
    entries
        .iter()
        .map(|e| (e.key.clone(), e.value.clone()))
        .collect()
}

/// Upserts all entries from a map into the given category.
fn upsert_map(
    repo: &apollia_memory::user_memory::UserMemoryRepository,
    category: UserMemoryCategory,
    map: &std::collections::HashMap<String, String>,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    for (key, value) in map {
        repo.store(category, key, value, UserMemorySource::UserExplicit)
            .map_err(|e| storage_error(e.to_string()))?;
    }
    Ok(())
}

/// Parses a category string into a [`UserMemoryCategory`].
fn parse_category(
    s: &str,
) -> Result<UserMemoryCategory, (StatusCode, Json<ErrorResponse>)> {
    match s {
        "preferences" => Ok(UserMemoryCategory::Preferences),
        "habits" => Ok(UserMemoryCategory::Habits),
        "context" => Ok(UserMemoryCategory::Context),
        _ => Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: format!("invalid category: {s} (expected: preferences, habits, context)"),
            }),
        )),
    }
}

/// Builds a 500 Internal Server Error response for storage failures.
fn storage_error(msg: String) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: format!("storage error: {msg}"),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::server::AppState;
    use crate::coordinator::{DynBackend, ExecutionBackend};
    use crate::eventbus::EventBus;
    use crate::registry::AgentRegistry;
    use crate::router::TaskRouterHandle;
    use apollia_core::{AIPResult, AIPTask, TaskStatus};
    use apollia_memory::user_memory::UserMemoryRepository;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use tower::ServiceExt;

    #[derive(Clone)]
    struct MockBackend;

    impl From<DynBackend> for MockBackend {
        fn from(_: DynBackend) -> Self {
            MockBackend
        }
    }

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

    fn test_app_state_with_memory(
        repo: Arc<std::sync::Mutex<UserMemoryRepository>>,
    ) -> AppState<MockBackend> {
        let (event_tx, _event_rx) = EventBus::new();
        let registry_handle = AgentRegistry::spawn(event_tx.clone());
        let router_handle: TaskRouterHandle<MockBackend> =
            TaskRouterHandle::spawn(registry_handle.clone(), event_tx.clone(), 64);
        AppState {
            router_handle,
            registry_handle,
            event_sender: event_tx,
            agent_loader: Arc::new(crate::api::routes_agents::StubAgentLoader),
            backend: MockBackend,
            llm_router: None,
            trigger_engine: None,
            config_path: None,
            task_repository: None,
            pending_approvals: None,
            notification_config: None,
            pipeline_engine: None,
            backend_factory: None,
            tool_registry_handle: None,
            audit_trail: None,
            obs_config: apollia_core::ObservabilityConfig::default(),
            llm_call_repository: None,
            trigger_def_repo: None,
            pipeline_def_repo: None,
            notification_repo: None,
            notification_engine_handle: None,
            chat_manager: None,
            plan_cache: None,
            mailbox_handle: None,
            user_memory: Some(repo),
        }
    }

    fn temp_repo() -> Arc<std::sync::Mutex<UserMemoryRepository>> {
        let dir =
            std::env::temp_dir().join(format!("apollia_user_api_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("user_memory.db");
        Arc::new(std::sync::Mutex::new(UserMemoryRepository::new(&path).unwrap()))
    }

    fn build_test_router(state: AppState<MockBackend>) -> axum::Router {
        crate::api::server::APIServer::build_router_for_test(state)
    }

    #[tokio::test]
    async fn test_get_profile_returns_aggregated_data() {
        // GIVEN
        let repo = temp_repo();
        {
            let r = repo.lock().unwrap();
            r.store(
                UserMemoryCategory::Preferences,
                "language",
                "francais",
                UserMemorySource::UserExplicit,
            )
            .unwrap();
            r.store(
                UserMemoryCategory::Habits,
                "working_hours",
                "9h-18h",
                UserMemorySource::AgentObservation,
            )
            .unwrap();
            r.store(
                UserMemoryCategory::Context,
                "project",
                "apollia-os",
                UserMemorySource::Onboarding,
            )
            .unwrap();
        }

        let state = test_app_state_with_memory(repo);
        let router = build_test_router(state);

        // WHEN
        let req = Request::builder()
            .uri("/api/v1/user/profile")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 4096)
            .await
            .unwrap();
        let profile: UserProfile = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            profile.preferences.get("language").map(String::as_str),
            Some("francais")
        );
        assert_eq!(
            profile.habits.get("working_hours").map(String::as_str),
            Some("9h-18h")
        );
        assert_eq!(
            profile.context.get("project").map(String::as_str),
            Some("apollia-os")
        );
    }

    #[tokio::test]
    async fn test_update_profile_persists_and_returns_ok() {
        // GIVEN
        let repo = temp_repo();
        let state = test_app_state_with_memory(repo);
        let router = build_test_router(state);

        // WHEN — PUT profile
        let body = serde_json::json!({
            "preferences": { "language": "francais" }
        });
        let req = Request::builder()
            .method("PUT")
            .uri("/api/v1/user/profile")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let resp = router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // THEN — GET profile reflects the change
        let req = Request::builder()
            .uri("/api/v1/user/profile")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 4096)
            .await
            .unwrap();
        let profile: UserProfile = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            profile.preferences.get("language").map(String::as_str),
            Some("francais")
        );
    }

    #[tokio::test]
    async fn test_get_memory_filters_by_category() {
        // GIVEN
        let repo = temp_repo();
        {
            let r = repo.lock().unwrap();
            r.store(
                UserMemoryCategory::Preferences,
                "language",
                "francais",
                UserMemorySource::UserExplicit,
            )
            .unwrap();
            r.store(
                UserMemoryCategory::Habits,
                "working_hours",
                "9h-18h",
                UserMemorySource::AgentObservation,
            )
            .unwrap();
        }

        let state = test_app_state_with_memory(repo);
        let router = build_test_router(state);

        // WHEN — filter by preferences
        let req = Request::builder()
            .uri("/api/v1/user/memory?category=preferences")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();

        // THEN
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 4096)
            .await
            .unwrap();
        let mem_resp: UserMemoryResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(mem_resp.entries.len(), 1);
        assert_eq!(mem_resp.entries[0].key, "language");
    }

    #[tokio::test]
    async fn test_forget_memory_deletes_and_returns_204() {
        // GIVEN
        let repo = temp_repo();
        {
            let r = repo.lock().unwrap();
            r.store(
                UserMemoryCategory::Preferences,
                "language",
                "francais",
                UserMemorySource::UserExplicit,
            )
            .unwrap();
        }

        let state = test_app_state_with_memory(repo);
        let router = build_test_router(state);

        // WHEN — DELETE the entry
        let req = Request::builder()
            .method("DELETE")
            .uri("/api/v1/user/memory/language")
            .body(Body::empty())
            .unwrap();
        let resp = router.clone().oneshot(req).await.unwrap();

        // THEN — 204
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // AND — GET memory shows nothing in preferences
        let req = Request::builder()
            .uri("/api/v1/user/memory?category=preferences")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), 4096)
            .await
            .unwrap();
        let mem_resp: UserMemoryResponse = serde_json::from_slice(&body).unwrap();
        assert!(mem_resp.entries.is_empty());
    }
}
