use std::sync::Arc;

use apollia_auth::{AccountId, ConnectorProvider, GoogleScope};
use apollia_connectors::google::GoogleConnector;
use apollia_tools::executor::ToolExecutor;

use super::{bearer_for, google_op_executors};
use crate::connectors_bridge::test_support::auth_with_tokens;

// GIVEN two connected Google accounts, each with its own token
// WHEN the send tool of the second account resolves its token
// THEN it is the second account's token, never the first one's
#[tokio::test]
async fn test_the_tool_of_an_account_uses_that_accounts_token() {
    let dir = tempfile::tempdir().unwrap();
    let auth = auth_with_tokens(
        dir.path(),
        ConnectorProvider::Google,
        &[
            ("home@example.com", "token-home"),
            ("pro@example.com", "token-pro"),
        ],
    )
    .await;
    let connector = Arc::new(GoogleConnector::new(auth).unwrap());
    let accounts = [
        AccountId::new("home@example.com"),
        AccountId::new("pro@example.com"),
    ];
    let executors = google_op_executors(&accounts);
    let send_pro = executors
        .iter()
        .find(|e| e.name() == "conn:google:pro@example.com/gmail.send")
        .unwrap();

    let (account, token) = bearer_for(&connector, &send_pro.account, &[GoogleScope::MailSend])
        .await
        .unwrap();

    assert_eq!(account.as_str(), "pro@example.com");
    assert_eq!(token, "token-pro");
}

// GIVEN two connected Google accounts
// WHEN their executors are built
// THEN each operation has one executor per account, named after its account
#[test]
fn test_every_operation_has_one_executor_per_account() {
    let accounts = [
        AccountId::new("a@example.com"),
        AccountId::new("b@example.com"),
    ];

    let executors = google_op_executors(&accounts);

    assert_eq!(executors.len(), 2 * super::GOOGLE_OPERATIONS.len());
    for op in super::GOOGLE_OPERATIONS {
        for account in &accounts {
            let name = format!("conn:google:{}/{op}", account.as_str());
            let executor = executors.iter().find(|e| e.name() == name).unwrap();
            assert_eq!(&executor.account, account, "{name}");
        }
    }
    assert!(executors.iter().all(|e| !e.name().starts_with("gmail.")));
}

// GIVEN no connected Google account
// WHEN the executors are built
// THEN there is none: no account is chosen in place of a missing one
#[test]
fn test_no_account_builds_no_executor() {
    assert!(google_op_executors(&[]).is_empty());
}
