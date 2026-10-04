//! The governance of native tools over HTTP, under `/api/v1/governance`: the
//! tools with their flag and configuration, the permission rules and their
//! revocation, the audit of the permission decisions, and whether governance
//! fails closed.
//!
//! Only the desktop running the runtime in its own process could read or
//! change them. These routes serve the functions of [`crate::tool_governance`]
//! to any client of the daemon, the attached desktop first, against the
//! `governance.db` of the daemon's data directory.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::tool_governance::{
    get_tool_config, list_audit, list_permission_rules, list_tools, revoke_all_rules,
    revoke_permission_rule, set_tool_config, set_tool_enabled, AuditEntryDto, PermissionRuleDto,
    PermissionRuleFilter, ToolGovernanceError, ToolStatusDto,
};

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Request body for `PUT /api/v1/governance/tools/{name}/enabled`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct ToolEnabledBody {
    /// `true` to enable the tool, `false` to disable it.
    pub enabled: bool,
}

/// Request and answer body of `/api/v1/governance/tools/{name}/config`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct ToolConfigBody {
    /// Tool-specific JSON configuration, `null` when none is stored.
    #[schema(value_type = Option<Object>)]
    pub config: Option<serde_json::Value>,
}

/// Answer of `DELETE /api/v1/governance/rules/{id}`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct RevokedRule {
    /// The identifier of the rule removed.
    pub revoked: i64,
}

/// Request body for `POST /api/v1/governance/rules/revoke-all`.
#[derive(Debug, Default, Deserialize, Serialize, utoipa::ToSchema)]
pub struct RevokeAllBody {
    /// Scope to clear, `"agent"` or `"global"`; `global` when absent.
    pub scope: Option<String>,
}

/// Answer of `POST /api/v1/governance/rules/revoke-all`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct RevokedRules {
    /// How many rules were removed.
    pub removed: u32,
}

/// Query of `GET /api/v1/governance/audit`.
#[derive(Debug, Default, Deserialize, Serialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AuditQuery {
    /// Tool name to filter on.
    pub tool_name: Option<String>,
    /// Page size, 100 when absent.
    pub limit: Option<u32>,
    /// Entries to skip.
    pub offset: Option<u32>,
}

/// Answer of `GET /api/v1/governance/state`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct GovernanceStateBody {
    /// Why governance fails closed, `null` while it decides normally. While it
    /// fails closed, every sensitive tool call and every new connection is
    /// refused; reading continues.
    pub fail_closed: Option<crate::governance::FailClosed>,
}

/// Build the tool governance router.
pub fn tool_governance_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route("/api/v1/governance/tools", get(list_governance_tools::<B>))
        .route(
            "/api/v1/governance/tools/:name/enabled",
            put(set_governance_tool_enabled::<B>),
        )
        .route(
            "/api/v1/governance/tools/:name/config",
            get(get_governance_tool_config::<B>).put(set_governance_tool_config::<B>),
        )
        .route("/api/v1/governance/rules", get(list_governance_rules::<B>))
        .route(
            "/api/v1/governance/rules/:id",
            delete(revoke_governance_rule::<B>),
        )
        .route(
            "/api/v1/governance/rules/revoke-all",
            post(revoke_all_governance_rules::<B>),
        )
        .route("/api/v1/governance/audit", get(list_governance_audit::<B>))
        .route("/api/v1/governance/state", get(get_governance_state::<B>))
}

fn refusal(error: ToolGovernanceError) -> ApiError {
    let status = match error {
        ToolGovernanceError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        ToolGovernanceError::EmptyToolName | ToolGovernanceError::UnknownScope(_) => {
            StatusCode::BAD_REQUEST
        }
        ToolGovernanceError::RuleNotFound(_) => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

/// `GET /api/v1/governance/tools`: every native tool with its flag,
/// configuration, credential names and active backend.
#[utoipa::path(
    get,
    path = "/api/v1/governance/tools",
    tag = "governance",
    responses(
        (status = 200, description = "The native tools", body = Vec<ToolStatusDto>),
        (status = 500, description = "The governance database refused the read", body = ApiErrorBody),
        (status = 503, description = "The runtime has no data directory", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_governance_tools<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<ToolStatusDto>>, ApiError> {
    list_tools(state.data_dir.clone())
        .await
        .map(Json)
        .map_err(refusal)
}

/// `PUT /api/v1/governance/tools/{name}/enabled`: enables or disables one tool.
#[utoipa::path(
    put,
    path = "/api/v1/governance/tools/{name}/enabled",
    tag = "governance",
    params(("name" = String, Path, description = "Tool name")),
    request_body = ToolEnabledBody,
    responses(
        (status = 200, description = "The flag as stored", body = ToolEnabledBody),
        (status = 400, description = "Empty tool name", body = ApiErrorBody),
        (status = 500, description = "The governance database refused the write", body = ApiErrorBody),
        (status = 503, description = "The runtime has no data directory", body = ApiErrorBody),
    )
)]
pub(crate) async fn set_governance_tool_enabled<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
    Json(body): Json<ToolEnabledBody>,
) -> Result<Json<ToolEnabledBody>, ApiError> {
    set_tool_enabled(state.data_dir.clone(), name, body.enabled)
        .await
        .map_err(refusal)?;
    Ok(Json(body))
}

/// `GET /api/v1/governance/tools/{name}/config`: the configuration of one tool.
#[utoipa::path(
    get,
    path = "/api/v1/governance/tools/{name}/config",
    tag = "governance",
    params(("name" = String, Path, description = "Tool name")),
    responses(
        (status = 200, description = "The configuration, null when none is stored", body = ToolConfigBody),
        (status = 400, description = "Empty tool name", body = ApiErrorBody),
        (status = 500, description = "The governance database refused the read", body = ApiErrorBody),
        (status = 503, description = "The runtime has no data directory", body = ApiErrorBody),
    )
)]
pub(crate) async fn get_governance_tool_config<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
) -> Result<Json<ToolConfigBody>, ApiError> {
    let config = get_tool_config(state.data_dir.clone(), name)
        .await
        .map_err(refusal)?;
    Ok(Json(ToolConfigBody { config }))
}

/// `PUT /api/v1/governance/tools/{name}/config`: stores the configuration of
/// one tool.
#[utoipa::path(
    put,
    path = "/api/v1/governance/tools/{name}/config",
    tag = "governance",
    params(("name" = String, Path, description = "Tool name")),
    request_body = ToolConfigBody,
    responses(
        (status = 200, description = "The configuration as stored", body = ToolConfigBody),
        (status = 400, description = "Empty tool name", body = ApiErrorBody),
        (status = 500, description = "The governance database refused the write", body = ApiErrorBody),
        (status = 503, description = "The runtime has no data directory", body = ApiErrorBody),
    )
)]
pub(crate) async fn set_governance_tool_config<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(name): Path<String>,
    Json(body): Json<ToolConfigBody>,
) -> Result<Json<ToolConfigBody>, ApiError> {
    let config = body.config.clone().unwrap_or(serde_json::Value::Null);
    set_tool_config(state.data_dir.clone(), name, config)
        .await
        .map_err(refusal)?;
    Ok(Json(body))
}

/// `GET /api/v1/governance/rules`: the `agent` and `global` permission rules.
#[utoipa::path(
    get,
    path = "/api/v1/governance/rules",
    tag = "governance",
    params(PermissionRuleFilter),
    responses(
        (status = 200, description = "The matching rules", body = Vec<PermissionRuleDto>),
        (status = 400, description = "Unknown scope", body = ApiErrorBody),
        (status = 500, description = "The governance database refused the read", body = ApiErrorBody),
        (status = 503, description = "The runtime has no data directory", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_governance_rules<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(filter): Query<PermissionRuleFilter>,
) -> Result<Json<Vec<PermissionRuleDto>>, ApiError> {
    list_permission_rules(state.data_dir.clone(), filter)
        .await
        .map(Json)
        .map_err(refusal)
}

/// `DELETE /api/v1/governance/rules/{id}`: revokes one rule.
#[utoipa::path(
    delete,
    path = "/api/v1/governance/rules/{id}",
    tag = "governance",
    params(("id" = i64, Path, description = "Rule identifier")),
    responses(
        (status = 200, description = "The rule is gone", body = RevokedRule),
        (status = 404, description = "No rule has this identifier", body = ApiErrorBody),
        (status = 500, description = "The governance database refused the deletion", body = ApiErrorBody),
        (status = 503, description = "The runtime has no data directory", body = ApiErrorBody),
    )
)]
pub(crate) async fn revoke_governance_rule<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(id): Path<i64>,
) -> Result<Json<RevokedRule>, ApiError> {
    revoke_permission_rule(state.data_dir.clone(), id)
        .await
        .map_err(refusal)?;
    Ok(Json(RevokedRule { revoked: id }))
}

/// `POST /api/v1/governance/rules/revoke-all`: revokes every rule of one
/// scope, `global` when none is named.
#[utoipa::path(
    post,
    path = "/api/v1/governance/rules/revoke-all",
    tag = "governance",
    request_body = RevokeAllBody,
    responses(
        (status = 200, description = "How many rules were removed", body = RevokedRules),
        (status = 400, description = "Unknown scope", body = ApiErrorBody),
        (status = 500, description = "The governance database refused the deletion", body = ApiErrorBody),
        (status = 503, description = "The runtime has no data directory", body = ApiErrorBody),
    )
)]
pub(crate) async fn revoke_all_governance_rules<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<RevokeAllBody>,
) -> Result<Json<RevokedRules>, ApiError> {
    let removed = revoke_all_rules(state.data_dir.clone(), body.scope)
        .await
        .map_err(refusal)?;
    Ok(Json(RevokedRules { removed }))
}

/// `GET /api/v1/governance/audit`: the permission decisions, most recent first.
#[utoipa::path(
    get,
    path = "/api/v1/governance/audit",
    tag = "governance",
    params(AuditQuery),
    responses(
        (status = 200, description = "The audit entries", body = Vec<AuditEntryDto>),
        (status = 500, description = "The governance database refused the read", body = ApiErrorBody),
        (status = 503, description = "The runtime has no data directory", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_governance_audit<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEntryDto>>, ApiError> {
    list_audit(
        state.data_dir.clone(),
        query.tool_name,
        query.limit,
        query.offset,
    )
    .await
    .map(Json)
    .map_err(refusal)
}

/// `GET /api/v1/governance/state`: whether governance fails closed, and why.
#[utoipa::path(
    get,
    path = "/api/v1/governance/state",
    tag = "governance",
    responses(
        (status = 200, description = "The state of governance", body = GovernanceStateBody),
    )
)]
pub(crate) async fn get_governance_state<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<GovernanceStateBody> {
    // The sources are read again first: a database whose permissions were
    // repaired keeps its modification time, so only an explicit read lifts
    // the failure, which is what retrying from the interface asks for.
    if let Some(governance) = &state.governance {
        governance.refresh().await;
    }
    let gate = crate::governance::tool_gate(state.governance.as_ref(), &state.data_dir, &[]);
    Json(GovernanceStateBody {
        fail_closed: gate.fail_closed,
    })
}

#[cfg(test)]
mod tests;
