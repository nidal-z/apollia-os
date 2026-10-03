//! Tauri IPC command behind the export of an agent from its sheet.
//!
//! - `export_agent`: `POST /api/v1/agent-definitions/{name}/export`, then the
//!   archive written to the file the user chose in the save dialog.
//!
//! The runtime builds the archive and signs it with the author key of its
//! secret store, whether it runs in this process or as a daemon; this command
//! only writes the file. The answer is the report of the export: the file
//! written, the publisher, the key fingerprint when signed, the accesses left
//! out and the connections the package requires.

use std::path::Path;

use serde_json::{json, Value};
use tauri::State;

use crate::attached::{segment, RuntimeLink};

/// Header that carries the report of an export.
const EXPORT_REPORT_HEADER: &str = "x-apollia-export-report";

/// Route of the export of the agent `name`.
fn export_path(name: &str) -> String {
    format!("/api/v1/agent-definitions/{}/export", segment(name))
}

pub(crate) async fn export_on(
    link: &RuntimeLink,
    name: &str,
    sign: bool,
    publisher: Option<&str>,
    target: &Path,
) -> Result<Value, String> {
    let body = json!({ "sign": sign, "publisher": publisher });
    let (report, bytes) = link
        .post_for_file(&export_path(name), &body, EXPORT_REPORT_HEADER)
        .await?;
    std::fs::write(target, &bytes).map_err(|e| format!("{}: {e}", target.display()))?;
    let mut report = report
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or_else(|| json!({}));
    if let Some(object) = report.as_object_mut() {
        object.insert("path".into(), target.display().to_string().into());
    }
    tracing::event!(
        tracing::Level::INFO,
        agent = %name,
        signed = sign,
        bytes = bytes.len(),
        "agent.export.written"
    );
    Ok(report)
}

/// Exports the agent `name` as a `.apo` package written to `path`, signed when
/// `sign`, under `publisher` when given (else the `author.name` setting), and
/// answers the report of the export.
#[tauri::command]
pub async fn export_agent(
    link: State<'_, RuntimeLink>,
    name: String,
    sign: bool,
    publisher: Option<String>,
    path: String,
) -> Result<Value, String> {
    export_on(
        link.inner(),
        &name,
        sign,
        publisher.as_deref(),
        Path::new(&path),
    )
    .await
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::attached::{gate, AttachedClient, LinkMode};

    const TOKEN: &str = "token-of-this-installation";

    #[tokio::test]
    async fn test_the_archive_is_written_where_the_user_chose_with_its_report() {
        // GIVEN a daemon that exports `veille` signed
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/agent-definitions/veille/export"))
            .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
            .and(body_json(json!({ "sign": true, "publisher": null })))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/octet-stream")
                    .insert_header(
                        EXPORT_REPORT_HEADER,
                        r#"{"publisher":"Ada","signed":true,"key_fingerprint":"ab","removed_access":["pkg:crm/lookup"]}"#,
                    )
                    .set_body_bytes(b"PK-archive".to_vec()),
            )
            .expect(1)
            .mount(&server)
            .await;
        let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("veille.apo");

        // WHEN the sheet exports it to that file
        let report = export_on(&link, "veille", true, None, &target)
            .await
            .unwrap();

        // THEN the bytes are on disk and the report names the file
        assert_eq!(std::fs::read(&target).unwrap(), b"PK-archive");
        assert_eq!(report["publisher"], "Ada");
        assert_eq!(report["removed_access"], json!(["pkg:crm/lookup"]));
        assert_eq!(report["path"], json!(target.display().to_string()));
    }

    #[tokio::test]
    async fn test_a_refusal_writes_no_file() {
        // GIVEN a daemon that refuses the export for want of a publisher
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/agent-definitions/veille/export"))
            .respond_with(ResponseTemplate::new(422).set_body_json(json!({
                "error": "no publisher name: set `author.name` or give one for this export",
            })))
            .mount(&server)
            .await;
        let link = RuntimeLink::Attached(AttachedClient::new(server.uri(), TOKEN).unwrap());
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("veille.apo");

        // WHEN the sheet exports it
        let refused = export_on(&link, "veille", false, None, &target).await;

        // THEN the runtime's reason comes back and nothing is written
        assert!(refused.unwrap_err().contains("publisher"));
        assert!(!target.exists());
    }

    #[test]
    fn test_the_export_is_routed_to_a_daemon() {
        // GIVEN the export command
        // WHEN it passes the gate of an attached window
        // THEN it is not refused
        assert!(gate(LinkMode::Attached, "export_agent").is_ok());
    }
}
