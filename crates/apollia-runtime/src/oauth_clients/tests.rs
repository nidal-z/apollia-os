use tempfile::TempDir;

use super::*;
use crate::api::test_support::MemorySecrets;

fn no_env(_: &str) -> Option<String> {
    None
}

#[test]
fn test_is_guid_like_accepts_canonical_guid() {
    // GIVEN two canonical GUIDs, one lowercase and one uppercase
    // WHEN each is checked for shape
    // THEN both pass, so case is not what a rejection means
    assert!(is_guid_like("12345678-1234-1234-1234-123456789abc"));
    assert!(is_guid_like("DEADBEEF-0000-1111-2222-333344445555"));
}

#[test]
fn test_is_guid_like_rejects_malformed() {
    // GIVEN a word, a GUID without its dashes, one with a non-hexadecimal digit and an empty string
    // WHEN each is checked for shape
    // THEN all four are rejected
    assert!(!is_guid_like("not-a-guid"));
    assert!(!is_guid_like("12345678123412341234123456789abc"));
    assert!(!is_guid_like("12345678-1234-1234-1234-123456789abz"));
    assert!(!is_guid_like(""));
}

#[test]
fn test_client_id_shape_error_google() {
    // GIVEN a well-formed Google client id
    // WHEN its shape is checked
    // THEN nothing is reported
    assert!(client_id_shape_error(
        ConnectorProvider::Google,
        "123-abc.apps.googleusercontent.com"
    )
    .is_none());
    // AND a malformed one
    // WHEN its shape is checked
    // THEN the operator is told before any OAuth round trip starts
    assert!(client_id_shape_error(ConnectorProvider::Google, "bogus").is_some());
}

#[test]
fn test_client_id_shape_error_microsoft() {
    // GIVEN a well-formed Microsoft client id, then a malformed one
    // WHEN the shape of each is checked
    // THEN the first passes and the second is reported before any OAuth round trip
    assert!(client_id_shape_error(
        ConnectorProvider::Microsoft,
        "12345678-1234-1234-1234-123456789abc"
    )
    .is_none());
    assert!(client_id_shape_error(ConnectorProvider::Microsoft, "not-a-guid").is_some());
}

#[test]
fn test_oauth_client_test_result_serializes() {
    // GIVEN the result of a connectivity test
    let result = OauthClientTestResult {
        ok: true,
        detail: "reachable".to_string(),
    };
    // WHEN it crosses the bridge as JSON
    let json = serde_json::to_value(&result).expect("serialize");
    // THEN the front end reads both the verdict and its detail
    assert_eq!(json["ok"], true);
    assert_eq!(json["detail"], "reachable");
}

#[test]
fn test_parse_google_client_json_reads_installed_section() {
    // GIVEN the file a Google Cloud Desktop client download produces
    let raw = r#"{
        "installed": {
            "client_id": "123-abc.apps.googleusercontent.com",
            "project_id": "apollia-test",
            "client_secret": "GOCSPX-example",
            "redirect_uris": ["http://localhost"]
        }
    }"#;

    // WHEN it is parsed
    let (id, secret) = parse_google_client_json(raw).expect("parse");

    // THEN both halves come out, so the operator never reads the file
    assert_eq!(id, "123-abc.apps.googleusercontent.com");
    assert_eq!(secret.as_deref(), Some("GOCSPX-example"));
}

#[test]
fn test_parse_google_client_json_accepts_web_section() {
    // GIVEN the other shape the console produces
    let raw = r#"{"web": {"client_id": "456-def.apps.googleusercontent.com"}}"#;

    // WHEN it is parsed
    let (id, secret) = parse_google_client_json(raw).expect("parse");

    // THEN the client id is read and the absent secret stays absent
    assert_eq!(id, "456-def.apps.googleusercontent.com");
    assert!(secret.is_none());
}

#[test]
fn test_parse_google_client_json_rejects_unknown_shape() {
    // GIVEN a JSON file that is not an OAuth client export
    let raw = r#"{"type": "service_account", "private_key": "..."}"#;

    // WHEN it is parsed
    let err = parse_google_client_json(raw).expect_err("service accounts are not OAuth clients");

    // THEN the operator is told what was expected
    match err {
        OauthClientsError::InvalidClientFile(reason) => assert!(reason.contains("installed")),
        other => panic!("expected InvalidClientFile, got: {other:?}"),
    }
}

#[test]
fn test_parse_google_client_json_rejects_malformed_input() {
    // GIVEN a file that is not JSON at all
    // WHEN it is parsed
    let err = parse_google_client_json("not json").expect_err("must not accept garbage");

    // THEN the failure is attributed to the file, not to Apollia
    assert!(matches!(err, OauthClientsError::InvalidClientFile(_)));
}

#[test]
fn test_a_written_client_keeps_its_id_in_the_file_and_its_secrets_in_the_store() {
    // GIVEN an empty data directory, a secret store and no environment override
    let dir = TempDir::new().unwrap();
    let file = file_in(dir.path());
    let secrets = MemorySecrets::default();

    // WHEN a Google client id, secret and API key are written, padded
    for (field, value) in [
        (
            OauthClientField::ClientId,
            " 1-a.apps.googleusercontent.com ",
        ),
        (OauthClientField::ClientSecret, "GOCSPX-s"),
        (OauthClientField::ApiKey, "AIza-k"),
    ] {
        set_field(&file, "google", field, value, &secrets.opener()).unwrap();
    }
    let rows = list_client_ids_with(&file, &no_env, Some(&secrets));

    // THEN the Google row reports the trimmed id from the file and the
    // secret and key from the store, the file holds neither, and no row
    // serialises a secret value
    let google = rows.iter().find(|r| r.provider == "google").unwrap();
    assert_eq!(google.effective_client_id, "1-a.apps.googleusercontent.com");
    assert_eq!(google.source, "file");
    assert_eq!(
        google.override_client_id.as_deref(),
        Some("1-a.apps.googleusercontent.com")
    );
    assert!(google.has_client_secret && google.has_api_key);
    assert_eq!(google.client_secret_source, "store");
    assert_eq!(google.api_key_source, "store");
    let on_disk = std::fs::read_to_string(&file).unwrap();
    assert!(
        !on_disk.contains("GOCSPX-s") && !on_disk.contains("AIza-k"),
        "{on_disk}"
    );
    let json = serde_json::to_string(&rows).unwrap();
    assert!(
        !json.contains("GOCSPX-s") && !json.contains("AIza-k"),
        "{json}"
    );
    assert_eq!(rows.len(), 2);
}

#[test]
fn test_the_environment_wins_over_the_stored_secret() {
    // GIVEN a client secret in the store, and the env var holding another
    let dir = TempDir::new().unwrap();
    let file = file_in(dir.path());
    let secrets = MemorySecrets::default();
    set_field(
        &file,
        "google",
        OauthClientField::ClientSecret,
        "GOCSPX-stored",
        &secrets.opener(),
    )
    .unwrap();
    let env = |name: &str| {
        (name == ConnectorProvider::Google.client_secret_env_var()).then(|| "GOCSPX-env".to_owned())
    };

    // WHEN the rows are resolved
    let rows = list_client_ids_with(&file, &env, Some(&secrets));

    // THEN the environment is the layer that wins
    let google = rows.iter().find(|r| r.provider == "google").unwrap();
    assert!(google.has_client_secret);
    assert_eq!(google.client_secret_source, "env");
}

#[test]
fn test_a_file_an_earlier_version_wrote_gives_its_secrets_to_the_store_at_start() {
    // GIVEN an oauth-clients.toml holding a client secret and an API key in clear
    let dir = TempDir::new().unwrap();
    let file = file_in(dir.path());
    std::fs::write(
        &file,
        "[google]\nclient_id = \"1-a.apps.googleusercontent.com\"\n\
         client_secret = \"GOCSPX-old\"\napi_key = \"AIza-old\"\n",
    )
    .unwrap();
    let secrets = MemorySecrets::default();

    // WHEN Apollia starts
    let moved = migrate_file_secrets_at_startup(&file, &secrets.opener());

    // THEN the store holds both, the file holds neither, and the row reads
    // them from the store
    assert_eq!(moved, 2);
    assert_eq!(
        secrets
            .value("apollia-oauth-clients", "google:client_secret")
            .as_deref(),
        Some("GOCSPX-old")
    );
    let on_disk = std::fs::read_to_string(&file).unwrap();
    assert!(!on_disk.contains("-old\""), "{on_disk}");
    let rows = list_client_ids_with(&file, &no_env, Some(&secrets));
    let google = rows.iter().find(|r| r.provider == "google").unwrap();
    assert_eq!(google.client_secret_source, "store");
    assert_eq!(google.effective_client_id, "1-a.apps.googleusercontent.com");
}

#[test]
fn test_a_store_that_does_not_open_leaves_the_file_for_the_next_start() {
    // GIVEN a file holding a client secret, and a secret store that cannot open
    let dir = TempDir::new().unwrap();
    let file = file_in(dir.path());
    let raw = "[google]\nclient_secret = \"GOCSPX-kept\"\n";
    std::fs::write(&file, raw).unwrap();
    let broken: SecretStoreOpener = std::sync::Arc::new(|| Err("locked".to_owned()));

    // WHEN Apollia starts
    let moved = migrate_file_secrets_at_startup(&file, &broken);

    // THEN nothing moves and the file is untouched
    assert_eq!(moved, 0);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), raw);
}

#[test]
fn test_the_environment_wins_over_the_file() {
    // GIVEN a file holding a Google client id, and the env var holding another
    let dir = TempDir::new().unwrap();
    let file = file_in(dir.path());
    set_field(
        &file,
        "google",
        OauthClientField::ClientId,
        "f.apps.googleusercontent.com",
        &MemorySecrets::default().opener(),
    )
    .unwrap();
    let env = |name: &str| {
        (name == ConnectorProvider::Google.client_id_env_var())
            .then(|| "e.apps.googleusercontent.com".to_owned())
    };

    // WHEN the rows are resolved
    let rows = list_client_ids_with(&file, &env, None);

    // THEN the environment value is effective, and the override still shows
    let google = rows.iter().find(|r| r.provider == "google").unwrap();
    assert_eq!(google.effective_client_id, "e.apps.googleusercontent.com");
    assert_eq!(google.source, "env");
    assert_eq!(
        google.override_client_id.as_deref(),
        Some("f.apps.googleusercontent.com")
    );
}

#[test]
fn test_an_unknown_provider_is_refused_before_any_write() {
    // GIVEN an empty data directory
    let dir = TempDir::new().unwrap();
    let file = file_in(dir.path());

    // WHEN a client id is written for a provider Apollia does not know
    let err = set_field(
        &file,
        "yahoo",
        OauthClientField::ClientId,
        "x",
        &MemorySecrets::default().opener(),
    )
    .unwrap_err();

    // THEN it is refused by name and no file appears
    assert!(matches!(&err, OauthClientsError::UnknownProvider(p) if p == "yahoo"));
    assert!(!file.exists());
}

#[test]
fn test_an_imported_export_writes_both_halves() {
    // GIVEN the text of a Google Desktop client export
    let dir = TempDir::new().unwrap();
    let file = file_in(dir.path());
    let raw = r#"{"installed": {"client_id": "9-z.apps.googleusercontent.com", "client_secret": "GOCSPX-z"}}"#;

    let secrets = MemorySecrets::default();

    // WHEN it is imported
    import_client_json(&file, "google", raw, &secrets.opener()).unwrap();

    // THEN the client id is in the file and the secret in the store only
    let rows = list_client_ids_with(&file, &no_env, Some(&secrets));
    let google = rows.iter().find(|r| r.provider == "google").unwrap();
    assert_eq!(google.effective_client_id, "9-z.apps.googleusercontent.com");
    assert!(google.has_client_secret);
    assert_eq!(google.client_secret_source, "store");
    assert!(!std::fs::read_to_string(&file).unwrap().contains("GOCSPX-z"));
}

#[test]
fn test_microsoft_has_no_export_to_import() {
    // GIVEN a well-formed export, sent for Microsoft
    let dir = TempDir::new().unwrap();
    let file = file_in(dir.path());
    let raw = r#"{"installed": {"client_id": "9-z.apps.googleusercontent.com"}}"#;

    // WHEN it is imported
    let err = import_client_json(&file, "microsoft", raw, &MemorySecrets::default().opener())
        .unwrap_err();

    // THEN the operator is told to paste the application id, and nothing is written
    assert!(
        matches!(&err, OauthClientsError::InvalidClientFile(r) if r.contains("paste the application id"))
    );
    assert!(!file.exists());
}

#[test]
fn test_the_local_check_names_the_first_missing_piece() {
    // GIVEN a Google client id without its secret
    let dir = TempDir::new().unwrap();
    let file = file_in(dir.path());
    set_field(
        &file,
        "google",
        OauthClientField::ClientId,
        "1-a.apps.googleusercontent.com",
        &MemorySecrets::default().opener(),
    )
    .unwrap();

    // WHEN the client is checked locally
    let verdict = local_check(ConnectorProvider::Google, &file, &no_env, None);

    // THEN the missing secret is named before any network call
    assert_eq!(
        verdict.as_deref(),
        Some("client secret is required for this provider but not configured")
    );
}
