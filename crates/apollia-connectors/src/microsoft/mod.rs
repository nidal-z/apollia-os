//! Microsoft 365 connector, implements [`Connector`](crate::trait_def::Connector).
//!
//! Composes the Outlook Mail / Calendar / OneDrive sub-clients into a single
//! connector. Microsoft Graph has no CASA-like restriction, so the v0.1.0
//! surface is broader than Google's free-tier surface (Mail read + search
//! + send are all available, not just send).

pub mod calendar;
pub mod mail;
pub mod onedrive;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use apollia_auth::{AccountId, AuthManager, ConnectorProvider, MicrosoftScope};
use serde::Deserialize;

use crate::{
    error::ConnectorError,
    http::HttpClient,
    manifest::ConnectorManifest,
    operation::{ApprovalPolicy, OperationSpec},
    trait_def::{Connector, HealthReport},
};

pub use calendar::OutlookCalendarClient;
pub use mail::OutlookMailClient;
pub use onedrive::OneDriveClient;

const MICROSOFT_PROVIDER_ID: &str = "microsoft";

const MANIFEST: ConnectorManifest = ConnectorManifest {
    id: MICROSOFT_PROVIDER_ID,
    name: "Microsoft 365",
    description: "Outlook Mail, Outlook Calendar, OneDrive",
    publisher: "Microsoft Corporation",
    services: &["outlook", "outlook_cal", "onedrive"],
};

/// Microsoft 365 connector.
#[derive(Clone)]
pub struct MicrosoftConnector {
    auth: Arc<AuthManager>,
    mail: OutlookMailClient,
    calendar: OutlookCalendarClient,
    onedrive: OneDriveClient,
}

impl MicrosoftConnector {
    /// Build a Microsoft connector.
    pub fn new(auth: Arc<AuthManager>) -> Result<Self, ConnectorError> {
        let http = HttpClient::new(MICROSOFT_PROVIDER_ID)?;
        Ok(Self {
            auth,
            mail: OutlookMailClient::new(http.clone()),
            calendar: OutlookCalendarClient::new(http.clone()),
            onedrive: OneDriveClient::new(http),
        })
    }

    /// Outlook Mail sub-client.
    pub fn mail(&self) -> &OutlookMailClient {
        &self.mail
    }

    /// Outlook Calendar sub-client.
    pub fn calendar(&self) -> &OutlookCalendarClient {
        &self.calendar
    }

    /// OneDrive sub-client.
    pub fn onedrive(&self) -> &OneDriveClient {
        &self.onedrive
    }

    /// Resolve the current bearer token for `account_id` with the given scopes.
    pub async fn bearer(
        &self,
        account_id: &AccountId,
        scopes: &[MicrosoftScope],
    ) -> Result<String, ConnectorError> {
        let cfg = apollia_auth::build_microsoft_provider(scopes);
        let token = self
            .auth
            .get_valid_token(ConnectorProvider::Microsoft, account_id, &cfg)
            .await?;
        Ok(token.access_token)
    }
}

impl Connector for MicrosoftConnector {
    fn id(&self) -> &'static str {
        MICROSOFT_PROVIDER_ID
    }

    fn manifest(&self) -> &ConnectorManifest {
        &MANIFEST
    }

    fn operations(&self) -> &[OperationSpec] {
        OPERATIONS
    }

    fn check<'a>(
        &'a self,
        account_id: &'a AccountId,
    ) -> Pin<Box<dyn Future<Output = Result<HealthReport, ConnectorError>> + Send + 'a>> {
        Box::pin(async move {
            // Probe via /me which only requires User.Read (always implicitly granted).
            let cfg = apollia_auth::build_microsoft_provider(&[]);
            let token = self
                .auth
                .get_valid_token(ConnectorProvider::Microsoft, account_id, &cfg)
                .await?;
            let access = token.access_token.clone();
            let access_for_refresh = access.clone();
            let http = HttpClient::new(MICROSOFT_PROVIDER_ID)?;
            let info: UserInfo = http
                .get_json(
                    "https://graph.microsoft.com/v1.0/me",
                    &access,
                    || async move { Ok::<_, ConnectorError>(access_for_refresh) },
                )
                .await?;
            Ok(HealthReport {
                reachable: true,
                granted_scopes: token.scopes,
                detail: format!(
                    "connected as {}",
                    info.user_principal_name
                        .or(info.mail)
                        .unwrap_or_else(|| "<unknown>".into())
                ),
            })
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserInfo {
    #[serde(default)]
    user_principal_name: Option<String>,
    #[serde(default)]
    mail: Option<String>,
}

// ─── Operations catalog ──────────────────────────────────────────────────────

const OPERATIONS: &[OperationSpec] = &[
    OperationSpec {
        id: "outlook.search",
        service: "outlook",
        action: "search",
        scopes_required: &["mail.read"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Search the user's Outlook mailbox.\n\nWhen to use: to find emails matching a query (sender, subject, body).\nInputs: query (string, Graph $search syntax), top (integer, max results).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "outlook.get",
        service: "outlook",
        action: "get",
        scopes_required: &["mail.read"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Retrieve a single Outlook message by id.\n\nWhen to use: to read the body of a message found by search.\nInputs: message_id (string).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "outlook.send",
        service: "outlook",
        action: "send",
        scopes_required: &["mail.send"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Compose and send an Outlook email.\n\nWhen to use: when the user explicitly asks to email someone.\nInputs: subject (string), body (plain text or HTML), to (list of emails), optional cc, bcc.\nApproval: required.\nSide effects: sends an email from the connected Microsoft account.",
    },
    OperationSpec {
        id: "outlook.reply",
        service: "outlook",
        action: "reply",
        scopes_required: &["mail.send"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Reply to an Outlook message.\n\nWhen to use: to respond to a specific email.\nInputs: message_id (string), comment (plain text).\nApproval: required.\nSide effects: sends a reply email referencing the original.",
    },
    OperationSpec {
        id: "outlook.list_folders",
        service: "outlook",
        action: "list_folders",
        scopes_required: &["mail.read"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "List the user's Outlook mail folders.\n\nWhen to use: to discover folder ids for moving messages.\nInputs: none.\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "outlook.move",
        service: "outlook",
        action: "move",
        scopes_required: &["mail.read"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Move an Outlook message to a different folder.\n\nWhen to use: to triage messages (e.g. archive, classify).\nInputs: message_id (string), destination_folder_id (string).\nApproval: required.\nSide effects: changes the parent folder of the message.",
    },
    OperationSpec {
        id: "outlook_cal.list_events",
        service: "outlook_cal",
        action: "list_events",
        scopes_required: &["calendar.read"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "List Outlook calendar events in a time window.\n\nWhen to use: to find what is on the user's Outlook calendar.\nInputs: start_after, end_before (RFC 3339 UTC), top (integer).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "outlook_cal.get_event",
        service: "outlook_cal",
        action: "get_event",
        scopes_required: &["calendar.read"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Get a single Outlook calendar event by id.\n\nInputs: event_id (string).\nApproval: not required.",
    },
    OperationSpec {
        id: "outlook_cal.create_event",
        service: "outlook_cal",
        action: "create_event",
        scopes_required: &["calendar.write"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Create a new Outlook calendar event.\n\nWhen to use: when the user wants to schedule something in Outlook.\nInputs: subject (string), start, end (date_time + time_zone), optional body, location, attendees.\nApproval: required.\nSide effects: creates an event that may notify attendees.",
    },
    OperationSpec {
        id: "outlook_cal.update_event",
        service: "outlook_cal",
        action: "update_event",
        scopes_required: &["calendar.write"],
        approval: ApprovalPolicy::AlwaysRequireApproval,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Update an existing Outlook calendar event (PATCH semantics).\n\nInputs: event_id (string), partial draft.\nApproval: required.\nSide effects: updates the event and may re-notify attendees.",
    },
    OperationSpec {
        id: "outlook_cal.delete_event",
        service: "outlook_cal",
        action: "delete_event",
        scopes_required: &["calendar.write"],
        approval: ApprovalPolicy::ConfirmPhrase,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Delete an Outlook calendar event.\n\nInputs: event_id (string).\nApproval: required + confirmation phrase.\nSide effects: permanently removes the event.",
    },
    OperationSpec {
        id: "onedrive.search",
        service: "onedrive",
        action: "search",
        scopes_required: &["files.read"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Search the user's OneDrive for items matching a query.\n\nInputs: query (string), top (integer).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "onedrive.get_metadata",
        service: "onedrive",
        action: "get_metadata",
        scopes_required: &["files.read"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Get metadata for a OneDrive item by id.\n\nInputs: item_id (string).\nApproval: not required.\nSide effects: none.",
    },
    OperationSpec {
        id: "onedrive.download",
        service: "onedrive",
        action: "download",
        scopes_required: &["files.read"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "Download the content of a OneDrive item.\n\nInputs: item_id (string).\nApproval: not required.\nSide effects: none (read-only).",
    },
    OperationSpec {
        id: "onedrive.list_recent",
        service: "onedrive",
        action: "list_recent",
        scopes_required: &["files.read"],
        approval: ApprovalPolicy::AutoApprove,
        input_schema: serde_json::Value::Null,
        output_schema: serde_json::Value::Null,
        description: "List recently accessed OneDrive items.\n\nInputs: top (integer).\nApproval: not required.\nSide effects: none.",
    },
];

/// All operations exposed by the Microsoft 365 connector.
///
/// Static accessor mirroring [`crate::google::operations`], consumed by
/// `apollia-runtime::connectors_bridge` to derive tool descriptors without a
/// live `MicrosoftConnector` instance.
pub fn operations() -> &'static [OperationSpec] {
    OPERATIONS
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_lists_three_services() {
        // GIVEN the Microsoft connector manifest
        // WHEN its service list is read
        // THEN the three shipped services are named, in order
        assert_eq!(MANIFEST.services, &["outlook", "outlook_cal", "onedrive"]);
    }

    #[test]
    fn test_operations_cover_three_services() {
        // GIVEN the declared Microsoft operations
        // WHEN the services they belong to are collected
        let services: std::collections::HashSet<&'static str> =
            OPERATIONS.iter().map(|op| op.service).collect();
        // THEN each of the three services carries at least one operation
        assert!(services.contains("outlook"));
        assert!(services.contains("outlook_cal"));
        assert!(services.contains("onedrive"));
    }

    #[test]
    fn test_write_operations_require_approval() {
        // GIVEN the three operations that write to a Microsoft service
        let writes = ["outlook.send", "outlook.reply", "outlook_cal.create_event"];
        // WHEN each is looked up in the operation table
        for id in writes {
            let op = OPERATIONS.iter().find(|o| o.id == id).expect(id);
            // THEN every one of them is gated behind an approval
            assert!(op.requires_approval(), "expected {id} to require approval");
        }
    }

    #[test]
    fn test_read_operations_auto_approve() {
        // GIVEN the five operations that only read
        let reads = [
            "outlook.search",
            "outlook.get",
            "outlook_cal.list_events",
            "onedrive.search",
            "onedrive.download",
        ];
        // WHEN each is looked up in the operation table
        for id in reads {
            let op = OPERATIONS.iter().find(|o| o.id == id).expect(id);
            // THEN all of them are read-only, so no approval is asked for
            assert!(op.is_read_only(), "expected {id} to auto-approve");
        }
    }

    #[test]
    fn test_delete_event_uses_confirm_phrase() {
        // GIVEN the calendar event deletion operation
        let op = OPERATIONS
            .iter()
            .find(|o| o.id == "outlook_cal.delete_event")
            .expect("op");
        // WHEN its approval policy is read
        // THEN deletion asks for a typed confirmation, not a plain yes
        assert_eq!(op.approval, ApprovalPolicy::ConfirmPhrase);
    }
}
