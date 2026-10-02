use std::path::{Path, PathBuf};

use apollia_signing::{sha256, DigestList, SignatureFile, SigningKeyPair};

use super::*;

const FULL_POLICY: &str = include_str!("../../tests/fixtures/org_policy/full.toml");
const MINIMAL_POLICY: &str = "schema_version = 1\nsigned_at = 2026-10-02T07:30:00Z\n";

fn org_key() -> SigningKeyPair {
    SigningKeyPair::from_seed(&[7u8; 32])
}

fn key_hex(pair: &SigningKeyPair) -> String {
    pair.public_key()
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn signature_for(policy: &[u8], pair: &SigningKeyPair) -> String {
    let mut list = DigestList::new();
    list.insert(POLICY_FILE, sha256(policy)).unwrap();
    SignatureFile::create(&list, "Acme IT", pair)
        .unwrap()
        .render()
}

/// Writes a signed policy folder under `root` and returns its path.
fn write_policy_dir(root: &Path, policy: &str) -> PathBuf {
    let dir = root.join("policy");
    std::fs::create_dir(&dir).unwrap();
    restrict(&dir, 0o755);
    let pair = org_key();
    std::fs::write(dir.join(POLICY_FILE), policy).unwrap();
    std::fs::write(
        dir.join(SIGNATURE_FILE),
        signature_for(policy.as_bytes(), &pair),
    )
    .unwrap();
    std::fs::write(dir.join(ORG_KEY_FILE), format!("{}\n", key_hex(&pair))).unwrap();
    for name in [POLICY_FILE, SIGNATURE_FILE, ORG_KEY_FILE] {
        restrict(&dir.join(name), 0o644);
    }
    dir
}

#[cfg(unix)]
fn restrict(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[cfg(not(unix))]
fn restrict(_path: &Path, _mode: u32) {}

fn load(dir: &Path) -> PolicyVerdict {
    load_policy(dir, OwnerRequirement::CurrentUser)
}

fn reason_of(verdict: &PolicyVerdict) -> Option<PolicyInvalidReason> {
    match verdict {
        PolicyVerdict::Invalid(invalid) => Some(invalid.reason),
        _ => None,
    }
}

// ---- Criterion: verdicts and system paths ----

#[test]
fn test_missing_folder_is_absent() {
    // GIVEN a path where no policy folder exists
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("policy");

    // WHEN it is read with either requirement
    let as_user = load(&dir);
    let as_system = load_policy(&dir, OwnerRequirement::System);

    // THEN there is no policy
    assert_eq!(as_user, PolicyVerdict::Absent);
    assert_eq!(as_system, PolicyVerdict::Absent);
}

#[test]
fn test_signed_policy_round_trip_is_valid() {
    // GIVEN a folder signed by the organisation key
    let root = tempfile::tempdir().unwrap();
    let dir = write_policy_dir(root.path(), FULL_POLICY);

    // WHEN it is read
    let verdict = load(&dir);

    // THEN it is valid, with the key fingerprint, the publisher and the signed date
    let valid = match verdict {
        PolicyVerdict::Valid(valid) => valid,
        other => return assert_eq!(other.kind(), "valid", "{other:?}"),
    };
    assert_eq!(valid.key_fingerprint, org_key().public_key().fingerprint());
    assert_eq!(valid.publisher, "Acme IT");
    assert_eq!(valid.signed_at.to_string(), "2026-10-02T09:30:00+02:00");
    assert_eq!(valid.policy, parse_policy(FULL_POLICY).unwrap());
}

#[cfg(target_os = "macos")]
#[test]
fn test_system_folder_on_macos() {
    // GIVEN a macOS build
    // WHEN the system folder is read
    let dir = SYSTEM_POLICY_DIR;
    // THEN it is the machine-wide Application Support folder
    assert_eq!(dir, "/Library/Application Support/Apollia/policy");
}

#[cfg(windows)]
#[test]
fn test_system_folder_on_windows() {
    // GIVEN a Windows build
    // WHEN the system folder is read
    let dir = SYSTEM_POLICY_DIR;
    // THEN it is under ProgramData, written out
    assert_eq!(dir, r"C:\ProgramData\Apollia\policy");
}

#[cfg(all(not(target_os = "macos"), not(windows)))]
#[test]
fn test_system_folder_on_linux() {
    // GIVEN a Linux build
    // WHEN the system folder is read
    let dir = SYSTEM_POLICY_DIR;
    // THEN it is under /etc
    assert_eq!(dir, "/etc/apollia/policy");
}

#[test]
fn test_location_without_override_is_system() {
    // GIVEN a process where the override variable is not set
    if std::env::var_os(POLICY_DIR_OVERRIDE_ENV).is_some() {
        return;
    }

    // WHEN the location is asked
    let location = policy_location();

    // THEN it is the system folder with the system requirement
    assert_eq!(location.dir, PathBuf::from(SYSTEM_POLICY_DIR));
    assert_eq!(location.owner, OwnerRequirement::System);
}

// ---- Criterion: signature and key ----

#[test]
fn test_one_modified_byte_is_invalid_signature() {
    // GIVEN a signed folder whose policy then loses one byte to another value
    let root = tempfile::tempdir().unwrap();
    let dir = write_policy_dir(root.path(), MINIMAL_POLICY);
    let mut bytes = std::fs::read(dir.join(POLICY_FILE)).unwrap();
    bytes[0] ^= 0x01;
    std::fs::write(dir.join(POLICY_FILE), bytes).unwrap();

    // WHEN it is read
    let verdict = load(&dir);

    // THEN the signature is the reason
    assert_eq!(reason_of(&verdict), Some(PolicyInvalidReason::Signature));
}

#[test]
fn test_missing_signature_is_invalid_signature() {
    // GIVEN a folder without SIGNATURE
    let root = tempfile::tempdir().unwrap();
    let dir = write_policy_dir(root.path(), MINIMAL_POLICY);
    std::fs::remove_file(dir.join(SIGNATURE_FILE)).unwrap();

    // WHEN it is read
    let verdict = load(&dir);

    // THEN the signature is the reason
    assert_eq!(reason_of(&verdict), Some(PolicyInvalidReason::Signature));
}

#[test]
fn test_missing_org_key_is_invalid_key() {
    // GIVEN a folder without org.pub
    let root = tempfile::tempdir().unwrap();
    let dir = write_policy_dir(root.path(), MINIMAL_POLICY);
    std::fs::remove_file(dir.join(ORG_KEY_FILE)).unwrap();

    // WHEN it is read
    let verdict = load(&dir);

    // THEN the key is the reason
    assert_eq!(reason_of(&verdict), Some(PolicyInvalidReason::Key));
}

#[test]
fn test_unreadable_org_key_is_invalid_key() {
    // GIVEN a folder whose org.pub is not a key
    let root = tempfile::tempdir().unwrap();
    let dir = write_policy_dir(root.path(), MINIMAL_POLICY);
    std::fs::write(dir.join(ORG_KEY_FILE), "not a key\n").unwrap();

    // WHEN it is read
    let verdict = load(&dir);

    // THEN the key is the reason
    assert_eq!(reason_of(&verdict), Some(PolicyInvalidReason::Key));
}

#[test]
fn test_policy_signed_by_another_key_is_invalid_key() {
    // GIVEN a folder whose SIGNATURE was made by a key other than org.pub
    let root = tempfile::tempdir().unwrap();
    let dir = write_policy_dir(root.path(), MINIMAL_POLICY);
    let other = SigningKeyPair::from_seed(&[9u8; 32]);
    std::fs::write(
        dir.join(SIGNATURE_FILE),
        signature_for(MINIMAL_POLICY.as_bytes(), &other),
    )
    .unwrap();

    // WHEN it is read
    let verdict = load(&dir);

    // THEN the key is the reason
    assert_eq!(reason_of(&verdict), Some(PolicyInvalidReason::Key));
}

// ---- Criterion: rights ----

#[cfg(unix)]
#[test]
fn test_user_owned_folder_is_invalid_owner_for_system() {
    // GIVEN a valid folder owned by the current user, not by root
    use std::os::unix::fs::MetadataExt as _;
    let root = tempfile::tempdir().unwrap();
    let dir = write_policy_dir(root.path(), MINIMAL_POLICY);
    if std::fs::metadata(&dir).unwrap().uid() == 0 {
        return;
    }

    // WHEN it is read with the system requirement
    let verdict = load_policy(&dir, OwnerRequirement::System);

    // THEN the owner is the reason
    assert_eq!(reason_of(&verdict), Some(PolicyInvalidReason::Owner));
}

#[cfg(unix)]
#[test]
fn test_group_writable_file_is_invalid_owner() {
    // GIVEN a valid folder whose policy file its group can modify
    let root = tempfile::tempdir().unwrap();
    let dir = write_policy_dir(root.path(), MINIMAL_POLICY);
    restrict(&dir.join(POLICY_FILE), 0o664);

    // WHEN it is read
    let verdict = load(&dir);

    // THEN the owner is the reason
    assert_eq!(reason_of(&verdict), Some(PolicyInvalidReason::Owner));
}

#[cfg(unix)]
#[test]
fn test_world_writable_folder_is_invalid_owner() {
    // GIVEN a valid folder every user can modify
    let root = tempfile::tempdir().unwrap();
    let dir = write_policy_dir(root.path(), MINIMAL_POLICY);
    restrict(&dir, 0o757);

    // WHEN it is read
    let verdict = load(&dir);

    // THEN the owner is the reason
    assert_eq!(reason_of(&verdict), Some(PolicyInvalidReason::Owner));
}

#[cfg(unix)]
#[test]
fn test_symlinked_key_is_invalid_owner() {
    // GIVEN a valid folder whose org.pub is a symbolic link to a valid key
    let root = tempfile::tempdir().unwrap();
    let dir = write_policy_dir(root.path(), MINIMAL_POLICY);
    let elsewhere = root.path().join("key.pub");
    std::fs::rename(dir.join(ORG_KEY_FILE), &elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, dir.join(ORG_KEY_FILE)).unwrap();

    // WHEN it is read
    let verdict = load(&dir);

    // THEN the owner is the reason
    assert_eq!(reason_of(&verdict), Some(PolicyInvalidReason::Owner));
}

// ---- Criterion: schema ----

#[test]
fn test_full_fixture_round_trips() {
    // GIVEN the fixture that uses every key of the schema
    let policy = parse_policy(FULL_POLICY).unwrap();

    // WHEN it is written back to TOML and read again
    let rewritten = toml::to_string(&policy).unwrap();
    let reread = parse_policy(&rewritten).unwrap();

    // THEN nothing is lost, and every section carries its values
    assert_eq!(reread, policy);
    assert_eq!(policy.connections.remote, Some(DenyOnly::Deny));
    assert_eq!(
        policy.families.deny,
        vec![PolicyFamily::ExecuteCode, PolicyFamily::SendExternal]
    );
    assert!(policy.inference.require_model_hash);
    assert_eq!(policy.settings.len(), 3);
    assert_eq!(policy.packages.trusted_publishers.len(), 1);
    assert_eq!(policy.journal.retention_days, Some(365));
    assert_eq!(policy.journal.export.detail, Some(ExportDetail::Forbid));
}

#[test]
fn test_minimal_policy_imposes_nothing() {
    // GIVEN a policy that only declares its version
    // WHEN it is parsed
    let policy = parse_policy(MINIMAL_POLICY).unwrap();

    // THEN every section is empty
    assert!(policy.connections.allow.is_empty());
    assert!(policy.families.deny.is_empty());
    assert!(policy.settings.is_empty());
    assert!(!policy.journal.required);
}

#[test]
fn test_other_schema_version_is_refused() {
    // GIVEN a policy written for version 2
    let text = "schema_version = 2\n[connections]\nfuture = true\n";

    // WHEN it is parsed
    let result = parse_policy(text);

    // THEN the version is reported, not the unknown key
    assert_eq!(
        result,
        Err(PolicySchemaError::UnsupportedVersion { found: 2 })
    );
}

#[test]
fn test_missing_schema_version_is_refused() {
    // GIVEN a policy without schema_version
    let text = "[journal]\nrequired = true\n";

    // WHEN it is parsed
    let result = parse_policy(text);

    // THEN it is refused
    assert!(matches!(result, Err(PolicySchemaError::Invalid(_))));
}

#[test]
fn test_missing_signed_at_is_refused() {
    // GIVEN a policy that declares its version but no date of signature
    let text = "schema_version = 1\n";

    // WHEN it is parsed
    let result = parse_policy(text);

    // THEN it is refused
    assert!(matches!(result, Err(PolicySchemaError::Invalid(_))));
}

#[test]
fn test_signed_at_without_offset_is_refused() {
    // GIVEN a date of signature without offset, and one without time
    let local = "schema_version = 1\nsigned_at = 2026-10-02T09:30:00\n";
    let date_only = "schema_version = 1\nsigned_at = 2026-10-02\n";

    // WHEN each is parsed
    let local = parse_policy(local);
    let date_only = parse_policy(date_only);

    // THEN both are refused as incomplete
    assert!(matches!(
        local,
        Err(PolicySchemaError::IncompleteSignedAt { .. })
    ));
    assert!(matches!(
        date_only,
        Err(PolicySchemaError::IncompleteSignedAt { .. })
    ));
}

#[test]
fn test_changed_signed_at_breaks_the_signature() {
    // GIVEN a signed folder whose date of signature is then moved forward
    let root = tempfile::tempdir().unwrap();
    let dir = write_policy_dir(root.path(), MINIMAL_POLICY);
    let moved = MINIMAL_POLICY.replace("2026-10-02", "2027-10-02");
    std::fs::write(dir.join(POLICY_FILE), moved).unwrap();

    // WHEN it is read
    let verdict = load(&dir);

    // THEN the signature is the reason: the date is signed
    assert_eq!(reason_of(&verdict), Some(PolicyInvalidReason::Signature));
}

fn is_refused(section: &str) -> bool {
    let text = format!("{MINIMAL_POLICY}{section}");
    matches!(parse_policy(&text), Err(PolicySchemaError::Invalid(_)))
}

#[test]
fn test_unknown_top_level_key_is_refused() {
    // GIVEN an unknown key at the top level
    // WHEN it is parsed
    // THEN it is refused
    assert!(is_refused("comment = \"hello\"\n"));
}

#[test]
fn test_unknown_key_in_connections_is_refused() {
    // GIVEN [connections] with an unknown key, and with remote set to allow
    // WHEN each is parsed
    // THEN both are refused
    assert!(is_refused("[connections]\nallowed = [\"gmail\"]\n"));
    assert!(is_refused("[connections]\nremote = \"allow\"\n"));
}

#[test]
fn test_unknown_key_in_families_is_refused() {
    // GIVEN [families] with an unknown key, and with an unknown family
    // WHEN each is parsed
    // THEN both are refused
    assert!(is_refused("[families]\nallow = [\"read\"]\n"));
    assert!(is_refused("[families]\ndeny = [\"delete_everything\"]\n"));
}

#[test]
fn test_unknown_key_in_inference_is_refused() {
    // GIVEN [inference] with an unknown key
    // WHEN it is parsed
    // THEN it is refused
    assert!(is_refused("[inference]\nallowed_models = []\n"));
}

#[test]
fn test_settings_must_be_a_table() {
    // GIVEN [settings] given as a scalar, and as a table of free keys
    let free_keys = format!("{MINIMAL_POLICY}[settings]\n\"any.key\" = 3\n");

    // WHEN each is parsed
    let table = parse_policy(&free_keys);

    // THEN the scalar is refused and the free keys are kept
    assert!(is_refused("settings = 1\n"));
    assert_eq!(table.unwrap().settings.len(), 1);
}

#[test]
fn test_unknown_key_in_packages_is_refused() {
    // GIVEN [packages] with an unknown key, and with a publisher that is no fingerprint
    let bad_publisher = format!("{MINIMAL_POLICY}[packages]\ntrusted_publishers = [\"Acme\"]\n");

    // WHEN each is parsed
    let publisher = parse_policy(&bad_publisher);

    // THEN both are refused
    assert!(is_refused("[packages]\nrequire_signatures = true\n"));
    assert!(matches!(
        publisher,
        Err(PolicySchemaError::InvalidPublisherFingerprint { .. })
    ));
}

#[test]
fn test_unknown_key_in_journal_is_refused() {
    // GIVEN [journal] and [journal.export] each with an unknown key
    // WHEN each is parsed
    // THEN both are refused
    assert!(is_refused("[journal]\nerasable = false\n"));
    assert!(is_refused("[journal.export]\nformat = \"pdf\"\n"));
}

#[test]
fn test_signed_policy_with_bad_schema_is_invalid_schema() {
    // GIVEN a correctly signed folder whose policy has an unknown key
    let root = tempfile::tempdir().unwrap();
    let dir = write_policy_dir(root.path(), &format!("{MINIMAL_POLICY}extra = 1\n"));

    // WHEN it is read
    let verdict = load(&dir);

    // THEN the schema is the reason
    assert_eq!(reason_of(&verdict), Some(PolicyInvalidReason::Schema));
}

#[test]
fn test_missing_policy_file_is_invalid_schema() {
    // GIVEN a folder without policy.toml
    let root = tempfile::tempdir().unwrap();
    let dir = write_policy_dir(root.path(), MINIMAL_POLICY);
    std::fs::remove_file(dir.join(POLICY_FILE)).unwrap();

    // WHEN it is read
    let verdict = load(&dir);

    // THEN the schema is the reason
    assert_eq!(reason_of(&verdict), Some(PolicyInvalidReason::Schema));
}
