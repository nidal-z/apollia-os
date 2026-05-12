//! Google Workspace connector — implements [`Connector`](crate::trait_def::Connector).
//!
//! Composes the Gmail / Calendar / Drive Workspace clients into a single
//! connector instance registered with the runtime [`ConnectorRegistry`](crate::registry::ConnectorRegistry).

pub mod calendar;
pub mod drive_workspace;
pub mod gmail;

use std::sync::Arc;

use apollia_auth::{AccountId, AuthManager, ConnectorProvider, GoogleScope};
use async_trait::async_trait;
use serde::Deserialize;

use crate::{
    error::ConnectorError,
    http::HttpClient,
    manifest::ConnectorManifest,
    operation::{ApprovalPolicy, OperationSpec},
    trait_def::{Connector, HealthReport},
};

pub use calendar::CalendarClient;
pub use drive_workspace::DriveWorkspaceClient;
pub use gmail::GmailClient;

const GOOGLE_PROVIDER_ID: &str = "google";

const MANIFEST: ConnectorManifest = ConnectorManifest {
    id: GOOGLE_PROVIDER_ID,
    name: "Google Workspace",
    description: "Gmail (send + compose), Google Calendar, Google Drive Workspace",
    publisher: "Google LLC",
    services: &["gmail", "gcal", "gdrive"],
};

/// Google Workspace connector. Construct once at startup, register with the
/// [`ConnectorRegistry`](crate::registry::ConnectorRegistry).
///
/// Holds typed sub-clients (Gmail, Calendar, Drive Workspace) and an
/// [`AuthManager`] handle used to resolve tokens at call time.
#[derive(Clone)]
pub struct GoogleConnector {
    auth: Arc<AuthManager>,
    gmail: GmailClient,
    calendar: CalendarClient,
    drive: DriveWorkspaceClient,
}

impl GoogleConnector {
    /// Build a Google connector.
    pub fn new(auth: Arc<AuthManager>) -> Result<Self, ConnectorError> {
        let http = HttpClient::new(GOOGLE_PROVIDER_ID)?;
        Ok(Self {
            auth,
            gmail: GmailClient::new(http.clone()),
            calendar: CalendarClient::new(http.clone()),
            drive: DriveWorkspaceClient::new(http),
        })
    }

    /// Access the Gmail sub-client.
    pub fn gmail(&self) -> &GmailClient {
        &self.gmail
    }

    /// Access the Calendar sub-client.
    pub fn calendar(&self) -> &CalendarClient {
        &self.calendar
    }

    /// Access the Drive Workspace sub-client.
    pub fn drive(&self) -> &DriveWorkspaceClient {
        &self.drive
    }

    /// Resolve the current bearer token for `account_id`.
    ///
    /// Connector implementations call this on each request; the underlying
    /// [`AuthManager`] caches and refreshes (with singleflight) so successive
    /// calls are cheap.
    pub async fn bearer(
        &self,
        account_id: &AccountId,
        scopes: &[GoogleScope],
    ) -> Result<String, ConnectorError> {
        let cfg = apollia_auth::build_google_provider(scopes);
        let token = self
            .auth
            .get_valid_token(ConnectorProvider::Google, account_id, &cfg)
            .await?;
        Ok(token.access_token)
    }
}

#[async_trait]
impl Connector for GoogleConnector {
    fn id(&self) -> &'static str {
        GOOGLE_PROVIDER_ID
    }

    fn manifest(&self) -> &ConnectorManifest {
        &MANIFEST
    }

    fn operations(&self) -> &[OperationSpec] {
        OPERATIONS
    }

    async fn check(&self, account_id: &AccountId) -> Result<HealthReport, ConnectorError> {
        // The cheapest authenticated probe Google offers is the OIDC userinfo
        // endpoint, which only requires the profile scope (always implicitly
        // granted).
        let cfg = apollia_auth::build_google_provider(&[]);
        let token = self
            .auth
            .get_valid_token(ConnectorProvider::Google, account_id, &cfg)
            .await?;
        let access = token.access_token.clone();
        let access_for_refresh = access.clone();
        let http = HttpClient::new(GOOGLE_PROVIDER_ID)?;
        let info: UserInfo = http
            .get_json(
                "https://www.googleapis.com/oauth2/v3/userinfo",
                &access,
                || async move { Ok::<_, ConnectorError>(access_for_refresh) },
            )
            .await?;
        Ok(HealthReport {
            reachable: true,
            granted_scopes: token.scopes,
            detail: format!("connected as {}", info.email.unwrap_or_else(|| "<unknown>".into())),
        })
    }
}

/// Minimal subset of the Google OIDC userinfo response.
#[derive(Debug, Deserialize)]
struct UserInfo {
    #[serde(default)]
    email: Option<String>,
}

// ─── Operations catalog ──────────────────────────────────────────────────────

/// All operations exposed by the Google Workspace connector.
///
/// Build-time array consumed by `apollia-tools::registry` to register each
/// operation as a native tool with the canonical naming `<service>.<action>`.
const OPERATIONS: &[OperationSpec] = &[
    OperationSpec {
        id: "gmail.send",
        service: "gmail",
        action: "send",
        scopes_required: &["mail.send"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Send an email via Gmail.\n\nWhen to use: when the user explicitly asks to email someone.\nInputs: to (email), subject (string), body (plain text), optional cc, bcc.\nApproval: required.\nSide effects: sends an email from the connected account.",
    },
    OperationSpec {
        id: "gmail.compose_draft",
        service: "gmail",
        action: "compose_draft",
        scopes_required: &["mail.compose"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Create a draft email in the user's Drafts folder.\n\nWhen to use: when the user wants to prepare an email but not send it yet.\nInputs: to (email), subject (string), body (plain text), optional cc, bcc.\nApproval: required.\nSide effects: creates a draft visible in the user's Gmail Drafts folder.",
    },
    OperationSpec {
        id: "gmail.list_drafts",
        service: "gmail",
        action: "list_drafts",
        scopes_required: &["mail.compose"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "List the user's existing draft emails.\n\nWhen to use: to find a previously composed draft.\nInputs: max_results (integer).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gmail.delete_draft",
        service: "gmail",
        action: "delete_draft",
        scopes_required: &["mail.compose"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Delete a draft email by id.\n\nWhen to use: when the user wants to discard a draft.\nInputs: draft_id (string).\nApproval: required.\nSide effects: permanently removes the draft.",
    },
    OperationSpec {
        id: "gcal.list_events",
        service: "gcal",
        action: "list_events",
        scopes_required: &["calendar.read"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "List calendar events in a time window.\n\nWhen to use: to find what is on the user's calendar.\nInputs: calendar_id (string, default \"primary\"), time_min, time_max (RFC 3339), max_results, q (text filter).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gcal.get_event",
        service: "gcal",
        action: "get_event",
        scopes_required: &["calendar.read"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Retrieve a single calendar event by id.\n\nWhen to use: to inspect a specific event.\nInputs: calendar_id (string), event_id (string).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gcal.create_event",
        service: "gcal",
        action: "create_event",
        scopes_required: &["calendar.write"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Create a new calendar event.\n\nWhen to use: when the user wants to schedule something on their calendar.\nInputs: calendar_id (string), summary, start, end, optional description, location, attendees.\nApproval: required.\nSide effects: creates a calendar event that may notify attendees.",
    },
    OperationSpec {
        id: "gcal.update_event",
        service: "gcal",
        action: "update_event",
        scopes_required: &["calendar.write"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Update an existing calendar event (full replace).\n\nWhen to use: when the user wants to modify an existing event.\nInputs: calendar_id (string), event_id (string), full event draft.\nApproval: required.\nSide effects: updates the event and may re-notify attendees.",
    },
    OperationSpec {
        id: "gcal.delete_event",
        service: "gcal",
        action: "delete_event",
        scopes_required: &["calendar.write"],
        approval: ApprovalPolicy::ConfirmPhrase,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Delete a calendar event.\n\nWhen to use: when the user wants to cancel an event.\nInputs: calendar_id (string), event_id (string).\nApproval: required + confirmation phrase.\nSide effects: permanently removes the event from the calendar.",
    },
    OperationSpec {
        id: "gdrive.workspace_list",
        service: "gdrive",
        action: "workspace_list",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "List files in the agent's Drive workspace folder.\n\nWhen to use: to see what files the agent has created or has access to.\nInputs: agent_slug (string).\nApproval: not required.\nSide effects: none. Scope: only files under Drive/Apollia/<agent_slug>/.",
    },
    OperationSpec {
        id: "gdrive.workspace_read",
        service: "gdrive",
        action: "workspace_read",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Read a workspace file by id.\n\nWhen to use: to fetch the content of a previously-listed file.\nInputs: file_id (string).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "gdrive.workspace_write",
        service: "gdrive",
        action: "workspace_write",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Create or replace a file in the agent's Drive workspace.\n\nWhen to use: to persist text content to the user's Drive scoped to this agent.\nInputs: agent_slug (string), name (string), content (string).\nApproval: required.\nSide effects: writes a file under Drive/Apollia/<agent_slug>/.",
    },
    OperationSpec {
        id: "gdrive.workspace_delete",
        service: "gdrive",
        action: "workspace_delete",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Move a workspace file to Trash.\n\nWhen to use: when the user wants to remove a file the agent created.\nInputs: file_id (string).\nApproval: required.\nSide effects: file is moved to Drive Trash (recoverable for 30 days).",
    },
    OperationSpec {
        id: "gdrive.workspace_share",
        service: "gdrive",
        action: "workspace_share",
        scopes_required: &["drive.workspace"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Share a workspace file with a specific email address (reader role).\n\nWhen to use: when the user explicitly asks to share a file with someone.\nInputs: file_id (string), email (string).\nApproval: required.\nSide effects: grants reader access to the specified email.",
    },
];

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_id_matches_provider_id() {
        assert_eq!(MANIFEST.id, GOOGLE_PROVIDER_ID);
    }

    #[test]
    fn test_manifest_lists_three_services() {
        assert_eq!(MANIFEST.services, &["gmail", "gcal", "gdrive"]);
    }

    #[test]
    fn test_operations_cover_expected_services() {
        let services: std::collections::HashSet<&'static str> =
            OPERATIONS.iter().map(|op| op.service).collect();
        assert!(services.contains("gmail"));
        assert!(services.contains("gcal"));
        assert!(services.contains("gdrive"));
    }

    #[test]
    fn test_write_operations_require_approval() {
        let writes = ["gmail.send", "gcal.create_event", "gdrive.workspace_write"];
        for id in writes {
            let op = OPERATIONS.iter().find(|o| o.id == id).expect(id);
            assert!(
                op.requires_approval(),
                "expected {id} to require approval"
            );
        }
    }

    #[test]
    fn test_read_operations_auto_approve() {
        let reads = ["gmail.list_drafts", "gcal.list_events", "gdrive.workspace_read"];
        for id in reads {
            let op = OPERATIONS.iter().find(|o| o.id == id).expect(id);
            assert!(
                op.is_read_only(),
                "expected {id} to auto-approve"
            );
        }
    }

    #[test]
    fn test_delete_event_uses_confirm_phrase() {
        let op = OPERATIONS
            .iter()
            .find(|o| o.id == "gcal.delete_event")
            .expect("op");
        assert_eq!(op.approval, ApprovalPolicy::ConfirmPhrase);
    }

    #[test]
    fn test_no_restricted_scopes_in_v0_1_0_catalog() {
        // GIVEN the Google operations catalog
        // WHEN we collect every scope required
        // THEN none of the restricted scopes appear (per ADR-088 free-tier policy).
        let restricted = ["mail.read_inbox", "mail.full", "drive.read_all", "drive.full"];
        for op in OPERATIONS {
            for scope in op.scopes_required {
                assert!(
                    !restricted.contains(scope),
                    "operation {} requires restricted scope {} — must be deferred to Expert Mode",
                    op.id,
                    scope
                );
            }
        }
    }
}
