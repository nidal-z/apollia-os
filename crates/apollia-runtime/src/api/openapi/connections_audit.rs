//! The connections and the audit trail of the `/api/v1` document, declared
//! apart from [`super::ApiDoc`] and merged into it.
//!
//! [`ConnectionsAndAudit`] is the first modifier of the document: its paths
//! and schemas are merged before the security and local-model modifiers run,
//! so they see every operation as if it had been declared in one place.

use utoipa::openapi::OpenApi as Document;
use utoipa::{Modify, OpenApi};

/// The paths and schemas of the connections and of the audit trail.
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::api::routes_audit::verify_audit_run,
        crate::api::routes_audit::verify_audit_journal,
        crate::api::routes_audit::get_audit_anchor,
        crate::api::routes_audit::show_audit_run,
        crate::api::routes_audit::show_audit_entry,
        crate::api::routes_audit::list_audit_journal,
        crate::api::routes_audit::post_replay_run,
        crate::api::routes_audit_export::export_audit_journal,
        crate::api::routes_audit_activity::get_connection_activity,
        crate::api::routes_connections::list_connections,
        crate::api::routes_connections::folders::add_folder_connection,
        crate::api::routes_connections::folders::remove_folder_connection,
        crate::api::routes_connections::agents::get_connection_agents,
        crate::api::routes_connections::agents::grant_connection,
        crate::api::routes_connections::check::check_connection,
        crate::api::routes_connections::pause::pause_connection,
        crate::api::routes_connections::pause::resume_connection,
    ),
    components(schemas(
        crate::api::routes_audit::AuditJournalResponse,
        crate::api::routes_audit::AuditJournalPageResponse,
        crate::api::routes_audit_activity::ConnectionActivityResponse,
        crate::audit_journal::AgentActivity,
        crate::api::routes_audit::AuditJournalEntryDetailResponse,
        crate::audit_journal::JournalExportFormat,
        crate::audit_journal::JournalSummary,
        crate::audit_journal::JournalNature,
        crate::audit_journal::JournalOutcome,
        crate::audit_journal::ToolCallDetail,
        crate::audit_journal::verify::VerifyChainReport,
        crate::audit_journal::verify::BrokenLink,
        crate::audit_journal::verify::BrokenLinkReason,
        crate::audit_journal::verify::VerifyJournalReport,
        crate::audit_journal::verify::JournalBreak,
        crate::audit_journal::verify::JournalBreakReason,
        crate::audit_journal::verify::JournalStart,
        crate::audit_journal::verify::JournalAnchor,
        crate::audit_journal::JournalNotice,
        crate::connections::Connection,
        crate::connections::ConnectionTool,
        crate::connections::ConnectionKind,
        crate::connections::ConnectionState,
        crate::api::routes_connections::folders::AddFolderBody,
        crate::api::routes_connections::folders::FolderErrorBody,
        crate::api::routes_connections::folders::DependentSource,
        crate::api::routes_connections::agents::GrantConnectionBody,
        crate::api::routes_connections::check::CheckConnectionBody,
        crate::connections::agents::ConnectionAgents,
        crate::connections::agents::ConnectionAgent,
    ))
)]
pub(super) struct ConnectionsAndAudit;

impl Modify for ConnectionsAndAudit {
    fn modify(&self, openapi: &mut Document) {
        openapi.merge(Self::openapi());
    }
}
