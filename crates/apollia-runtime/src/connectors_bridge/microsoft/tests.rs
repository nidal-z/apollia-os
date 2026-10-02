use std::sync::Arc;

use apollia_auth::{AccountId, ConnectorProvider, MicrosoftScope};
use apollia_connectors::microsoft::MicrosoftConnector;
use apollia_tools::executor::ToolExecutor;

use super::{microsoft_op_executors, ms_bearer_for};
use crate::connectors_bridge::test_support::auth_with_tokens;

// GIVEN two connected Microsoft accounts, each with its own token
// WHEN the send tool of the second account resolves its token
// THEN it is the second account's token, never the first one's
#[tokio::test]
async fn test_the_tool_of_an_account_uses_that_accounts_token() {
    let dir = tempfile::tempdir().unwrap();
    let auth = auth_with_tokens(
        dir.path(),
        ConnectorProvider::Microsoft,
        &[
            ("home@example.com", "token-home"),
            ("pro@example.com", "token-pro"),
        ],
    )
    .await;
    let connector = Arc::new(MicrosoftConnector::new(auth).unwrap());
    let accounts = [
        AccountId::new("home@example.com"),
        AccountId::new("pro@example.com"),
    ];
    let executors = microsoft_op_executors(&accounts);
    let send_pro = executors
        .iter()
        .find(|e| e.name() == "conn:microsoft:pro@example.com/outlook.send")
        .unwrap();

    let (account, token) =
        ms_bearer_for(&connector, &send_pro.account, &[MicrosoftScope::MailSend])
            .await
            .unwrap();

    assert_eq!(account.as_str(), "pro@example.com");
    assert_eq!(token, "token-pro");
}
