//! Assembly of the axum router: the two built-in handlers and every mounted
//! route module.
//!
//! `build_router` is the single place that knows the full URL surface; each
//! domain module contributes its handlers, and the health and shutdown
//! endpoints live here because they belong to the server itself.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use tracing::info;

use crate::api::server::{AppState, PRODUCT_MARKER};
use crate::coordinator::{DynBackend, ExecutionBackend};

/// Response body for the health endpoint.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct HealthResponse {
    status: String,
    /// Always [`PRODUCT_MARKER`]: what lets a caller that finds the port
    /// taken tell an Apollia runtime from any other listener.
    product: &'static str,
    /// The version of the runtime answering, as its crate declares it.
    version: &'static str,
}

/// Response body for the shutdown endpoint.
#[derive(Serialize, utoipa::ToSchema)]
pub(crate) struct ShutdownResponse {
    status: String,
}

/// Handler for `GET /api/v1/health`.
#[utoipa::path(
    get,
    path = "/api/v1/health",
    tag = "health",
    // Open on the TCP listener: a process that finds the port taken has no
    // token, and the body reveals the product marker and the version only.
    security(()),
    responses((status = 200, description = "Runtime is up", body = HealthResponse))
)]
pub(crate) async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
        product: PRODUCT_MARKER,
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Handler for `POST /api/v1/shutdown`.
///
/// Emits [`RuntimeEvent::ShutdownRequested`] on the EventBus. The caller
/// (typically `apollia-os start`) listens for this event to trigger
/// graceful shutdown.
#[utoipa::path(
    post,
    path = "/api/v1/shutdown",
    tag = "health",
    responses((status = 200, description = "Shutdown initiated", body = ShutdownResponse))
)]
pub(crate) async fn shutdown_handler<B: ExecutionBackend + Clone>(
    State(state): State<AppState<B>>,
) -> Json<ShutdownResponse> {
    info!("api.shutdown.requested");
    let _ = state
        .event_sender
        .send(apollia_core::RuntimeEvent::ShutdownRequested);
    Json(ShutdownResponse {
        status: "shutting_down".into(),
    })
}

/// Build the axum Router with all routes and shared state.
pub(crate) fn build_router<B: ExecutionBackend + Clone + From<DynBackend>>(
    state: AppState<B>,
) -> Router {
    use crate::api::routes_a2a::{
        delegate, get_task_sidechains, invoke_by_skill, list_a2a_agents, list_a2a_skills,
    };
    use crate::api::routes_agents::installed::{install_agent, list_installed_agents};
    use crate::api::routes_agents::{get_agent, list_agents, start_agent, stop_agent};
    use crate::api::routes_app_lifecycle::app_lifecycle_router;
    use crate::api::routes_approvals::{list_pending_approvals, list_resolved_approvals};
    use crate::api::routes_artifacts::artifacts_router;
    use crate::api::routes_audit::{
        get_audit_anchor, get_audit_stats, list_audit, list_audit_journal, post_replay_run,
        show_audit_run, verify_audit_journal, verify_audit_run,
    };
    use crate::api::routes_chat::conversations::{conversations_router, update_session};
    use crate::api::routes_chat::user_inputs::{
        answer_user_input, list_pending_user_inputs, reject_user_input,
    };
    use crate::api::routes_chat::{approval_history, turns};
    use crate::api::routes_chat::{
        authorize_tool as chat_authorize_tool, close_session, create_session,
        fork_session as chat_fork_session, get_session as chat_get_session,
        get_session_todo as chat_get_session_todo, list_recent_sessions, list_session_children,
        list_sessions, pause::pause_session, plan, resume_session as chat_resume_session,
        send_message, stream_session,
    };
    use crate::api::routes_chat_assists::chat_assists_router;
    use crate::api::routes_companion::companion_router;
    use crate::api::routes_connections::connections_router;
    use crate::api::routes_connectors::connectors_router;
    use crate::api::routes_drive_folders::drive_folders_router;
    use crate::api::routes_engines::engines_router;
    use crate::api::routes_hooks::list_hooks;
    use crate::api::routes_imap::imap_router;
    use crate::api::routes_llm::llm_routes;
    use crate::api::routes_local_ai::local_ai_router;
    use crate::api::routes_mcp::mcp_router;
    use crate::api::routes_mcp_registry::mcp_registry_router;
    use crate::api::routes_mcp_secrets::mcp_secrets_router;
    use crate::api::routes_memory::memory_router;
    use crate::api::routes_memory_maintenance::memory_maintenance_router;
    use crate::api::routes_messages::{inject_agent_message, list_agent_messages, stream_mailbox};
    use crate::api::routes_model_hub::model_hub_routes;
    use crate::api::routes_notifications::logs::notification_logs;
    use crate::api::routes_notifications::probe::test_channels;
    use crate::api::routes_notifications::{
        create_channel, delete_channel, get_events, list_channels, set_events, update_channel,
    };
    use crate::api::routes_oauth_clients::oauth_clients_router;
    use crate::api::routes_onboarding::onboarding_router;
    use crate::api::routes_permissions::permissions_router;
    use crate::api::routes_plan_cache::{clear_plan_cache, get_plan_cache_stats};
    use crate::api::routes_profile::profile_router;
    use crate::api::routes_resilience::resilience_router;
    use crate::api::routes_review::post_review;
    use crate::api::routes_sse::stream_task;
    use crate::api::routes_stt::settings::{get_stt_config, reload_stt_engine, update_stt_config};
    use crate::api::routes_stt::{
        delete_transcription, list_models, list_transcriptions, stt_status, transcribe_audio,
    };
    use crate::api::routes_tasks::summaries::list_task_summaries;
    use crate::api::routes_tasks::{
        cancel_task, get_task, list_tasks, resume_task, submit_plan_decision, submit_task,
    };
    use crate::api::routes_timeline::get_task_timeline;
    use crate::api::routes_tool_credentials::tool_credentials_router;
    use crate::api::routes_tool_governance::tool_governance_router;
    use crate::api::routes_tools::{describe_tool, list_tools};
    use crate::api::routes_trace::get_task_trace;
    use crate::api::routes_triggers::{
        create_trigger, delete_trigger, disable_trigger, enable_trigger, fire_trigger,
        get_trigger_by_id, get_trigger_logs, list_triggers, reload_triggers, update_trigger,
    };
    use crate::api::routes_webhooks::handle_webhook;

    Router::new()
        .route("/api/v1/health", get(health_handler))
        .route(
            "/api/v1/openapi.json",
            get(crate::api::openapi::openapi_json),
        )
        .route("/api/v1/shutdown", post(shutdown_handler::<B>))
        .route(
            "/api/v1/events",
            get(crate::api::routes_events::stream_events::<B>),
        )
        .route("/api/v1/tasks", get(list_tasks::<B>).post(submit_task::<B>))
        // Registered before /:id so the static segment is never read as an id.
        .route("/api/v1/tasks/summaries", get(list_task_summaries::<B>))
        .route(
            "/api/v1/tasks/:id",
            get(get_task::<B>).delete(cancel_task::<B>),
        )
        .route("/api/v1/tasks/:id/stream", get(stream_task::<B>))
        .route("/api/v1/tasks/:id/resume", post(resume_task::<B>))
        .route(
            "/api/v1/tasks/:id/plan-decision",
            post(submit_plan_decision::<B>),
        )
        .route("/api/v1/tasks/:id/review", post(post_review::<B>))
        // Timeline route (legacy, deprecation candidate)
        .route("/api/v1/tasks/:id/timeline", get(get_task_timeline::<B>))
        // Event-sourced trace
        .route("/api/v1/tasks/:id/trace", get(get_task_trace::<B>))
        // Tool routes
        .route("/api/v1/tools", get(list_tools::<B>))
        .route("/api/v1/tools/:name", get(describe_tool::<B>))
        // Audit trail routes
        .route("/api/v1/audit", get(list_audit::<B>))
        .route("/api/v1/audit/stats", get(get_audit_stats::<B>))
        .route("/api/v1/audit/verify", get(verify_audit_journal::<B>))
        .route("/api/v1/audit/verify/:run_id", get(verify_audit_run::<B>))
        .route("/api/v1/audit/anchor", get(get_audit_anchor::<B>))
        .route("/api/v1/audit/journal", get(list_audit_journal::<B>))
        .route("/api/v1/audit/journal/:run_id", get(show_audit_run::<B>))
        .route("/api/v1/audit/replay/:run_id", post(post_replay_run::<B>))
        // Lifecycle hooks route
        .route("/api/v1/hooks", get(list_hooks::<B>))
        .route(
            "/api/v1/agents",
            get(list_agents::<B>).post(start_agent::<B>),
        )
        .route(
            "/api/v1/agents/installed",
            get(list_installed_agents::<B>).post(install_agent::<B>),
        )
        .route(
            "/api/v1/agents/:id",
            get(get_agent::<B>).delete(stop_agent::<B>),
        )
        .route(
            "/api/v1/agents/:name/messages",
            get(list_agent_messages::<B>).post(inject_agent_message::<B>),
        )
        .route("/api/v1/mailbox/stream", get(stream_mailbox::<B>))
        // A2A routing routes
        .route("/api/v1/a2a/agents", get(list_a2a_agents::<B>))
        .route("/api/v1/a2a/delegate", post(delegate::<B>))
        .route("/api/v1/a2a/skills", get(list_a2a_skills::<B>))
        .route("/api/v1/a2a/invoke", post(invoke_by_skill::<B>))
        // Sidechain traceability
        .route(
            "/api/v1/tasks/:id/sidechains",
            get(get_task_sidechains::<B>),
        )
        // Plan cache routes
        .route("/api/v1/plan-cache/stats", get(get_plan_cache_stats::<B>))
        .route("/api/v1/plan-cache/clear", post(clear_plan_cache::<B>))
        .route("/webhooks/:id", post(handle_webhook::<B>))
        // HITL approval routes
        .route(
            "/api/v1/approvals/pending",
            get(list_pending_approvals::<B>),
        )
        .route(
            "/api/v1/approvals/resolved",
            get(list_resolved_approvals::<B>),
        )
        // Trigger routes (reload + status + CRUD)
        .route(
            "/api/v1/triggers",
            get(list_triggers::<B>).post(create_trigger::<B>),
        )
        .route("/api/v1/triggers/reload", post(reload_triggers::<B>))
        .route(
            "/api/v1/triggers/:id",
            get(get_trigger_by_id::<B>)
                .put(update_trigger::<B>)
                .delete(delete_trigger::<B>),
        )
        .route("/api/v1/triggers/:id/fire", post(fire_trigger::<B>))
        .route("/api/v1/triggers/:id/enable", post(enable_trigger::<B>))
        .route("/api/v1/triggers/:id/disable", post(disable_trigger::<B>))
        .route("/api/v1/triggers/:id/logs", get(get_trigger_logs::<B>))
        // Notification routes (CRUD)
        .route(
            "/api/v1/notifications/channels",
            get(list_channels::<B>).post(create_channel::<B>),
        )
        .route(
            "/api/v1/notifications/channels/:id",
            axum::routing::put(update_channel::<B>).delete(delete_channel::<B>),
        )
        .route(
            "/api/v1/notifications/events",
            get(get_events::<B>).put(set_events::<B>),
        )
        .route("/api/v1/notifications/test", post(test_channels::<B>))
        .route("/api/v1/notifications/logs", get(notification_logs::<B>))
        .merge(llm_routes::<B>())
        .merge(crate::api::routes_agent_definitions::agent_definition_routes::<B>())
        .merge(model_hub_routes::<B>())
        .merge(engines_router::<B>())
        .merge(resilience_router::<B>())
        // Chat session routes
        .route(
            "/api/v1/sessions",
            get(list_sessions::<B>).post(create_session::<B>),
        )
        // /recent must be registered before /:id to avoid the path param capturing "recent"
        .route("/api/v1/sessions/recent", get(list_recent_sessions::<B>))
        .route(
            "/api/v1/sessions/approval-history",
            get(approval_history::list_approval_history::<B>),
        )
        .route(
            "/api/v1/sessions/:id",
            get(chat_get_session::<B>)
                .patch(update_session::<B>)
                .delete(close_session::<B>),
        )
        .route("/api/v1/sessions/:id/messages", post(send_message::<B>))
        .route(
            "/api/v1/sessions/:id/messages/:message_id/regenerate",
            post(turns::regenerate_response::<B>),
        )
        .route(
            "/api/v1/sessions/:id/messages/:message_id/resend",
            post(turns::resend_message::<B>),
        )
        .route(
            "/api/v1/sessions/:id/authorize",
            post(chat_authorize_tool::<B>),
        )
        .route("/api/v1/sessions/:id/stream", get(stream_session::<B>))
        .route("/api/v1/sessions/:id/pause", post(pause_session::<B>))
        .route(
            "/api/v1/sessions/:id/metrics",
            get(crate::api::routes_chat::session_reads::get_session_metrics::<B>),
        )
        .route(
            "/api/v1/sessions/:id/workspace",
            get(crate::api::routes_chat::session_reads::get_session_workspace::<B>),
        )
        .route(
            "/api/v1/sessions/plan-mode-default",
            get(plan::get_plan_mode_default::<B>),
        )
        .route(
            "/api/v1/sessions/:id/plan-mode",
            axum::routing::put(plan::set_plan_mode::<B>),
        )
        .route("/api/v1/sessions/:id/plan", get(plan::get_session_plan::<B>))
        .route(
            "/api/v1/sessions/:id/plan/mutations",
            get(plan::list_plan_mutations::<B>),
        )
        .route(
            "/api/v1/sessions/:id/plan/approve",
            post(plan::approve_plan::<B>),
        )
        .route(
            "/api/v1/sessions/:id/plan/reject",
            post(plan::reject_plan::<B>),
        )
        .route("/api/v1/sessions/:id/todo", get(chat_get_session_todo::<B>))
        .route(
            "/api/v1/sessions/:id/resume",
            post(chat_resume_session::<B>),
        )
        .route("/api/v1/sessions/:id/fork", post(chat_fork_session::<B>))
        .route(
            "/api/v1/sessions/:id/children",
            get(list_session_children::<B>),
        )
        // Questions an agent asked the user
        .route("/api/v1/user-inputs", get(list_pending_user_inputs::<B>))
        .route(
            "/api/v1/user-inputs/:id/answer",
            post(answer_user_input::<B>),
        )
        .route(
            "/api/v1/user-inputs/:id/reject",
            post(reject_user_input::<B>),
        )
        // STT routes
        .route("/api/v1/stt/status", get(stt_status::<B>))
        .route("/api/v1/stt/transcribe", post(transcribe_audio::<B>))
        .route("/api/v1/stt/transcriptions", get(list_transcriptions::<B>))
        .route(
            "/api/v1/stt/transcriptions/:id",
            axum::routing::delete(delete_transcription::<B>),
        )
        .route("/api/v1/stt/models", get(list_models::<B>))
        .route(
            "/api/v1/stt/config",
            get(get_stt_config::<B>).put(update_stt_config::<B>),
        )
        .route("/api/v1/stt/reload", post(reload_stt_engine::<B>))
        // MCP routes
        .merge(mcp_router::<B>())
        .merge(mcp_registry_router::<B>())
        .merge(mcp_secrets_router::<B>())
        // Native connector consent routes
        .merge(connections_router::<B>())
        .merge(profile_router::<B>())
        .merge(companion_router::<B>())
        .merge(local_ai_router::<B>())
        .merge(tool_credentials_router::<B>())
        .merge(onboarding_router::<B>())
        .merge(artifacts_router::<B>())
        .merge(memory_maintenance_router::<B>())
        .merge(app_lifecycle_router::<B>())
        .merge(conversations_router::<B>())
        .merge(memory_router::<B>())
        .merge(permissions_router::<B>())
        .merge(tool_governance_router::<B>())
        .merge(oauth_clients_router::<B>())
        .merge(crate::api::routes_observability::observability_router::<B>())
        .merge(chat_assists_router::<B>())
        .merge(connectors_router::<B>())
        .merge(drive_folders_router::<B>())
        .merge(imap_router::<B>())
        .merge(crate::api::routes_batches::batch_routes::<B>())
        .merge(crate::api::routes_workflows::workflow_routes::<B>())
        .merge(crate::api::routes_mcp_bridge::mcp_bridge_routes::<B>())
        .merge(crate::api::routes_packages::package_routes::<B>())
        .merge(crate::api::routes_agent_packages::agent_package_routes::<B>())
        .with_state(state)
}
