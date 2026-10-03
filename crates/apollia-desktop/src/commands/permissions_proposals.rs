//! Tauri IPC commands for the onboarding permission proposals flow.
//!
//! When the onboarding agent finalises the user profile, it derives a list of
//! permission rules from the answers. The onboarding modal renders one
//! approval card per proposal; on the user's click the proposal is applied
//! (stored as a rule) or dismissed. No native tool writes a permission rule:
//! an agent proposes, the user decides.
//!
//! The logic lives in [`apollia_runtime::permission_rules`] and serves both
//! modes: embedded, these commands call it in this process; attached, the
//! daemon's `/api/v1/permissions/proposals` routes run it.

use std::path::PathBuf;

use apollia_runtime::api::routes_permissions::{AppliedProposal, DismissedProposal};
use apollia_runtime::permission_rules::{self, ProposedRuleView};
use tauri::State;

use crate::attached::{AttachedClient, RuntimeLink};

/// The proposals, as the daemon serves them.
const PROPOSALS_PATH: &str = "/api/v1/permissions/proposals";

/// Reads the proposals the onboarding agent stored, empty when none is.
#[tauri::command]
pub async fn list_proposed_permission_rules(
    link: State<'_, RuntimeLink>,
) -> Result<Vec<ProposedRuleView>, String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => {
            // A missing home reads as no proposal rather than an error.
            let home = apollia_core::paths::home_dir_or_temp()
                .display()
                .to_string();
            permission_rules::list_proposals(apollia_core::paths::data_dir_under(home))
                .await
                .map_err(|e| e.to_string())
        }
        RuntimeLink::Attached(remote) => list_attached(remote).await,
    }
}

/// Applies the proposal at `index`: stores it as a rule created by the
/// onboarding agent, then removes it from the list. Returns the rule's id.
#[tauri::command]
pub async fn apply_proposed_permission_rule(
    link: State<'_, RuntimeLink>,
    index: usize,
) -> Result<i64, String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => permission_rules::apply_proposal(data_dir()?, index)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => apply_attached(remote, index).await,
    }
}

/// Removes the proposal at `index` without storing any rule.
#[tauri::command]
pub async fn dismiss_proposed_permission_rule(
    link: State<'_, RuntimeLink>,
    index: usize,
) -> Result<(), String> {
    match link.inner() {
        RuntimeLink::Embedded(_) => permission_rules::dismiss_proposal(data_dir()?, index)
            .await
            .map_err(|e| e.to_string()),
        RuntimeLink::Attached(remote) => dismiss_attached(remote, index).await,
    }
}

fn data_dir() -> Result<PathBuf, String> {
    let home = apollia_core::paths::home_string_or_err()?;
    Ok(apollia_core::paths::data_dir_under(home))
}

async fn list_attached(remote: &AttachedClient) -> Result<Vec<ProposedRuleView>, String> {
    Ok(remote.get(PROPOSALS_PATH).await?)
}

async fn apply_attached(remote: &AttachedClient, index: usize) -> Result<i64, String> {
    let path = format!("{PROPOSALS_PATH}/{index}/apply");
    let applied: AppliedProposal = remote.post(&path, &()).await?;
    Ok(applied.rule_id)
}

async fn dismiss_attached(remote: &AttachedClient, index: usize) -> Result<(), String> {
    let _: DismissedProposal = remote.delete(&format!("{PROPOSALS_PATH}/{index}")).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    const TOKEN: &str = "token-of-this-installation";

    fn bearer() -> String {
        format!("Bearer {TOKEN}")
    }

    #[tokio::test]
    async fn test_attached_the_proposals_are_read_from_the_daemon_with_the_token() {
        // GIVEN a daemon holding one proposal, answering only the token
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/permissions/proposals"))
            .and(header("authorization", bearer().as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {"index": 0, "tool_name": "file_read", "action": "allow",
                 "arg_prefix": null, "scope": "global"}
            ])))
            .expect(1)
            .mount(&server)
            .await;
        let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

        // WHEN the attached arm lists the proposals
        let proposals = list_attached(&client).await.unwrap();

        // THEN the daemon's proposal reads as the card draws it
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].tool_name, "file_read");
    }

    #[tokio::test]
    async fn test_attached_a_proposal_is_applied_then_another_dismissed_on_the_daemon() {
        // GIVEN a daemon accepting the application of the proposal at 2 and
        // the dismissal of the one at 0
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/permissions/proposals/2/apply"))
            .and(header("authorization", bearer().as_str()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"rule_id": 41})),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/api/v1/permissions/proposals/0"))
            .and(header("authorization", bearer().as_str()))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"dismissed": 0})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

        // WHEN the attached arm applies one and dismisses the other
        let rule_id = apply_attached(&client, 2).await.unwrap();
        dismiss_attached(&client, 0).await.unwrap();

        // THEN the rule id is the daemon's, and both calls reached it once
        assert_eq!(rule_id, 41);
    }

    #[tokio::test]
    async fn test_attached_a_refusal_of_the_daemon_shows_its_own_message() {
        // GIVEN a daemon with a single proposal, refusing the cursor 3
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/permissions/proposals/3/apply"))
            .respond_with(ResponseTemplate::new(404).set_body_json(
                serde_json::json!({"error": "proposal index 3 out of range (len=1)"}),
            ))
            .mount(&server)
            .await;
        let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

        // WHEN the attached arm applies it
        let error = apply_attached(&client, 3).await.unwrap_err();

        // THEN the interface shows the daemon's words
        assert!(
            error.contains("proposal index 3 out of range (len=1)"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn test_attached_a_refused_listing_shows_the_daemon_message() {
        // GIVEN a daemon whose namespace database does not open
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/permissions/proposals"))
            .and(header("authorization", bearer().as_str()))
            .respond_with(ResponseTemplate::new(500).set_body_json(
                serde_json::json!({"error": "failed to open /data/memory/onboarding.db: locked"}),
            ))
            .expect(1)
            .mount(&server)
            .await;
        let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

        // WHEN the attached arm lists the proposals
        let error = list_attached(&client).await.unwrap_err();

        // THEN the interface shows the daemon's words
        assert!(
            error.contains("failed to open /data/memory/onboarding.db: locked"),
            "{error}"
        );
    }

    #[tokio::test]
    async fn test_attached_a_refused_dismissal_shows_the_daemon_message() {
        // GIVEN a daemon holding no proposal any more
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/api/v1/permissions/proposals/0"))
            .and(header("authorization", bearer().as_str()))
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(serde_json::json!({"error": "no proposed rules in memory"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = AttachedClient::new(server.uri(), TOKEN).unwrap();

        // WHEN the attached arm dismisses the proposal at 0
        let error = dismiss_attached(&client, 0).await.unwrap_err();

        // THEN the interface shows the daemon's words
        assert!(error.contains("no proposed rules in memory"), "{error}");
    }
}
