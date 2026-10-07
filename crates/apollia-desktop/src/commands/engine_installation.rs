//! Tauri IPC commands for the followed installation of an engine: the
//! accelerated engine the AI screen proposes, its import from a folder, and
//! its updates.
//!
//! Each command delegates to the runtime this window drives, embedded or
//! attached, and each answer is the state of the installation as the runtime
//! reports it: idle, verifying the signature, downloading, installing,
//! installed, or failed with its code. A refusal is a state and never a
//! rejected promise, so the window reads why an installation stopped from the
//! same place it reads its progress.

use serde_json::{json, Value};
use tauri::State;

use crate::attached::RuntimeLink;

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

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, header, method, path};
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

    #[test]
    fn test_the_engine_installation_commands_are_routed() {
        // GIVEN the commands of the followed installation
        let commands = [
            "get_engine_installation",
            "start_engine_installation",
            "import_engine_folder",
            "dismiss_engine_installation",
        ];

        // WHEN each passes the gate of an attached window
        // THEN none is refused
        for command in commands {
            assert!(gate(LinkMode::Attached, command).is_ok(), "{command}");
        }
    }
}
