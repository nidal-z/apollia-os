//! The read tools of a mailbox connected over IMAP: list the folders, search
//! the messages, read one.
//!
//! All three read the user's mail, so each declares the family "read personal
//! data"; the consent that family needs is the governance's, not the tool's.
//! None of them writes: the client opens folders with `EXAMINE` and fetches
//! with `BODY.PEEK`, see [`apollia_connectors::imap::ImapClient`].
//!
//! A call names the mailbox by its address in `account`; with a single
//! mailbox connected, the field may be left out. The password is read from the
//! secret store on each call and never enters the tool's input or output.
//!
//! A mailbox the organisation policy forbids is refused before anything is
//! read from it: each call submits the mailbox it names to the admission a
//! connection goes through, under the policy deposited on the machine.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use apollia_connectors::imap::{
    ImapAccount, ImapAccountStore, ImapClient, ImapError, MailQuery, DEFAULT_FOLDER,
};
use apollia_core::SandboxProfile;
use apollia_tools::descriptor::{
    ActionFamily, ApprovalRiskLevel, FamilySource, ToolDescriptor, ToolKind,
};
use apollia_tools::executor::{ToolExecutionError, ToolExecutor as DispatchableExecutor};
use serde_json::{json, Value};

use crate::governance::{mailbox_refusal, ConnectionAdmission, PolicyAdmission};

/// Code of the error a call on a mailbox the policy forbids answers.
pub const IMAP_FORBIDDEN_CODE: &str = "imap.forbidden_by_policy";

/// Lists the folders of a connected mailbox.
pub const IMAP_LIST_FOLDERS: &str = "imap.list_folders";
/// Searches the messages of a connected mailbox.
pub const IMAP_SEARCH: &str = "imap.search";
/// Reads one message of a connected mailbox.
pub const IMAP_READ: &str = "imap.read";

const IMAP_TOOLS: [&str; 3] = [IMAP_LIST_FOLDERS, IMAP_SEARCH, IMAP_READ];

/// The descriptors of the three mailbox read tools.
#[must_use]
pub fn imap_tool_descriptors() -> Vec<ToolDescriptor> {
    IMAP_TOOLS.iter().map(|id| descriptor(id)).collect()
}

/// One executor per mailbox read tool, for the shared dispatcher.
#[must_use]
pub fn build_imap_executors() -> Vec<Box<dyn DispatchableExecutor>> {
    IMAP_TOOLS
        .iter()
        .map(|id| Box::new(ImapToolExecutor { tool: id }) as Box<dyn DispatchableExecutor>)
        .collect()
}

fn descriptor(id: &str) -> ToolDescriptor {
    let (description, impact) = match id {
        IMAP_LIST_FOLDERS => (
            "List the folders of a mailbox connected over IMAP.",
            "Reads the folder names of the connected mailbox (read-only).",
        ),
        IMAP_SEARCH => (
            "Search the messages of a mailbox connected over IMAP by sender, subject, \
             period or unread state, newest first. Answers each message's uid, sender, \
             subject, date and unread state.",
            "Reads message headers of the connected mailbox (read-only).",
        ),
        _ => (
            "Read one message of a mailbox connected over IMAP: sender, recipients, date, \
             subject, text body and attachment names. The message stays unread.",
            "Reads a message of the connected mailbox (read-only).",
        ),
    };
    ToolDescriptor {
        name: id.to_owned(),
        version: "1.0.0".to_owned(),
        description: description.to_owned(),
        kind: ToolKind::Native,
        input_schema: input_schema(id),
        output_schema: None,
        sandbox_profile: SandboxProfile::ReadOnly,
        tags: vec!["imap".to_owned(), "mail".to_owned(), "read".to_owned()],
        family: ActionFamily::ReadPersonal,
        family_source: FamilySource::Builtin,
        is_read_only: true,
        risk_score: 1,
        approval_risk_level: Some(ApprovalRiskLevel::Low),
        impact_description: Some(impact.to_owned()),
        reject_reason_required: false,
    }
}

fn input_schema(id: &str) -> Value {
    let account = json!({
        "type": "string",
        "description": "Address of the connected mailbox; optional when only one is connected"
    });
    let folder = json!({
        "type": "string",
        "description": format!("Folder name as listed; {DEFAULT_FOLDER} when absent")
    });
    match id {
        IMAP_LIST_FOLDERS => json!({
            "type": "object",
            "properties": { "account": account },
        }),
        IMAP_SEARCH => json!({
            "type": "object",
            "properties": {
                "account": account,
                "folder": folder,
                "from": { "type": "string", "description": "Text the sender contains" },
                "subject": { "type": "string", "description": "Text the subject contains" },
                "since": { "type": "string", "description": "Received on or after this day, YYYY-MM-DD" },
                "before": { "type": "string", "description": "Received before this day, YYYY-MM-DD" },
                "unread_only": { "type": "boolean", "description": "Only messages not yet read" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 100, "description": "Most messages answered, 20 by default" },
            },
        }),
        _ => json!({
            "type": "object",
            "properties": {
                "account": account,
                "folder": folder,
                "uid": { "type": "integer", "minimum": 1, "description": "uid answered by imap.search" },
            },
            "required": ["uid"],
        }),
    }
}

struct ImapToolExecutor {
    tool: &'static str,
}

impl DispatchableExecutor for ImapToolExecutor {
    fn name(&self) -> &str {
        self.tool
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn execute(
        &self,
        input: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolExecutionError>> + Send + '_>> {
        Box::pin(async move {
            let data_dir = apollia_core::paths::data_dir().ok_or_else(|| ImapError::Index {
                action: "read",
                detail: "the home directory cannot be resolved".to_owned(),
            });
            let store = data_dir.and_then(|dir| ImapAccountStore::open(&dir));
            let store = store.map_err(failed)?;
            run_tool(self.tool, &input, &store, &deposited_policy()).await
        })
    }
}

/// Admission over the organisation policy deposited on the machine.
pub(super) fn deposited_policy() -> PolicyAdmission {
    let location = apollia_permissions::policy_location();
    PolicyAdmission::new(Arc::new(apollia_permissions::load_policy(
        &location.dir,
        location.owner,
    )))
}

fn failed(e: ImapError) -> ToolExecutionError {
    ToolExecutionError::ExecutionFailed {
        code: format!("imap.{}", e.code()),
        message: e.to_string(),
    }
}

/// Run `tool` on the mailbox `input` names in `store`, once `admission`
/// admits it.
async fn run_tool(
    tool: &str,
    input: &Value,
    store: &ImapAccountStore,
    admission: &dyn ConnectionAdmission,
) -> Result<Value, ToolExecutionError> {
    let account =
        resolve_account(store, input.get("account").and_then(Value::as_str)).map_err(failed)?;
    if let Some(reason) = mailbox_refusal(admission, &account) {
        tracing::event!(
            tracing::Level::WARN,
            tool = %tool,
            account = %account.email,
            "governance.connection.forbidden.call_refused"
        );
        return Err(ToolExecutionError::ExecutionFailed {
            code: IMAP_FORBIDDEN_CODE.to_owned(),
            message: reason.to_owned(),
        });
    }
    let password = store.password(&account.email).map_err(failed)?;
    let client = ImapClient::with_system_roots().map_err(failed)?;
    run_with(&client, tool, &account, &password, input)
        .await
        .map_err(failed)
}

/// Run `tool` against `account` with `client`.
async fn run_with(
    client: &ImapClient,
    tool: &str,
    account: &ImapAccount,
    password: &str,
    input: &Value,
) -> Result<Value, ImapError> {
    let folder = input
        .get("folder")
        .and_then(Value::as_str)
        .filter(|f| !f.trim().is_empty())
        .unwrap_or(DEFAULT_FOLDER);
    match tool {
        IMAP_LIST_FOLDERS => to_value(&client.list_folders(account, password).await?),
        IMAP_SEARCH => {
            let query: MailQuery =
                serde_json::from_value(input.clone()).map_err(|e| ImapError::InvalidQuery {
                    detail: e.to_string(),
                })?;
            to_value(&client.search(account, password, &query).await?)
        }
        _ => {
            let uid = input
                .get("uid")
                .and_then(Value::as_u64)
                .and_then(|u| u32::try_from(u).ok())
                .ok_or_else(|| ImapError::InvalidQuery {
                    detail: "`uid` must be a message identifier answered by imap.search".to_owned(),
                })?;
            to_value(&client.read(account, password, folder, uid).await?)
        }
    }
}

/// The mailbox `requested` names, or the only one connected.
fn resolve_account(
    store: &ImapAccountStore,
    requested: Option<&str>,
) -> Result<ImapAccount, ImapError> {
    if let Some(email) = requested.map(str::trim).filter(|e| !e.is_empty()) {
        return store.get(email);
    }
    let mut accounts = store.list()?;
    match accounts.len() {
        1 => Ok(accounts.remove(0)),
        0 => Err(ImapError::NotFound {
            email: "(none connected)".to_owned(),
        }),
        _ => Err(ImapError::InvalidQuery {
            detail: "several mailboxes are connected; name one in `account`".to_owned(),
        }),
    }
}

fn to_value<T: serde::Serialize>(value: &T) -> Result<Value, ImapError> {
    serde_json::to_value(value).map_err(|e| ImapError::Protocol {
        detail: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A secret store holding nothing; resolving an account never reads it.
    struct NoSecrets;

    impl apollia_auth::SecretStore for NoSecrets {
        fn set(&self, _: &str, _: &str, _: &str) -> Result<(), apollia_auth::AuthError> {
            Ok(())
        }
        fn get(&self, _: &str, _: &str) -> Result<Option<String>, apollia_auth::AuthError> {
            Ok(None)
        }
        fn delete(&self, _: &str, _: &str) -> Result<(), apollia_auth::AuthError> {
            Ok(())
        }
        fn backend_id(&self) -> &'static str {
            "none"
        }
    }

    #[test]
    fn test_every_mailbox_tool_reads_personal_data() {
        // GIVEN the descriptors of the mailbox tools
        let descriptors = imap_tool_descriptors();
        // WHEN their classification is read
        // THEN each reads personal data, fixed in Apollia's code, read-only
        assert_eq!(descriptors.len(), 3);
        for d in &descriptors {
            assert_eq!(d.family, ActionFamily::ReadPersonal, "{}", d.name);
            assert_eq!(d.family_source, FamilySource::Builtin, "{}", d.name);
            assert!(d.is_read_only, "{}", d.name);
        }
    }

    #[test]
    fn test_every_mailbox_descriptor_has_an_executor() {
        // GIVEN the mailbox descriptors and executors
        let executors = build_imap_executors();
        let names: Vec<&str> = executors.iter().map(|e| e.name()).collect();
        // WHEN they are crossed
        // THEN every advertised tool can run, and none writes
        for d in imap_tool_descriptors() {
            assert!(names.contains(&d.name.as_str()), "{}", d.name);
        }
        assert!(executors.iter().all(|e| e.is_read_only()));
    }

    #[tokio::test]
    async fn test_mailbox_tools_are_registered_with_the_connectors() {
        // GIVEN the tools registered at boot, with no native account
        let registry = apollia_tools::ToolRegistryHandle::start();
        crate::supervisor::register_builtin_tools(&registry, &[], true).await;
        // WHEN the mailbox tools are looked for
        let all = registry.list().await.unwrap();
        // THEN all three are there: they take their mailbox as an argument
        for id in IMAP_TOOLS {
            assert!(all.iter().any(|d| d.name == id), "{id}");
        }
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn test_mailbox_tools_are_not_registered_when_withdrawn() {
        // GIVEN the tools registered at boot when every mailbox is forbidden
        let registry = apollia_tools::ToolRegistryHandle::start();
        crate::supervisor::register_builtin_tools(&registry, &[], false).await;
        // WHEN the mailbox tools are looked for
        let all = registry.list().await.unwrap();
        // THEN none is offered
        for id in IMAP_TOOLS {
            assert!(!all.iter().any(|d| d.name == id), "{id}");
        }
        registry.shutdown().await;
    }

    /// A store holding the one mailbox `eve@example.com` on a remote server.
    fn store_with_one_mailbox(dir: &std::path::Path) -> ImapAccountStore {
        let index = dir.join(apollia_connectors::imap::IMAP_INDEX_FILE);
        std::fs::write(
            &index,
            r#"{"accounts":[
                {"email":"eve@example.com","host":"imap.example.com","port":993,"username":"eve"}]}"#,
        )
        .unwrap();
        ImapAccountStore::with_store(index, Box::new(NoSecrets))
    }

    #[tokio::test]
    async fn test_a_forbidden_mailbox_is_refused_before_anything_is_read() {
        // GIVEN one remote mailbox, a policy denying the mailboxes and one
        //   admitting everything
        let dir = tempfile::tempdir().unwrap();
        let store = store_with_one_mailbox(dir.path());
        let deny = PolicyAdmission::new(
            crate::governance::policy_admission::valid_verdict_with_connections(
                "deny = [\"native:imap\"]",
            ),
        );
        let admit = PolicyAdmission::new(
            crate::governance::policy_admission::valid_verdict_with_connections(""),
        );

        // WHEN each tool runs on it, named and left implicit
        for tool in IMAP_TOOLS {
            for input in [
                json!({ "account": "eve@example.com", "uid": 1 }),
                json!({ "uid": 1 }),
            ] {
                let refused = run_tool(tool, &input, &store, &deny).await.unwrap_err();
                let admitted = run_tool(tool, &input, &store, &admit).await.unwrap_err();

                // THEN the forbidden mailbox is refused by the policy, before
                //   its password is read; the admitted one goes past the gate
                //   and stops on the missing password
                match refused {
                    ToolExecutionError::ExecutionFailed { code, message } => {
                        assert_eq!(code, IMAP_FORBIDDEN_CODE, "{tool}");
                        assert!(
                            message.starts_with("refused by the organisation policy"),
                            "{tool}: {message}"
                        );
                    }
                    other => panic!("{tool}: {other:?}"),
                }
                match admitted {
                    ToolExecutionError::ExecutionFailed { code, .. } => {
                        assert_ne!(code, IMAP_FORBIDDEN_CODE, "{tool}");
                    }
                    other => panic!("{tool}: {other:?}"),
                }
            }
        }
    }

    #[test]
    fn test_account_is_resolved_from_the_store() {
        // GIVEN a store with no mailbox, then with two
        let dir = tempfile::tempdir().unwrap();
        let index = dir.path().join(apollia_connectors::imap::IMAP_INDEX_FILE);
        let empty = ImapAccountStore::with_store(index.clone(), Box::new(NoSecrets));
        // WHEN no account is named
        // THEN the empty store answers not found
        assert_eq!(
            resolve_account(&empty, None).unwrap_err().code(),
            "not_found"
        );
        std::fs::write(
            &index,
            r#"{"accounts":[
                {"email":"a@example.org","host":"h","port":993,"username":"a"},
                {"email":"b@example.org","host":"h","port":993,"username":"b"}]}"#,
        )
        .unwrap();
        // AND two mailboxes need a name, which then picks one
        assert_eq!(
            resolve_account(&empty, None).unwrap_err().code(),
            "invalid_query"
        );
        assert_eq!(
            resolve_account(&empty, Some("B@example.org"))
                .unwrap()
                .username,
            "b"
        );
    }
}
