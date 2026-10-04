//! The preview of a package: content, accesses, signature, connections, and
//! what an update adds. Shared helpers for the installation tests too.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use apollia_mcp::Provenance;
use apollia_tools::apo_archive::SigningKeyPair;
use apollia_tools::apo_archive::{build_from_dir, BuildOptions};
use apollia_tools::{
    AgentOrigin, AgentRepository, PackageRepository, StoredAgent, TrustedKeyOrigin,
    TrustedKeyRepository,
};
use tempfile::TempDir;

use super::*;
use crate::connections::{Connection, ConnectionKind, ConnectionState};
use crate::governance::{
    AccessCall, AccessDecision, DeclaredAccess, EffectiveAccess, GovernanceContract,
    GovernanceError, GovernanceEvent, InstallSubject,
};

/// Governance that resolves accesses as declared, answers the signature
/// question with `require_signature`, and keeps what it records.
#[derive(Default)]
pub(super) struct TestGovernance {
    pub(super) require_signature: bool,
    pub(super) recorded: Mutex<Vec<GovernanceEvent>>,
}

impl GovernanceContract for TestGovernance {
    fn effective_accesses(
        &self,
        _agent_id: &str,
        declared: &[DeclaredAccess],
    ) -> Vec<EffectiveAccess> {
        declared
            .iter()
            .map(|a| EffectiveAccess {
                access_id: a.access_id.clone(),
                permits: a.permits.clone(),
                sensitive: a.declared_sensitive,
            })
            .collect()
    }

    fn decide(&self, _call: &AccessCall<'_>) -> AccessDecision {
        AccessDecision::Allowed {
            reason: crate::governance::DecisionReason::NoConsentNeeded,
        }
    }

    fn signature_required(&self, _subject: &InstallSubject<'_>) -> bool {
        self.require_signature
    }

    fn record(&self, event: &GovernanceEvent) -> Result<(), GovernanceError> {
        self.recorded.lock().unwrap().push(event.clone());
        Ok(())
    }
}

/// A data directory with its stores.
pub(super) struct Env {
    pub(super) dir: TempDir,
    pub(super) packages: PackageRepository,
    pub(super) agents: AgentRepository,
    pub(super) keys: TrustedKeyRepository,
    pub(super) governance: TestGovernance,
    pub(super) connections: Vec<Connection>,
}

impl Env {
    pub(super) fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("agents.db");
        Self {
            packages: PackageRepository::open(&db).unwrap(),
            agents: AgentRepository::open(&db).unwrap(),
            keys: TrustedKeyRepository::open(&db).unwrap(),
            governance: TestGovernance::default(),
            connections: Vec::new(),
            dir,
        }
    }

    pub(super) fn sources(&self) -> PreviewSources<'_> {
        PreviewSources {
            governance: &self.governance,
            packages: &self.packages,
            agents: &self.agents,
            trusted_keys: &self.keys,
            connections: &self.connections,
            tools: &[],
        }
    }

    pub(super) fn data_dir(&self) -> &Path {
        self.dir.path()
    }
}

/// A connection of `kind` with identifier `id`.
pub(super) fn connection(id: &str, kind: ConnectionKind) -> Connection {
    Connection {
        id: id.to_owned(),
        kind,
        label: id.rsplit(':').next().unwrap_or(id).to_owned(),
        account: None,
        path: None,
        provenance: Provenance::Custom,
        remote: false,
        official: false,
        state: ConnectionState::Connected,
        tools: Vec::new(),
    }
}

/// The manifest of package `crm` at `version`: one agent granted the
/// package tool `lookup` and `extra` accesses, the tool itself, and
/// `extra_tables` appended as written.
pub(super) fn manifest(version: &str, extra: &[&str], extra_tables: &str) -> String {
    let mut access = vec!["\"pkg:crm/lookup\"".to_owned()];
    access.extend(extra.iter().map(|a| format!("\"{a}\"")));
    format!(
        r#"[package]
name = "crm"
version = "{version}"
publisher = "Acme"
description = "Leads and accounts."
sdk = ">=0.3"

[[agents]]
id = "crm-agent"
name = "CRM"
mission = "Track leads."
examples = ["Who called today?"]

[agents.defaults]
access = [{access}]

[[tools]]
name = "lookup"
module = "tools/lookup.py"
description = "Look up a lead by name."
action = "read"
inputs = {{ type = "object" }}

{extra_tables}"#,
        access = access.join(", ")
    )
}

/// Builds the archive of `manifest` under `dir`, signed with `key` when
/// given, and returns its path.
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
    std::fs::write(src.join("tools/extra_a.py"), "VALUE = 1\n").unwrap();
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

// ── Criterion 1: the dry-run preview ──────────────────────────────────────

#[test]
fn test_preview_reports_content_access_publisher_and_connections_and_writes_nothing() {
    // GIVEN an unsigned package that requires `gmail` and `notion`, brings
    // the MCP server `ledger`, and proposes a trigger, on a machine that has
    // a Google account and no Notion
    let mut env = Env::new();
    env.connections = vec![connection(
        "native:gmail:me@example.com",
        ConnectionKind::Native,
    )];
    let text = manifest(
        "1.0.0",
        &["web_search"],
        r#"[[workflows]]
name = "weekly"
module = "tools/lookup.py"
description = "Weekly digest."
action = "send_external"
inputs = { type = "object" }
steps = ["collect", "send"]

[[requires]]
type = "gmail"
reason = "Read the leads' mails."

[[requires]]
type = "notion"

[[mcp_servers]]
name = "ledger"
command = "uvx"
args = ["ledger-mcp"]

[[triggers]]
id = "monday"
target = "crm-agent"
source = { type = "cron", schedule = "0 8 * * MON" }
"#,
    );
    let apo = archive(env.data_dir(), "crm.apo", &text, None);
    let before: Vec<_> = std::fs::read_dir(env.data_dir()).unwrap().collect();

    // WHEN it is previewed
    let preview = preview(&apo, &env.sources()).unwrap();

    // THEN the content, the accesses with what they permit, the publisher,
    // the connections required and brought, and the trigger are reported
    assert_eq!(preview.name, "crm");
    assert_eq!(preview.version, "1.0.0");
    assert_eq!(preview.agents.len(), 1);
    let access = &preview.agents[0].access;
    assert_eq!(access[0].access_id, "pkg:crm/lookup");
    assert_eq!(access[0].permits, "Look up a lead by name.");
    assert!(!access[0].sensitive);
    assert_eq!(access[1].access_id, "web_search");
    assert_eq!(access[1].permits, "use web_search");
    assert!(access[1].sensitive);
    assert_eq!(preview.tools[0].name, "lookup");
    assert_eq!(preview.tools[0].family, "read");
    assert_eq!(preview.workflows[0].steps, vec!["collect", "send"]);
    assert!(preview.workflows[0].sensitive);
    assert_eq!(preview.signature.status, SignatureStatus::Unsigned);
    assert_eq!(preview.signature.publisher, "Acme");
    assert!(preview.requires[0].present);
    assert!(!preview.requires[1].present);
    assert_eq!(preview.mcp_servers[0].name, "ledger");
    assert!(!preview.mcp_servers[0].already_present);
    assert_eq!(preview.triggers[0].source, "cron");
    assert!(preview.notices.contains(&PackageNotice::Unsigned));
    assert!(preview.notices.contains(&PackageNotice::MissingConnection {
        kind: "notion".into()
    }));
    assert_eq!(preview.installed_version, None);
    assert_eq!(preview.refusal, None);

    // AND nothing was written: no package, no agent, no new file
    assert!(env.packages.list().unwrap().is_empty());
    assert!(env.agents.stored("crm-agent").unwrap().is_none());
    let after: Vec<_> = std::fs::read_dir(env.data_dir()).unwrap().collect();
    assert_eq!(after.len(), before.len());
    assert!(env.governance.recorded.lock().unwrap().is_empty());
}

#[test]
fn test_preview_names_a_trusted_publisher_by_its_keyring_label() {
    // GIVEN a package signed by a key the user trusts as "Acme Corp"
    let env = Env::new();
    let key = SigningKeyPair::from_seed(&[7u8; 32]);
    env.keys
        .import(
            &key.public_key().to_hex(),
            "Acme Corp",
            TrustedKeyOrigin::User,
        )
        .unwrap();
    let apo = archive(
        env.data_dir(),
        "signed.apo",
        &manifest("1.0.0", &[], ""),
        Some(&key),
    );

    // WHEN it is previewed
    let preview = preview(&apo, &env.sources()).unwrap();

    // THEN it reads as signed, trusted, under the label of the keyring
    assert_eq!(preview.signature.status, SignatureStatus::Signed);
    assert!(preview.signature.trusted);
    assert_eq!(preview.signature.publisher, "Acme Corp");
    assert_eq!(preview.signature.declared_publisher, "Acme");
    assert_eq!(
        preview.signature.key_fingerprint,
        Some(key.public_key().fingerprint())
    );
    assert!(preview.notices.is_empty());
}

#[test]
fn test_preview_of_an_unknown_key_shows_the_declared_name_unverified() {
    // GIVEN a package signed by a key absent from the keyring
    let env = Env::new();
    let key = SigningKeyPair::from_seed(&[8u8; 32]);
    let apo = archive(
        env.data_dir(),
        "signed.apo",
        &manifest("1.0.0", &[], ""),
        Some(&key),
    );

    // WHEN it is previewed
    let preview = preview(&apo, &env.sources()).unwrap();

    // THEN it is signed but not trusted, named as declared, with a notice
    assert_eq!(preview.signature.status, SignatureStatus::Signed);
    assert!(!preview.signature.trusted);
    assert_eq!(preview.signature.publisher, "Acme");
    assert_eq!(
        preview.notices,
        vec![PackageNotice::UntrustedPublisher {
            fingerprint: key.public_key().fingerprint()
        }]
    );
}

#[test]
fn test_preview_of_an_altered_package_reports_the_invalid_signature() {
    // GIVEN a signed package one entry of which was renamed after signing
    let env = Env::new();
    let key = SigningKeyPair::from_seed(&[9u8; 32]);
    let signed = archive(
        env.data_dir(),
        "a.apo",
        &manifest("1.0.0", &[], ""),
        Some(&key),
    );
    let altered = env.data_dir().join("altered.apo");
    tamper(&signed, &altered);

    // WHEN it is previewed
    let preview = preview(&altered, &env.sources()).unwrap();

    // THEN the signature reads as invalid and installing it would be refused
    assert_eq!(preview.signature.status, SignatureStatus::Invalid);
    assert!(matches!(
        preview.refusal,
        Some(InstallRefusal::InvalidSignature { .. })
    ));
}

/// Copies the archive `from` into `to` with the entry `tools/extra_a.py`
/// renamed `tools/extra_b.py`. The name is rewritten in the local header and
/// in the central directory alike, so the archive still reads; the digest
/// list now names another path, so a signature over it no longer holds.
pub(super) fn tamper(from: &Path, to: &Path) {
    let bytes = std::fs::read(from).unwrap();
    let (old, new) = (b"tools/extra_a.py", b"tools/extra_b.py");
    let mut out = bytes.clone();
    let mut replaced = 0;
    let mut i = 0;
    while i + old.len() <= out.len() {
        if &out[i..i + old.len()] == old {
            out[i..i + old.len()].copy_from_slice(new);
            replaced += 1;
            i += old.len();
        } else {
            i += 1;
        }
    }
    assert_eq!(
        replaced, 2,
        "the name sits in the local header and the directory"
    );
    std::fs::write(to, out).unwrap();
}

#[test]
fn test_preview_refuses_an_unsigned_package_when_governance_requires_a_signature() {
    // GIVEN governance that requires a signature, and an unsigned package
    let mut env = Env::new();
    env.governance.require_signature = true;
    let apo = archive(env.data_dir(), "u.apo", &manifest("1.0.0", &[], ""), None);

    // WHEN it is previewed
    let preview = preview(&apo, &env.sources()).unwrap();

    // THEN the preview says a signature is required and sets the refusal
    assert!(preview.signature_required);
    assert_eq!(preview.refusal, Some(InstallRefusal::SignatureRequired));
}

#[test]
fn test_preview_refuses_an_agent_identifier_another_package_owns() {
    // GIVEN an agent `crm-agent` stored by another package
    let env = Env::new();
    env.agents
        .save_definition(&StoredAgent {
            name: "crm-agent".into(),
            origin: AgentOrigin::Package,
            package: Some("other".into()),
            version: "1.0.0".into(),
            definition: serde_json::json!({ "id": "crm-agent" }),
            enabled: true,
            installed_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        })
        .unwrap();
    let apo = archive(env.data_dir(), "c.apo", &manifest("1.0.0", &[], ""), None);

    // WHEN the package is previewed
    let preview = preview(&apo, &env.sources()).unwrap();

    // THEN installing it would be refused, naming the owner
    assert_eq!(
        preview.refusal,
        Some(InstallRefusal::AgentTaken {
            agent: "crm-agent".into(),
            owner: Some("other".into())
        })
    );
}

#[test]
fn test_preview_of_a_file_that_is_not_a_package_is_an_error() {
    // GIVEN a file that is not a zip archive
    let env = Env::new();
    let path = env.data_dir().join("junk.apo");
    std::fs::write(&path, b"not a zip").unwrap();

    // WHEN it is previewed
    let result = preview(&path, &env.sources());

    // THEN it is refused as an archive error
    assert!(matches!(result, Err(PackageError::Archive(_))));
}

#[test]
fn test_connection_type_is_the_segment_after_the_kind() {
    // GIVEN a native account, an MCP server and a mailbox
    let google = connection("native:google:me@example.com", ConnectionKind::Native);
    let notion = connection("mcp:notion", ConnectionKind::Mcp);
    let mailbox = connection("imap:me@example.com", ConnectionKind::Imap);

    // WHEN each is matched against package connection types
    // THEN the provider, the server name and `imap` match, case aside
    assert!(connection_matches(&google, "Google"));
    assert!(connection_matches(&notion, "notion"));
    assert!(connection_matches(&mailbox, "imap"));
    assert!(!connection_matches(&google, "notion"));
}
