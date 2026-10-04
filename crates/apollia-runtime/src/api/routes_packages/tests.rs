//! The package routes through the whole router.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use apollia_tools::apo_archive::{build_from_dir, BuildOptions, SigningKeyPair};
use apollia_tools::{AgentRepository, PackageRepository};
use axum::http::StatusCode;
use serde_json::json;
use tempfile::TempDir;

use crate::api::server::AppState;
use crate::api::test_support::{app_state, call, MockBackend, StemLoader};
use crate::audit_journal::{AuditJournalHandle, JournalEntryKind};
use crate::governance::{governance_run_id, package_run_id};

const KEY: &[u8] = b"package-routes-journal-key-0001";

pub(super) const MANIFEST: &str = r#"[package]
name = "crm"
version = "1.0.0"
publisher = "Acme"
sdk = ">=0.3"

[[agents]]
id = "crm-agent"
name = "CRM"
mission = "Track leads."

[agents.defaults]
access = ["pkg:crm/lookup"]

[[tools]]
name = "lookup"
module = "tools/lookup.py"
description = "Look up a lead by name."
action = "read"
inputs = { type = "object" }
"#;

/// Builds the archive of `manifest` into `dir/file`, signed with `key`.
pub(super) fn archive(
    dir: &Path,
    file: &str,
    manifest: &str,
    key: Option<&SigningKeyPair>,
) -> PathBuf {
    let src = dir.join(format!("src-{file}"));
    std::fs::create_dir_all(src.join("tools")).unwrap();
    std::fs::write(src.join("apollia.toml"), manifest).unwrap();
    std::fs::write(
        src.join("tools/lookup.py"),
        "@tool(name='lookup')\nasync def lookup(ctx, inputs):\n    return inputs\n",
    )
    .unwrap();
    let out = dir.join(file);
    build_from_dir(
        &src,
        &out,
        &BuildOptions {
            signing_key: key,
            ..BuildOptions::default()
        },
    )
    .unwrap();
    out
}

/// A runtime over `dir`, with a signed audit journal when `journal` is set.
pub(super) async fn state(
    dir: &TempDir,
    journal: bool,
) -> (AppState<MockBackend>, Option<AuditJournalHandle>) {
    let mut state = app_state(
        Arc::new(StemLoader { refusal: None }),
        dir.path().to_path_buf(),
    );
    let handle = if journal {
        let handle =
            AuditJournalHandle::open_with_key_bytes(&dir.path().join("journal.db"), KEY.to_vec())
                .await
                .unwrap();
        state.audit_journal = Some(handle.clone());
        Some(handle)
    } else {
        None
    };
    (state, handle)
}

pub(super) fn agents_db(dir: &TempDir) -> PathBuf {
    apollia_core::paths::DataFile::Agents.path(dir.path())
}

#[tokio::test]
async fn test_preview_route_reports_the_package_and_writes_nothing() {
    // GIVEN a runtime with a journal and an unsigned package
    let dir = TempDir::new().unwrap();
    let (state, _) = state(&dir, true).await;
    let apo = archive(dir.path(), "crm.apo", MANIFEST, None);

    // WHEN it is previewed
    let (status, body) = call(
        state,
        "POST",
        "/api/v1/packages/preview",
        Some(json!({ "path": apo })),
    )
    .await;

    // THEN the preview names its content, access and signature, and no
    // package is stored
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "crm");
    assert_eq!(body["agents"][0]["access"][0]["accessId"], "pkg:crm/lookup");
    assert_eq!(
        body["agents"][0]["access"][0]["permits"],
        "Look up a lead by name."
    );
    assert_eq!(body["signature"]["status"], "unsigned");
    assert_eq!(body["notices"][0]["code"], "unsigned");
    assert!(body["refusal"].is_null());
    let packages = PackageRepository::open(&agents_db(&dir)).unwrap();
    assert!(packages.list().unwrap().is_empty());
}

#[tokio::test]
async fn test_package_routes_refuse_without_a_journal() {
    // GIVEN a runtime without an audit journal
    let dir = TempDir::new().unwrap();
    let (state, _) = state(&dir, false).await;
    let apo = archive(dir.path(), "crm.apo", MANIFEST, None);

    // WHEN the package is installed
    let (status, body) = call(
        state,
        "POST",
        "/api/v1/packages/install",
        Some(json!({ "path": apo })),
    )
    .await;

    // THEN it is refused rather than left unrecorded
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "journal_unavailable");
}

#[tokio::test]
async fn test_install_route_installs_and_journals_the_package() {
    // GIVEN a runtime with a journal and a signed package
    let dir = TempDir::new().unwrap();
    let (state, journal) = state(&dir, true).await;
    let key = SigningKeyPair::from_seed(&[3u8; 32]);
    let apo = archive(dir.path(), "crm.apo", MANIFEST, Some(&key));

    // WHEN it is installed
    let (status, body) = call(
        state,
        "POST",
        "/api/v1/packages/install",
        Some(json!({ "path": apo })),
    )
    .await;

    // THEN it answers 201 with what was installed, the agent is stored, and
    // the journal holds the package and the agent entries
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["version"], "1.0.0");
    assert_eq!(body["agents"], json!(["crm-agent"]));
    assert_eq!(body["signature"]["status"], "signed");
    let agents = AgentRepository::open(&agents_db(&dir)).unwrap();
    assert!(agents.stored("crm-agent").unwrap().is_some());
    let journal = journal.unwrap();
    let package_kinds: Vec<JournalEntryKind> = journal
        .query_run(&package_run_id("crm"))
        .await
        .into_iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(package_kinds, vec![JournalEntryKind::PackageInstalled]);
    let agent_kinds: Vec<JournalEntryKind> = journal
        .query_run(&governance_run_id("crm-agent"))
        .await
        .into_iter()
        .map(|e| e.kind)
        .collect();
    assert_eq!(agent_kinds, vec![JournalEntryKind::AgentInstalled]);
}

#[tokio::test]
async fn test_install_route_maps_refusals_to_codes() {
    // GIVEN version 2.0.0 installed, then 1.0.0 offered, and a package that
    // brings an MCP server in a runtime without MCP
    let dir = TempDir::new().unwrap();
    let (state, _) = state(&dir, true).await;
    let v2 = archive(
        dir.path(),
        "v2.apo",
        &MANIFEST.replace("1.0.0", "2.0.0"),
        None,
    );
    let (installed, _) = call(
        state.clone(),
        "POST",
        "/api/v1/packages/install",
        Some(json!({ "path": v2 })),
    )
    .await;
    let v1 = archive(dir.path(), "v1.apo", MANIFEST, None);
    let other = archive(
        dir.path(),
        "other.apo",
        &(MANIFEST
            .replace("name = \"crm\"", "name = \"other\"")
            .replace("crm-agent", "other-agent")
            .replace("pkg:crm/", "pkg:other/")
            + "\n[[mcp_servers]]\nname = \"ledger\"\ncommand = \"uvx\"\n"),
        None,
    );

    // WHEN the older version, then the package with a server, are installed
    let (downgrade, downgrade_body) = call(
        state.clone(),
        "POST",
        "/api/v1/packages/install",
        Some(json!({ "path": v1 })),
    )
    .await;
    let (no_mcp, no_mcp_body) = call(
        state,
        "POST",
        "/api/v1/packages/install",
        Some(json!({ "path": other })),
    )
    .await;

    // THEN each refusal carries its status and its code
    assert_eq!(installed, StatusCode::CREATED);
    assert_eq!(downgrade, StatusCode::CONFLICT);
    assert_eq!(downgrade_body["code"], "downgrade");
    assert_eq!(no_mcp, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(no_mcp_body["code"], "mcp_unavailable");
}

#[tokio::test]
async fn test_preview_route_refuses_a_file_that_is_not_a_package() {
    // GIVEN a file that is not an archive
    let dir = TempDir::new().unwrap();
    let (state, _) = state(&dir, true).await;
    let junk = dir.path().join("junk.apo");
    std::fs::write(&junk, b"not a zip").unwrap();

    // WHEN it is previewed
    let (status, body) = call(
        state,
        "POST",
        "/api/v1/packages/preview",
        Some(json!({ "path": junk })),
    )
    .await;

    // THEN it is refused as an archive
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "archive");
}

// ── Criterion 4: the keyring ──────────────────────────────────────────────

#[tokio::test]
async fn test_trusted_keys_import_list_remove_and_trust_the_publisher() {
    // GIVEN a runtime with a journal, and a package signed by a key
    let dir = TempDir::new().unwrap();
    let (state, _) = state(&dir, true).await;
    let key = SigningKeyPair::from_seed(&[6u8; 32]);
    let fingerprint = key.public_key().fingerprint();
    let apo = archive(dir.path(), "crm.apo", MANIFEST, Some(&key));

    // WHEN the key is imported, listed, the package previewed, the key
    // removed twice, and a text that is no key imported
    let (imported, imported_body) = call(
        state.clone(),
        "POST",
        "/api/v1/packages/trusted-keys",
        Some(json!({ "publicKey": key.public_key().to_hex(), "label": "Acme Corp" })),
    )
    .await;
    let (_, listed) = call(state.clone(), "GET", "/api/v1/packages/trusted-keys", None).await;
    let (_, preview) = call(
        state.clone(),
        "POST",
        "/api/v1/packages/preview",
        Some(json!({ "path": apo })),
    )
    .await;
    let uri = format!("/api/v1/packages/trusted-keys/{fingerprint}");
    let (removed, _) = call(state.clone(), "DELETE", &uri, None).await;
    let (removed_again, _) = call(state.clone(), "DELETE", &uri, None).await;
    let (invalid, invalid_body) = call(
        state,
        "POST",
        "/api/v1/packages/trusted-keys",
        Some(json!({ "publicKey": "nope", "label": "X" })),
    )
    .await;

    // THEN the key is stored under its fingerprint, the publisher is shown
    // trusted under its label, and the removal and the refusal answer as
    // they should
    assert_eq!(imported, StatusCode::CREATED);
    assert_eq!(imported_body["fingerprint"], fingerprint);
    assert_eq!(imported_body["origin"], "user");
    assert_eq!(listed[0]["label"], "Acme Corp");
    assert_eq!(preview["signature"]["trusted"], true);
    assert_eq!(preview["signature"]["publisher"], "Acme Corp");
    assert_eq!(removed, StatusCode::OK);
    assert_eq!(removed_again, StatusCode::NOT_FOUND);
    assert_eq!(invalid, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(invalid_body["code"], "invalid_key");
}

#[test]
fn test_removing_a_key_the_policy_trusts_is_forbidden() {
    // GIVEN the keyring's refusal to remove a key the policy trusts
    let refusal = apollia_tools::TrustedKeyError::HeldByPolicy {
        fingerprint: "ab".repeat(32),
    };

    // WHEN the route answers it
    let (status, axum::Json(body)) = super::removal_error(refusal);

    // THEN it is forbidden, with its own code
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body.code, "policy_key");
}
