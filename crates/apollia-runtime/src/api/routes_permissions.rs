//! Permission rules over HTTP:
//! `GET /api/v1/permissions/proposals`,
//! `POST /api/v1/permissions/proposals/{index}/apply`,
//! `DELETE /api/v1/permissions/proposals/{index}`, and
//! `POST /api/v1/permissions/rules`.
//!
//! These had no route: only the desktop running the runtime in its own
//! process could apply a proposed rule. The routes serve the functions of
//! [`crate::permission_rules`] to any client of the daemon, the attached
//! desktop first.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::openapi::ApiErrorBody;
use crate::api::server::AppState;
use crate::coordinator::ExecutionBackend;
use crate::permission_rules::{
    add_global_rule, apply_proposal, dismiss_proposal, list_proposals, PermissionRuleError,
    ProposedRuleView,
};

type ApiError = (StatusCode, Json<ApiErrorBody>);

/// Answer of `POST /api/v1/permissions/proposals/{index}/apply`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct AppliedProposal {
    /// Id of the rule stored in `governance.db`.
    pub rule_id: i64,
}

/// Answer of `DELETE /api/v1/permissions/proposals/{index}`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct DismissedProposal {
    /// The cursor of the proposal removed.
    pub dismissed: usize,
}

/// Request body for `POST /api/v1/permissions/rules`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct AddPermissionRuleBody {
    /// Tool the rule targets.
    pub tool_name: String,
    /// Argument prefix the rule matches; absent matches every call.
    #[serde(default)]
    pub arg_prefix: Option<String>,
    /// `allow` or `deny`.
    pub action: String,
    /// `global`, the only scope this route writes.
    pub scope: String,
}

/// Answer of `POST /api/v1/permissions/rules`.
#[derive(Debug, Deserialize, Serialize, utoipa::ToSchema)]
pub struct AddedPermissionRule {
    /// Id of the rule stored in `governance.db`.
    pub rule_id: i64,
}

/// Build the permissions router.
pub fn permissions_router<B: ExecutionBackend + Clone>() -> Router<AppState<B>> {
    Router::new()
        .route(
            "/api/v1/permissions/proposals",
            get(list_proposed_permission_rules::<B>),
        )
        .route(
            "/api/v1/permissions/proposals/:index/apply",
            post(apply_proposed_permission_rule::<B>),
        )
        .route(
            "/api/v1/permissions/proposals/:index",
            delete(dismiss_proposed_permission_rule::<B>),
        )
        .route("/api/v1/permissions/rules", post(add_permission_rule::<B>))
}

fn refusal(status: StatusCode, error: impl ToString) -> ApiError {
    (
        status,
        Json(ApiErrorBody {
            error: error.to_string(),
        }),
    )
}

fn rule_refusal(error: PermissionRuleError) -> ApiError {
    let status = match error {
        PermissionRuleError::NoProposals | PermissionRuleError::OutOfRange { .. } => {
            StatusCode::NOT_FOUND
        }
        PermissionRuleError::InvalidRule(_) => StatusCode::BAD_REQUEST,
        PermissionRuleError::InvalidProposal(_) => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    refusal(status, error)
}

/// `GET /api/v1/permissions/proposals`: the rules the onboarding agent
/// proposed and the user has not answered yet.
#[utoipa::path(
    get,
    path = "/api/v1/permissions/proposals",
    tag = "governance",
    responses(
        (status = 200, description = "The proposals, empty when none is stored", body = [ProposedRuleView]),
        (status = 500, description = "A namespace database did not read", body = ApiErrorBody),
    )
)]
pub(crate) async fn list_proposed_permission_rules<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Result<Json<Vec<ProposedRuleView>>, ApiError> {
    list_proposals(state.data_dir.clone())
        .await
        .map(Json)
        .map_err(rule_refusal)
}

/// `POST /api/v1/permissions/proposals/{index}/apply`: stores the proposal as
/// a rule and removes it from the list.
#[utoipa::path(
    post,
    path = "/api/v1/permissions/proposals/{index}/apply",
    tag = "governance",
    params(("index" = usize, Path, description = "Cursor of the proposal")),
    responses(
        (status = 200, description = "The rule stored", body = AppliedProposal),
        (status = 404, description = "No proposal at this cursor", body = ApiErrorBody),
        (status = 422, description = "The proposal cannot be a rule", body = ApiErrorBody),
        (status = 500, description = "A database refused", body = ApiErrorBody),
    )
)]
pub(crate) async fn apply_proposed_permission_rule<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(index): Path<usize>,
) -> Result<Json<AppliedProposal>, ApiError> {
    let rule_id = apply_proposal(state.data_dir.clone(), index)
        .await
        .map_err(rule_refusal)?;
    Ok(Json(AppliedProposal { rule_id }))
}

/// `DELETE /api/v1/permissions/proposals/{index}`: removes the proposal
/// without storing a rule.
#[utoipa::path(
    delete,
    path = "/api/v1/permissions/proposals/{index}",
    tag = "governance",
    params(("index" = usize, Path, description = "Cursor of the proposal")),
    responses(
        (status = 200, description = "The proposal is gone", body = DismissedProposal),
        (status = 404, description = "No proposal at this cursor", body = ApiErrorBody),
        (status = 500, description = "The namespace database refused", body = ApiErrorBody),
    )
)]
pub(crate) async fn dismiss_proposed_permission_rule<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Path(index): Path<usize>,
) -> Result<Json<DismissedProposal>, ApiError> {
    dismiss_proposal(state.data_dir.clone(), index)
        .await
        .map_err(rule_refusal)?;
    Ok(Json(DismissedProposal { dismissed: index }))
}

/// `POST /api/v1/permissions/rules`: adds a prefix rule for every agent.
#[utoipa::path(
    post,
    path = "/api/v1/permissions/rules",
    tag = "governance",
    request_body = AddPermissionRuleBody,
    responses(
        (status = 200, description = "The rule stored", body = AddedPermissionRule),
        (status = 400, description = "Empty tool, unknown action, or a scope other than global", body = ApiErrorBody),
        (status = 500, description = "The governance database refused", body = ApiErrorBody),
    )
)]
pub(crate) async fn add_permission_rule<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
    Json(body): Json<AddPermissionRuleBody>,
) -> Result<Json<AddedPermissionRule>, ApiError> {
    let rule_id = add_global_rule(
        state.data_dir.clone(),
        body.tool_name,
        body.arg_prefix,
        body.action,
        body.scope,
    )
    .await
    .map_err(rule_refusal)?;
    Ok(Json(AddedPermissionRule { rule_id }))
}

#[cfg(test)]
mod tests;
