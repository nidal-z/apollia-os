//! `POST /api/v1/agent-definitions/{name}/export`.

use std::sync::Arc;

use apollia_signing::{load_signing_key, AUTHOR_KEY_LABEL};
use apollia_tools::apo_archive::{ApoArchive, ArchiveLimits, SignatureState};
use apollia_tools::{AgentOrigin, StoredAgent};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::export::EXPORT_REPORT_HEADER;
use super::*;
use crate::api::routes_llm::tests::{test_app_state_no_llm, MockBackend};

/// The answer of an export: status, report header, body.
struct Answer {
    status: StatusCode,
    report: Option<Value>,
    disposition: Option<String>,
    body: Vec<u8>,
}

async fn export(state: AppState<MockBackend>, name: &str, body: Value) -> Answer {
    let app = agent_definition_routes::<MockBackend>().with_state(state);
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/agent-definitions/{name}/export"))
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let header = |name: &str| {
        resp.headers()
            .get(name)
            .map(|v| v.to_str().unwrap().to_owned())
    };
    let report = header(EXPORT_REPORT_HEADER).map(|v| serde_json::from_str(&v).unwrap());
    let disposition = header("content-disposition");
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    Answer {
        status,
        report,
        disposition,
        body,
    }
}

/// A runtime over `dir` holding the agent `veille` created in the interface,
/// the package agent `invoices`, a file secret store, and `config` as its
/// `apollia.toml`.
fn state(dir: &tempfile::TempDir, config: &str) -> AppState<MockBackend> {
    let agents =
        AgentRepository::open(&apollia_core::paths::DataFile::Agents.path(dir.path())).unwrap();
    for (name, origin, package) in [
        ("veille", AgentOrigin::Interface, None),
        ("invoices", AgentOrigin::Package, Some("office".to_owned())),
    ] {
        agents
            .save_definition(&StoredAgent {
                name: name.into(),
                origin,
                package,
                version: "1".into(),
                definition: json!({
                    "id": name,
                    "name": "Veille",
                    "mission": "Watch the market.",
                    "defaults": {"access": ["web_search", "pkg:crm/lookup"]}
                }),
                enabled: true,
                installed_at: "2026-10-01T00:00:00Z".into(),
                updated_at: "2026-10-01T00:00:00Z".into(),
            })
            .unwrap();
    }
    let config_path = dir.path().join("apollia.toml");
    std::fs::write(&config_path, config).unwrap();
    let mut state = test_app_state_no_llm();
    state.data_dir = dir.path().to_path_buf();
    state.config_path = Some(config_path);
    let secrets = dir.path().join("secrets");
    state.secret_store = Arc::new(move || {
        apollia_auth::AgeFileSecretStore::new(secrets.clone(), "test-passphrase".into())
            .map(|s| Box::new(s) as Box<dyn apollia_auth::SecretStore>)
            .map_err(|e| e.to_string())
    });
    state
}

fn signature_of(bytes: &[u8]) -> SignatureState {
    let mut archive = ApoArchive::from_reader(
        std::io::Cursor::new(bytes.to_vec()),
        ArchiveLimits::default(),
    )
    .unwrap();
    archive.signature_state().unwrap()
}

#[tokio::test]
async fn test_first_signature_generates_the_author_key_and_names_the_publisher() {
    // GIVEN a runtime whose author name is set and whose secret store holds
    // no author key
    let dir = tempfile::tempdir().unwrap();
    let state = state(&dir, "[author]\nname = \"Ada Lovelace\"\n");

    // WHEN the agent is exported signed, twice
    let first = export(state.clone(), "veille", json!({ "sign": true })).await;
    let second = export(state.clone(), "veille", json!({ "sign": true })).await;

    // THEN a key was generated and stored in the secret store
    assert_eq!(first.status, StatusCode::OK);
    let store = (state.secret_store)().unwrap();
    let key = load_signing_key(store.as_ref(), AUTHOR_KEY_LABEL).unwrap();
    let fingerprint = key.public_key().fingerprint();
    // AND the publisher is the chosen name plus the fingerprint of that key,
    // the same on both exports
    for answer in [&first, &second] {
        assert_eq!(
            signature_of(&answer.body),
            SignatureState::Signed {
                publisher: "Ada Lovelace".into(),
                key_fingerprint: fingerprint.clone(),
            }
        );
        let report = answer.report.as_ref().unwrap();
        assert_eq!(report["publisher"], "Ada Lovelace");
        assert_eq!(report["signed"], true);
        assert_eq!(report["key_fingerprint"], json!(fingerprint));
    }
    // AND the file is offered under the agent's name and version, the access
    // into another package listed as removed
    assert_eq!(
        first.disposition.as_deref(),
        Some("attachment; filename=\"veille-1.0.0.apo\"")
    );
    assert_eq!(
        first.report.as_ref().unwrap()["removed_access"],
        json!(["pkg:crm/lookup"])
    );
}

#[tokio::test]
async fn test_unsigned_export_with_a_publisher_for_this_export_only() {
    // GIVEN a runtime with an author name set
    let dir = tempfile::tempdir().unwrap();
    let state = state(&dir, "[author]\nname = \"Ada\"\n");

    // WHEN the agent is exported unsigned under another name
    let answer = export(state.clone(), "veille", json!({ "publisher": "Łukasz" })).await;

    // THEN the archive is unsigned, published under the given name, and the
    // report travels in ASCII
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(signature_of(&answer.body), SignatureState::Unsigned);
    let report = answer.report.unwrap();
    assert_eq!(report["publisher"], "Łukasz");
    assert_eq!(report["signed"], false);
    assert_eq!(report["key_fingerprint"], Value::Null);
    // AND no author key was generated
    let store = (state.secret_store)().unwrap();
    assert!(load_signing_key(store.as_ref(), AUTHOR_KEY_LABEL).is_err());
}

#[tokio::test]
async fn test_export_refusals() {
    // GIVEN a runtime without an author name
    let dir = tempfile::tempdir().unwrap();
    let state = state(&dir, "");

    // WHEN an export names no publisher, a packaged agent, or no agent
    let nobody = export(state.clone(), "veille", json!({})).await;
    let packaged = export(state.clone(), "invoices", json!({ "publisher": "Ada" })).await;
    let missing = export(state, "ghost", json!({ "publisher": "Ada" })).await;

    // THEN each is refused with its status and no archive
    assert_eq!(nobody.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(packaged.status, StatusCode::CONFLICT);
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert!(nobody.report.is_none() && packaged.report.is_none());
}
