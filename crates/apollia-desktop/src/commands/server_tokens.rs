//! Tauri IPC commands for the tokens of the local server: the named secrets
//! another application presents when the server requires authentication.
//!
//! - [`list_server_tokens`]: `GET /api/v1/server/tokens`
//! - [`create_server_token`]: `POST /api/v1/server/tokens`
//! - [`revoke_server_token`]: `DELETE /api/v1/server/tokens/{id}`
//!
//! The tokens live in the runtime. The value of a token crosses this process
//! once, in the answer of its creation, and is held nowhere here: the runtime
//! itself keeps only its digest. The token this application uses to reach its
//! own runtime is another one, which none of these commands reads.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::attached::RuntimeLink;
use crate::commands::HttpFailure;

const TOKENS_PATH: &str = "/api/v1/server/tokens";

/// The status the runtime answers to a name it refuses.
const NAME_INVALID_STATUS: u16 = 400;
/// The status the runtime answers when a token in use carries the name.
const NAME_TAKEN_STATUS: u16 = 409;

/// A token as the window lists it: never its value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTokenView {
    /// Identifier, which revokes it.
    pub id: i64,
    /// Name given at creation.
    pub name: String,
    /// The last four characters of the value.
    pub last_four: String,
    /// When it was created.
    pub created_at: String,
    /// When it last opened the server; absent if never.
    #[serde(default)]
    pub last_used_at: Option<String>,
    /// Whether it was revoked.
    #[serde(default)]
    pub revoked: bool,
}

/// A token that was just created: its row and, this once, its value.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedServerTokenView {
    /// The token as the list shows it.
    #[serde(flatten)]
    pub token: ServerTokenView,
    /// The value, given this once.
    pub value: String,
}

/// Why a token was not created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTokenRefusal {
    /// `name_invalid` for a name the runtime refuses, `name_taken` when a
    /// token in use carries it, `other` otherwise.
    pub kind: &'static str,
    /// The runtime's own words.
    pub message: String,
}

fn parse<T: serde::de::DeserializeOwned>(json: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(json).map_err(|e| format!("invalid server token response: {e}"))
}

fn refusal(failure: HttpFailure) -> ServerTokenRefusal {
    let kind = match failure.status {
        Some(NAME_INVALID_STATUS) => "name_invalid",
        Some(NAME_TAKEN_STATUS) => "name_taken",
        _ => "other",
    };
    ServerTokenRefusal {
        kind,
        message: failure.message,
    }
}

/// Lists the tokens of the local server, in use and revoked, oldest first.
///
/// Delegates to `GET /api/v1/server/tokens`.
#[tauri::command]
pub async fn list_server_tokens(
    state: State<'_, RuntimeLink>,
) -> Result<Vec<ServerTokenView>, String> {
    parse(state.get_json(TOKENS_PATH).await?)
}

/// Creates a named token and answers its value, the only time it is given.
///
/// Delegates to `POST /api/v1/server/tokens`. Rejected with a
/// [`ServerTokenRefusal`] that tells a refused name and a name already in
/// use from every other failure.
#[tauri::command]
pub async fn create_server_token(
    state: State<'_, RuntimeLink>,
    name: String,
) -> Result<CreatedServerTokenView, ServerTokenRefusal> {
    let json = state
        .post_json_refusable(TOKENS_PATH, &serde_json::json!({ "name": name }))
        .await
        .map_err(refusal)?;
    parse(json).map_err(|message| ServerTokenRefusal {
        kind: "other",
        message,
    })
}

/// Revokes a token by its identifier. The token stops opening the server at
/// once; its row stays in the runtime, marked revoked.
///
/// Delegates to `DELETE /api/v1/server/tokens/{id}`.
#[tauri::command]
pub async fn revoke_server_token(
    state: State<'_, RuntimeLink>,
    id: i64,
) -> Result<ServerTokenView, String> {
    parse(state.delete_json(&format!("{TOKENS_PATH}/{id}")).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_listed_token_reads_its_name_end_and_dates_and_no_value() {
        // GIVEN the list the runtime answers, one token used and one revoked
        let json = serde_json::json!([
            {
                "id": 1,
                "name": "crm",
                "lastFour": "a3f9",
                "createdAt": "2026-09-28T10:00:00Z",
                "lastUsedAt": "2026-10-01T08:00:00Z",
                "revoked": false,
                "revokedAt": null
            },
            {
                "id": 2,
                "name": "old",
                "lastFour": "71c2",
                "createdAt": "2026-09-14T10:00:00Z",
                "lastUsedAt": null,
                "revoked": true,
                "revokedAt": "2026-09-20T10:00:00Z"
            }
        ]);

        // WHEN the shell reads it and sends it to the window
        let tokens: Vec<ServerTokenView> = parse(json).unwrap();
        let sent = serde_json::to_value(&tokens).unwrap();

        // THEN each row carries its name, its last four characters and its dates
        assert_eq!(tokens[0].name, "crm");
        assert_eq!(tokens[0].last_four, "a3f9");
        assert_eq!(
            tokens[0].last_used_at.as_deref(),
            Some("2026-10-01T08:00:00Z")
        );
        assert!(tokens[1].revoked);
        assert_eq!(tokens[1].last_used_at, None);
        // AND no row has a field for a value
        assert!(sent[0].get("value").is_none());
        assert_eq!(sent[0]["lastFour"], "a3f9");
    }

    #[test]
    fn test_a_created_token_carries_its_row_and_its_value_once() {
        // GIVEN the answer of a creation, the row flattened beside the value
        let json = serde_json::json!({
            "id": 3,
            "name": "script",
            "lastFour": "1e8c",
            "createdAt": "2026-10-07T09:00:00Z",
            "lastUsedAt": null,
            "revoked": false,
            "revokedAt": null,
            "value": "apo_0000000000000000000000000000000000000000000000000000000000001e8c"
        });

        // WHEN the shell reads it
        let created: CreatedServerTokenView = parse(json).unwrap();

        // THEN the value is there, with the row the list will show
        assert!(created.value.starts_with("apo_"));
        assert_eq!(created.token.id, 3);
        assert_eq!(created.token.last_four, "1e8c");
    }

    #[test]
    fn test_a_refused_creation_tells_the_name_refused_from_the_name_taken() {
        // GIVEN the three refusals the runtime answers to a creation
        let failure = |status| HttpFailure {
            status,
            message: "refused".to_owned(),
        };

        // WHEN each is turned into what the window receives
        let invalid = refusal(failure(Some(400)));
        let taken = refusal(failure(Some(409)));
        let other = refusal(failure(None));

        // THEN each has its own kind and keeps the runtime's words
        assert_eq!(invalid.kind, "name_invalid");
        assert_eq!(taken.kind, "name_taken");
        assert_eq!(other.kind, "other");
        assert_eq!(taken.message, "refused");
    }
}
