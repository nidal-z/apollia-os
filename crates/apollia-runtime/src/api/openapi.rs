//! OpenAPI document assembly and the `GET /api/v1/openapi.json` handler.
//!
//! The spec is derived from the code: `#[utoipa::path]` attributes on the
//! handlers and `#[derive(ToSchema)]` on the request/response types. It cannot
//! drift from the wire contract because it is generated from the same types the
//! handlers serialize.
//!
//! `utoipa` lives in this crate only; cross-crate body types are represented
//! with `#[schema(value_type = Object)]` at the field so the dependency never
//! spreads to `apollia-core` and the other crates.
//!
//! Stability: `/api/v1` is a versioned, stable contract. A breaking change ships
//! as `/api/v2`; `v1` is never mutated in an incompatible way.

use axum::Json;
use serde::Serialize;
use utoipa::{OpenApi, ToSchema};

/// Canonical error body for every non-2xx response in the spec.
///
/// Matches the wire shape of the per-module `ErrorResponse { error }` used
/// across the route handlers.
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiErrorBody {
    /// Human-readable error description.
    pub error: String,
}

/// The generated OpenAPI document for the `/api/v1` driving contract.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Apollia OS Runtime API",
        version = "1.0.0",
        description = "Host driving contract for the Apollia OS sovereign agent runtime. Stable under /api/v1; breaking changes ship under /api/v2."
    ),
    tags(
        (name = "health", description = "Liveness and lifecycle"),
        (name = "tasks", description = "Submit and drive autonomous agent tasks"),
        (name = "chat", description = "Conversational chat sessions"),
        (name = "agents", description = "Agent inventory and lifecycle"),
        (name = "a2a", description = "Agent-to-agent skill invocation"),
        (name = "audit", description = "Signed audit trail and verification"),
        (name = "governance", description = "Approvals, hooks, plan cache"),
        (name = "tools", description = "Native tool catalogue"),
        (name = "triggers", description = "Scheduled and event triggers"),
        (name = "notifications", description = "Notification channels and events"),
        (name = "stt", description = "Local speech to text"),
        (name = "llm", description = "LLM backends, routing, and cost"),
        (name = "model_hub", description = "Hardware profiling and model registry"),
        (name = "resilience", description = "Circuit breakers"),
        (name = "mcp", description = "MCP servers and resources"),
        (name = "webhooks", description = "Inbound webhooks")
    ),
    paths(
        crate::api::server::health_handler,
        crate::api::server::shutdown_handler,
        crate::api::routes_tasks::list_tasks,
        crate::api::routes_tasks::submit_task,
        crate::api::routes_tasks::get_task,
        crate::api::routes_tasks::cancel_task,
        crate::api::routes_tasks::resume_task,
        crate::api::routes_tasks::submit_plan_decision,
        crate::api::routes_agents::list_agents,
        crate::api::routes_agents::start_agent,
        crate::api::routes_agents::get_agent,
        crate::api::routes_agents::stop_agent,
        crate::api::routes_messages::list_agent_messages,
        crate::api::routes_messages::inject_agent_message,
        crate::api::routes_messages::stream_mailbox,
        crate::api::routes_a2a::list_a2a_agents,
        crate::api::routes_a2a::delegate,
        crate::api::routes_a2a::list_a2a_skills,
        crate::api::routes_a2a::invoke_by_skill,
        crate::api::routes_a2a::get_task_sidechains,
        crate::api::routes_chat::list_sessions,
        crate::api::routes_chat::create_session,
        crate::api::routes_chat::list_recent_sessions,
        crate::api::routes_chat::get_session,
        crate::api::routes_chat::close_session,
        crate::api::routes_chat::send_message,
        crate::api::routes_chat::authorize_tool,
        crate::api::routes_chat::stream_session,
        crate::api::routes_chat::get_session_todo,
        crate::api::routes_chat::resume_session,
        crate::api::routes_chat::fork_session,
        crate::api::routes_chat::list_session_children,
        crate::api::routes_sse::stream_task,
        crate::api::routes_audit::list_audit,
        crate::api::routes_audit::get_audit_stats,
        crate::api::routes_audit::verify_audit_run,
        crate::api::routes_audit::verify_audit_journal,
        crate::api::routes_audit::get_audit_anchor,
        crate::api::routes_audit::show_audit_run,
        crate::api::routes_audit::post_replay_run,
        crate::api::routes_trace::get_task_trace,
        crate::api::routes_timeline::get_task_timeline,
        crate::api::routes_hooks::list_hooks,
        crate::api::routes_review::post_review,
        crate::api::routes_plan_cache::get_plan_cache_stats,
        crate::api::routes_plan_cache::clear_plan_cache,
        crate::api::routes_approvals::list_pending_approvals,
        crate::api::routes_approvals::list_resolved_approvals,
        crate::api::routes_tools::list_tools,
        crate::api::routes_tools::describe_tool,
        crate::api::routes_triggers::list_triggers,
        crate::api::routes_triggers::create_trigger,
        crate::api::routes_triggers::reload_triggers,
        crate::api::routes_triggers::get_trigger_by_id,
        crate::api::routes_triggers::update_trigger,
        crate::api::routes_triggers::delete_trigger,
        crate::api::routes_triggers::fire_trigger,
        crate::api::routes_triggers::enable_trigger,
        crate::api::routes_triggers::disable_trigger,
        crate::api::routes_triggers::get_trigger_logs,
        crate::api::routes_notifications::list_channels,
        crate::api::routes_notifications::create_channel,
        crate::api::routes_notifications::update_channel,
        crate::api::routes_notifications::delete_channel,
        crate::api::routes_notifications::get_events,
        crate::api::routes_notifications::set_events,
        crate::api::routes_notifications::test_channels,
        crate::api::routes_notifications::notification_logs,
        crate::api::routes_stt::stt_status,
        crate::api::routes_stt::transcribe_audio,
        crate::api::routes_stt::list_transcriptions,
        crate::api::routes_stt::delete_transcription,
        crate::api::routes_stt::list_models,
        crate::api::routes_stt::get_stt_config,
        crate::api::routes_stt::update_stt_config,
        crate::api::routes_stt::reload_stt_engine,
        crate::api::routes_webhooks::handle_webhook,
        crate::api::routes_llm::get_llm_status,
        crate::api::routes_llm::ping_llm_backend,
        crate::api::routes_llm::llm_chat,
        crate::api::routes_llm::llm_complete,
        crate::api::routes_llm::get_llm_costs,
        crate::api::routes_llm::get_llm_daily_costs,
        crate::api::routes_llm::list_llm_backends,
        crate::api::routes_llm::create_llm_backend,
        crate::api::routes_llm::get_llm_backend,
        crate::api::routes_llm::update_llm_backend,
        crate::api::routes_llm::delete_llm_backend,
        crate::api::routes_llm::set_default_llm_backend,
        crate::api::routes_llm::reload_llm_router,
        crate::api::routes_model_hub::get_hardware,
        crate::api::routes_resilience::list_breakers,
        crate::api::routes_resilience::get_breaker,
        crate::api::routes_resilience::reset_breaker,
        crate::api::routes_mcp::list_servers,
        crate::api::routes_mcp::add_server,
        crate::api::routes_mcp::list_resources,
        crate::api::routes_mcp::test_connection,
        crate::api::routes_mcp::test_live_server,
        crate::api::routes_mcp::get_server_detail,
        crate::api::routes_mcp::remove_server,
        crate::api::routes_mcp::get_server_raw_config,
        crate::api::routes_mcp::restart_server,
        crate::api::routes_mcp::update_server_config,
        crate::api::routes_mcp::set_server_approval,
    ),
    components(schemas(
        ApiErrorBody,
        crate::api::server::HealthResponse,
        crate::api::server::ShutdownResponse,
        crate::api::routes_tasks::SubmitTaskRequest,
        crate::api::routes_tasks::TaskResponse,
        crate::api::routes_tasks::TaskListResponse,
        crate::api::routes_tasks::TaskListItem,
        crate::api::routes_tasks::ResumeRequest,
        crate::api::routes_tasks::ResumeResponse,
        crate::api::routes_tasks::PlanDecisionRequest,
        crate::api::routes_tasks::PlanDecisionResponse,
        crate::api::routes_agents::SkillDto,
        crate::api::routes_agents::StartAgentRequest,
        crate::api::routes_agents::AgentResponse,
        crate::api::routes_agents::AgentListResponse,
        crate::api::routes_messages::AgentMessagesResponse,
        crate::api::routes_messages::AgentMessageDto,
        crate::api::routes_messages::InjectMessageBody,
        crate::api::routes_messages::InjectMessageResponse,
        crate::api::routes_messages::SseMailboxEvent,
        crate::api::routes_a2a::A2aSkillDto,
        crate::api::routes_a2a::A2aAgentDto,
        crate::api::routes_a2a::A2aAgentsResponse,
        crate::api::routes_a2a::DelegateRequest,
        crate::api::routes_a2a::A2aSkillsResponse,
        crate::api::routes_a2a::InvokeRequest,
        crate::a2a::invoker::A2AInvocationResult,
        crate::a2a::invoker::SkillListing,
        crate::a2a::A2aDelegateResult,
        crate::a2a::SidechainRow,
        crate::api::routes_chat::CreateSessionRequest,
        crate::api::routes_chat::SendMessageRequest,
        crate::api::routes_chat::SendMessageResponse,
        crate::api::routes_chat::AuthorizeToolRequest,
        crate::api::routes_chat::ForkSessionRequest,
        crate::api::routes_chat::TodoReadResponse,
        crate::api::routes_sse::SseTaskEvent,
        crate::api::routes_audit::AuditEventResponse,
        crate::api::routes_audit::AuditListResponse,
        crate::api::routes_audit::AuditStatsResponse,
        crate::api::routes_audit::AuditJournalResponse,
        crate::api::routes_trace::TraceResponse,
        crate::api::routes_timeline::TimelineEvent,
        crate::api::routes_timeline::TimelineResponse,
        crate::api::routes_plan_cache::PlanCacheStatsResponse,
        crate::api::routes_plan_cache::ClearCacheResponse,
        crate::api::routes_approvals::PendingApprovalResponse,
        crate::api::routes_approvals::ResolvedApprovalResponse,
        crate::audit_journal::verify::VerifyChainReport,
        crate::audit_journal::verify::BrokenLink,
        crate::audit_journal::verify::BrokenLinkReason,
        crate::audit_journal::verify::VerifyJournalReport,
        crate::audit_journal::verify::JournalBreak,
        crate::audit_journal::verify::JournalBreakReason,
        crate::audit_journal::verify::JournalAnchor,
        crate::api::routes_tools::ToolListResponse,
        crate::api::routes_triggers::CreateTriggerRequest,
        crate::api::routes_triggers::TriggerSourceInput,
        crate::api::routes_triggers::UpdateTriggerRequest,
        crate::api::routes_triggers::TriggerDefinitionResponse,
        crate::api::routes_triggers::DeleteResponse,
        crate::api::routes_triggers::ReloadResponse,
        crate::api::routes_triggers::FireResponse,
        crate::api::routes_triggers::OkResponse,
        crate::api::routes_triggers::LogsResponse,
        crate::api::routes_notifications::CreateChannelRequest,
        crate::api::routes_notifications::UpdateChannelRequest,
        crate::api::routes_notifications::SetEventsRequest,
        crate::api::routes_notifications::ChannelResponse,
        crate::api::routes_notifications::EventsResponse,
        crate::api::routes_notifications::DeleteResponse,
        crate::api::routes_stt::SttStatusResponse,
        crate::api::routes_stt::TranscriptionsListResponse,
        crate::api::routes_stt::ModelInfo,
        crate::api::routes_stt::ModelsListResponse,
        crate::api::routes_stt::SttReloadResponse,
        crate::api::routes_llm::LlmStatusResponse,
        crate::api::routes_llm::PingRequest,
        crate::api::routes_llm::PingResponse,
        crate::api::routes_llm::TokenUsageResponse,
        crate::api::routes_llm::ChatRequest,
        crate::api::routes_llm::ChatResponse,
        crate::api::routes_llm::MessageDto,
        crate::api::routes_llm::CompleteRequest,
        crate::api::routes_llm::CostSummaryRow,
        crate::api::routes_llm::CostsResponse,
        crate::api::routes_llm::DailyCostEntry,
        crate::api::routes_llm::DailyCostsResponse,
        crate::api::routes_llm::CreateLlmBackendRequest,
        crate::api::routes_llm::UpdateLlmBackendRequest,
        crate::api::routes_llm::LlmBackendResponse,
        crate::api::routes_llm::LlmBackendsListResponse,
        crate::api::routes_llm::DeleteBackendResponse,
        crate::api::routes_llm::SetDefaultResponse,
        crate::api::routes_llm::ReloadRouterResponse,
        crate::api::routes_model_hub::HardwareResponse,
        crate::api::routes_resilience::ResilienceStatusResponse,
        crate::api::routes_resilience::ResetResponse,
        crate::api::routes_mcp::TestLiveRequest,
        crate::api::routes_mcp::SetApprovalBody,
    ))
)]
pub struct ApiDoc;

/// `GET /api/v1/openapi.json` - serve the generated OpenAPI document.
///
/// Non-generic so it merges cleanly into the backend-generic `build_router`.
pub(crate) async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_doc_serializes_and_covers_core_paths() {
        // GIVEN the generated OpenAPI document
        let doc = ApiDoc::openapi();

        // WHEN serialized to JSON
        let json = doc.to_json().expect("openapi document must serialize");

        // THEN it declares the driving-contract core paths and its identity
        for path in [
            "/api/v1/health",
            "/api/v1/tasks",
            "/api/v1/tasks/{id}",
            "/api/v1/tasks/{id}/resume",
            "/api/v1/agents/{name}/messages",
            "/api/v1/mailbox/stream",
        ] {
            assert!(json.contains(path), "spec is missing path {path}");
        }
        // The mailbox inject endpoint and its schemas are part of the contract.
        assert!(
            json.contains("InjectMessageBody"),
            "spec is missing InjectMessageBody"
        );
        assert!(json.contains("Apollia OS Runtime API"));
    }
}
