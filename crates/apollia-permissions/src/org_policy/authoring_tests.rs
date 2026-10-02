use apollia_signing::SigningKeyPair;

use super::*;

const SIGNED_AT: &str = "2026-10-02T09:30:00Z";

fn org_key() -> SigningKeyPair {
    SigningKeyPair::from_seed(&[9u8; 32])
}

#[test]
fn test_policy_template_is_valid_in_the_schema() {
    // GIVEN the template dated with a full date, time and offset
    let text = policy_template(SIGNED_AT);
    // WHEN it is parsed in the schema
    let policy = parse_policy(&text).unwrap();
    // THEN it is accepted and imposes nothing
    assert_eq!(policy.schema_version, POLICY_SCHEMA_VERSION);
    assert!(policy.connections.allow.is_empty());
    assert!(policy.families.deny.is_empty());
    assert!(!policy.packages.require_signature);
    assert!(!policy.journal.required);
}

#[test]
fn test_init_sign_then_verify_holds() {
    // GIVEN a folder written from the template and signed with the org key
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("policy");
    write_policy_template(&dir, SIGNED_AT).unwrap();
    let signed = sign_policy_dir(&dir, &org_key(), "Acme IT").unwrap();
    // WHEN the folder is verified without its rights
    let verdict = verify_policy_dir(&dir);
    // THEN it holds, for the key that signed, and org.pub carries that key
    let PolicyVerdict::Valid(valid) = verdict else {
        panic!("expected a valid policy, got {verdict:?}");
    };
    assert_eq!(valid.key_fingerprint, signed.key_fingerprint);
    assert_eq!(valid.publisher, "Acme IT");
    let org_pub = std::fs::read_to_string(dir.join(ORG_KEY_FILE)).unwrap();
    assert_eq!(org_pub, format!("{}\n", org_key().public_key().to_hex()));
}

#[test]
fn test_verify_refuses_a_policy_altered_by_one_byte() {
    // GIVEN a signed folder whose policy.toml then has one byte altered
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("policy");
    write_policy_template(&dir, SIGNED_AT).unwrap();
    sign_policy_dir(&dir, &org_key(), "Acme IT").unwrap();
    let path = dir.join(POLICY_FILE);
    let mut bytes = std::fs::read(&path).unwrap();
    let last = bytes.len() - 2;
    bytes[last] ^= 0x01;
    std::fs::write(&path, bytes).unwrap();
    // WHEN the folder is verified
    let verdict = verify_policy_dir(&dir);
    // THEN the signature is refused
    let PolicyVerdict::Invalid(invalid) = verdict else {
        panic!("expected a refusal, got {verdict:?}");
    };
    assert_eq!(invalid.reason, PolicyInvalidReason::Signature);
}

#[test]
fn test_sign_refuses_a_policy_that_breaks_the_schema() {
    // GIVEN a folder whose policy.toml carries an unknown key
    let root = tempfile::tempdir().unwrap();
    let dir = root.path();
    std::fs::write(
        dir.join(POLICY_FILE),
        format!("schema_version = 1\nsigned_at = {SIGNED_AT}\nunknown = 1\n"),
    )
    .unwrap();
    // WHEN it is signed
    let result = sign_policy_dir(dir, &org_key(), "Acme IT");
    // THEN signing is refused and nothing is written next to it
    assert!(matches!(result, Err(PolicyAuthoringError::Schema(_))));
    assert!(!dir.join(SIGNATURE_FILE).exists());
    assert!(!dir.join(ORG_KEY_FILE).exists());
}

#[test]
fn test_init_never_replaces_an_existing_policy() {
    // GIVEN a folder that already holds a policy.toml
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(POLICY_FILE), "keep me").unwrap();
    // WHEN the template is written there
    let result = write_policy_template(root.path(), SIGNED_AT);
    // THEN it is refused and the file is untouched
    assert!(matches!(
        result,
        Err(PolicyAuthoringError::AlreadyExists { .. })
    ));
    let kept = std::fs::read_to_string(root.path().join(POLICY_FILE)).unwrap();
    assert_eq!(kept, "keep me");
}

#[test]
fn test_writing_into_the_system_folder_is_refused() {
    // GIVEN the system policy folder, a file inside it, and a user folder
    let system = std::path::Path::new(SYSTEM_POLICY_DIR);
    let inside = system.join(POLICY_FILE);
    let user = tempfile::tempdir().unwrap();
    // WHEN each is checked as a target
    // THEN the first two are the system folder and the third is not
    assert!(is_system_policy_dir(system));
    assert!(is_system_policy_dir(&inside));
    assert!(!is_system_policy_dir(user.path()));
    assert!(matches!(
        sign_policy_dir(system, &org_key(), "Acme IT"),
        Err(PolicyAuthoringError::SystemDir { .. })
    ));
    assert!(matches!(
        write_policy_template(system, SIGNED_AT),
        Err(PolicyAuthoringError::SystemDir { .. })
    ));
}

#[test]
fn test_verify_reports_a_missing_folder_as_absent() {
    // GIVEN a path where no folder exists
    let root = tempfile::tempdir().unwrap();
    // WHEN it is verified
    let verdict = verify_policy_dir(&root.path().join("nowhere"));
    // THEN the verdict is absent
    assert_eq!(verdict, PolicyVerdict::Absent);
}
