//! Tauri IPC commands for the followed installation of an engine: the
//! accelerated engine the AI screen proposes, any variant the Engines page
//! installs, an import from a folder, the updates, and the removal of an
//! installed build.
//!
//! Each command delegates to the runtime this window drives, embedded or
//! attached, and each answer is the state of the installation as the runtime
//! reports it: idle, verifying the signature, downloading, installing,
//! installed, or failed with its code. A refusal is a state and never a
//! rejected promise, so the window reads why an installation stopped from the
//! same place it reads its progress.

use serde::Serialize;
use serde_json::{json, Value};
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// Route of the followed installation on the runtime.
pub(crate) const INSTALLATION_ROUTE: &str = "/api/v1/engines/installation";

/// Route that starts an import from a directory on the runtime.
pub(crate) const IMPORT_ROUTE: &str = "/api/v1/engines/installation/import";

/// Where the installation of an engine stands, over `GET
/// /api/v1/engines/installation`. Read by the AI screen while one runs.
#[tauri::command]
pub async fn get_engine_installation(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    link.get_json(INSTALLATION_ROUTE).await
}

/// Asks the runtime to download, verify and install the newest build of one
/// engine variant on the followed channel.
pub(crate) async fn start_engine_installation_on(
    link: &RuntimeLink,
    engine: &str,
    variant: &str,
) -> Result<Value, String> {
    link.post_json(
        INSTALLATION_ROUTE,
        &json!({ "engine": engine, "variant": variant }),
    )
    .await
}

/// Starts the verified download of an engine variant, over `POST
/// /api/v1/engines/installation`, and answers at once with the state: the
/// signature is verified before any archive is requested.
#[tauri::command]
pub async fn start_engine_installation(
    link: State<'_, RuntimeLink>,
    engine: String,
    variant: String,
) -> Result<Value, String> {
    start_engine_installation_on(link.inner(), &engine, &variant).await
}

/// Asks the runtime to import the engines of `path`, a directory of the
/// machine the runtime runs on.
pub(crate) async fn import_engine_folder_on(
    link: &RuntimeLink,
    path: &str,
) -> Result<Value, String> {
    link.post_json(IMPORT_ROUTE, &json!({ "path": path })).await
}

/// Starts the import of the engines of a folder (archive, manifest and
/// signature), over `POST /api/v1/engines/installation/import`, and answers
/// at once with the state. The same verification as a download applies.
#[tauri::command]
pub async fn import_engine_folder(
    link: State<'_, RuntimeLink>,
    path: String,
) -> Result<Value, String> {
    import_engine_folder_on(link.inner(), &path).await
}

/// Cancels the download of an engine, or forgets the outcome of the last
/// installation, over `DELETE /api/v1/engines/installation`.
#[tauri::command]
pub async fn dismiss_engine_installation(link: State<'_, RuntimeLink>) -> Result<Value, String> {
    link.delete_json(INSTALLATION_ROUTE).await
}

/// What asking the runtime to remove one installed build came to.
///
/// A refusal is an outcome the page words, not a rejected promise: the
/// runtime refuses the build in service (`in_service`), the engine embedded in
/// the application (`embedded`), the build `engines.gguf` names until the
/// removal is confirmed (`chosen`, with `after`), and a build the policy fixes
/// (`locked`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub(crate) enum EngineRemoval {
    /// The build is gone.
    Removed,
    /// The runtime kept the build.
    Refused {
        /// The HTTP status of the refusal.
        status: u16,
        /// The stable code of the route, `unknown` when the body named none.
        code: String,
        /// The runtime's sentence.
        error: String,
        /// With `chosen`: `auto`, or the tag of the build that takes the place.
        after: Option<String>,
    },
}

impl EngineRemoval {
    fn from_answer(status: u16, body: &Value) -> Self {
        if (200..300).contains(&status) {
            return Self::Removed;
        }
        let text = |key: &str| body.get(key).and_then(Value::as_str).map(str::to_owned);
        Self::Refused {
            status,
            code: text("code").unwrap_or_else(|| "unknown".to_owned()),
            error: text("error").unwrap_or_default(),
            after: text("after"),
        }
    }
}

/// The route that removes the build `tag` of `variant` of `engine`, with the
/// confirmation a chosen build asks for.
pub(crate) fn removal_route(engine: &str, tag: &str, variant: &str, confirm: bool) -> String {
    let route = format!(
        "/api/v1/engines/{}/{}/{}",
        segment(engine),
        segment(tag),
        segment(variant)
    );
    if confirm {
        format!("{route}?confirm=true")
    } else {
        route
    }
}

/// Asks the runtime to remove one installed build.
pub(crate) async fn remove_engine_variant_on(
    link: &RuntimeLink,
    engine: &str,
    tag: &str,
    variant: &str,
    confirm: bool,
) -> Result<EngineRemoval, String> {
    let (status, body) = link
        .delete_answer(&removal_route(engine, tag, variant, confirm))
        .await?;
    Ok(EngineRemoval::from_answer(status, &body))
}

/// Removes one installed build of an engine, over `DELETE
/// /api/v1/engines/{engine}/{tag}/{variant}`; `confirm` lets the build
/// `engines.gguf` names go. A refusal comes back as an outcome with its code.
#[tauri::command]
pub async fn remove_engine_variant(
    link: State<'_, RuntimeLink>,
    engine: String,
    tag: String,
    variant: String,
    confirm: bool,
) -> Result<EngineRemoval, String> {
    remove_engine_variant_on(link.inner(), &engine, &tag, &variant, confirm).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::attached::{gate, AttachedClient, LinkMode};

    const TOKEN: &str = "token-of-this-installation";

    fn attached(server: &MockServer) -> RuntimeLink {
        RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap())
    }

    fn authorized(verb: &str, route: &str) -> wiremock::MockBuilder {
        Mock::given(method(verb))
            .and(path(route))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
    }

    #[tokio::test]
    async fn test_the_installation_is_started_followed_and_dismissed_on_the_daemon() {
        // GIVEN a daemon that starts an installation, reports its download,
        // and forgets it
        let server = MockServer::start().await;
        authorized("POST", INSTALLATION_ROUTE)
            .and(body_json(
                json!({ "engine": "llama-server", "variant": "cuda" }),
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "state": "verifying", "kind": "install" })),
            )
            .expect(1)
            .mount(&server)
            .await;
        authorized("GET", INSTALLATION_ROUTE)
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "state": "downloading", "kind": "install", "received": 80, "total": 412,
            })))
            .expect(1)
            .mount(&server)
            .await;
        authorized("DELETE", INSTALLATION_ROUTE)
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "state": "idle" })))
            .expect(1)
            .mount(&server)
            .await;
        let link = attached(&server);

        // WHEN the window starts the accelerated engine, reads the progress,
        // then cancels
        let started = start_engine_installation_on(&link, "llama-server", "cuda").await;
        let progress = link.get_json(INSTALLATION_ROUTE).await;
        let cancelled = link.delete_json(INSTALLATION_ROUTE).await;

        // THEN each answer is the daemon's state
        assert_eq!(started.unwrap()["state"], "verifying");
        assert_eq!(progress.unwrap()["received"], 80);
        assert_eq!(cancelled.unwrap()["state"], "idle");
    }

    #[tokio::test]
    async fn test_a_folder_is_imported_by_the_daemon_and_its_refusal_is_a_state() {
        // GIVEN a daemon that refuses a folder without signature
        let server = MockServer::start().await;
        authorized("POST", IMPORT_ROUTE)
            .and(body_json(json!({ "path": "D:\\Apollia\\engine" })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "state": "failed", "kind": "import", "code": "signature_missing",
                "error": "no signature beside the engine manifest",
                "archive_requested": false,
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the window imports that folder
        let outcome = import_engine_folder_on(&attached(&server), "D:\\Apollia\\engine").await;

        // THEN the refusal comes back as a state with its code, not as an error
        let state = outcome.unwrap();
        assert_eq!(state["state"], "failed");
        assert_eq!(state["code"], "signature_missing");
    }

    #[tokio::test]
    async fn test_a_removed_build_answers_removed_and_a_chosen_one_what_the_choice_becomes() {
        // GIVEN a daemon that removes the build b7100 of cuda-12 once confirmed
        // and refuses it unconfirmed, as the build engines.gguf names
        let server = MockServer::start().await;
        let route = "/api/v1/engines/llama-server/b7100/cuda-12";
        authorized("DELETE", route)
            .and(query_param("confirm", "true"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        authorized("DELETE", route)
            .respond_with(ResponseTemplate::new(409).set_body_json(json!({
                "error": "remove it with confirm=true and the choice goes back to auto",
                "code": "chosen",
                "after": "auto",
            })))
            .expect(1)
            .mount(&server)
            .await;
        let link = attached(&server);

        // WHEN the window asks without, then with, the confirmation
        let unconfirmed =
            remove_engine_variant_on(&link, "llama-server", "b7100", "cuda-12", false).await;
        let confirmed =
            remove_engine_variant_on(&link, "llama-server", "b7100", "cuda-12", true).await;

        // THEN the refusal carries its code and what the choice becomes, and the
        // confirmed removal is done
        assert_eq!(
            unconfirmed.unwrap(),
            EngineRemoval::Refused {
                status: 409,
                code: "chosen".to_owned(),
                error: "remove it with confirm=true and the choice goes back to auto".to_owned(),
                after: Some("auto".to_owned()),
            }
        );
        assert_eq!(confirmed.unwrap(), EngineRemoval::Removed);
    }

    #[tokio::test]
    async fn test_a_build_in_service_is_refused_as_an_outcome_not_an_error() {
        // GIVEN a daemon that runs the build b7100 of vulkan
        let server = MockServer::start().await;
        authorized("DELETE", "/api/v1/engines/llama-server/b7100/vulkan")
            .and(query_param("confirm", "true"))
            .respond_with(ResponseTemplate::new(409).set_body_json(json!({
                "error": "the build is in service",
                "code": "in_service",
            })))
            .expect(1)
            .mount(&server)
            .await;

        // WHEN the window confirms its removal
        let outcome =
            remove_engine_variant_on(&attached(&server), "llama-server", "b7100", "vulkan", true)
                .await;

        // THEN the refusal is an outcome with its code, its sentence and no `after`
        let outcome = outcome.unwrap();
        assert_eq!(
            outcome,
            EngineRemoval::Refused {
                status: 409,
                code: "in_service".to_owned(),
                error: "the build is in service".to_owned(),
                after: None,
            }
        );
        let wire = serde_json::to_value(&outcome).unwrap();
        assert_eq!(wire["outcome"], "refused");
        assert_eq!(wire["code"], "in_service");
    }

    #[test]
    fn test_the_removal_route_encodes_each_segment_and_carries_the_confirmation() {
        // GIVEN a tag with a character a path cannot carry as is
        // WHEN the route is built with and without the confirmation
        let plain = removal_route("llama-server", "b 1", "cpu", false);
        let confirmed = removal_route("llama-server", "b7100", "cpu", true);

        // THEN the segment is encoded and only the confirmed route asks for it
        assert_eq!(plain, "/api/v1/engines/llama-server/b%201/cpu");
        assert_eq!(
            confirmed,
            "/api/v1/engines/llama-server/b7100/cpu?confirm=true"
        );
    }

    #[test]
    fn test_the_engine_installation_commands_are_routed() {
        // GIVEN the commands of the followed installation
        let commands = [
            "get_engine_installation",
            "start_engine_installation",
            "import_engine_folder",
            "dismiss_engine_installation",
            "remove_engine_variant",
        ];

        // WHEN each passes the gate of an attached window
        // THEN none is refused
        for command in commands {
            assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
        }
    }
}
