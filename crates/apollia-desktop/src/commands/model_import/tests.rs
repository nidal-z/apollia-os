//! Import tests, on a key folder and a models folder built in a temporary
//! directory.

use std::path::{Path, PathBuf};

use apollia_llm::{ModelFormat, ModelImportRule, ModelOrigin};

use super::{import_model_file_into, import_rule, registry_digests, ImportModelError};
use crate::commands::model_index;

fn gguf_bytes(tag: u8) -> Vec<u8> {
    let mut out = b"GGUF".to_vec();
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    out.push(tag);
    out
}

fn write(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, bytes).unwrap();
    path
}

fn gguf() -> Vec<String> {
    vec!["gguf".to_string()]
}

fn import(source: &Path, dir: &Path, rule: ModelImportRule) -> Result<String, ImportModelError> {
    import_model_file_into(&source.to_string_lossy(), &gguf(), dir, rule)
}

// GIVEN a GGUF file outside the models folder, picked by the operator
// WHEN it is imported
// THEN the copy is in the model index, with an import origin
#[test]
fn test_an_import_adds_its_entry_to_the_index() {
    let tmp = tempfile::tempdir().unwrap();
    let source = write(tmp.path(), "picked.gguf", &gguf_bytes(0));
    let dir = tmp.path().join("models");

    import(&source, &dir, ModelImportRule::default()).unwrap();

    let index = model_index::current(&dir).unwrap();
    let entry = index.get("picked").unwrap();
    assert_eq!(entry.path, dir.join("picked.gguf"));
    assert_eq!(entry.origin, ModelOrigin::Import);
}

// GIVEN a verified download already in the index, and the same weights on a
//       USB key under another name
// WHEN the key's copy is imported
// THEN its entry is marked verified, its digest that of the download
#[test]
fn test_an_import_of_a_known_digest_is_verified() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("models");
    let downloaded = write(&dir, "qwen.gguf", &gguf_bytes(1));
    let digest = apollia_llm::check_import(&downloaded, &[], ModelImportRule::default())
        .unwrap()
        .sha256;
    model_index::record_download(
        &dir,
        &downloaded,
        Some("org/qwen-GGUF".into()),
        "qwen.gguf".into(),
        model_index::DownloadDigest {
            sha256: Some(digest.clone()),
            verified: true,
        },
    );
    let source = write(tmp.path(), "key/qwen-copy.gguf", &gguf_bytes(1));

    import(&source, &dir, ModelImportRule::default()).unwrap();

    let index = model_index::current(&dir).unwrap();
    let entry = index.get("qwen-copy").unwrap();
    assert!(entry.origin_verified);
    assert_eq!(entry.sha256.as_deref(), Some(digest.as_str()));
}

// GIVEN a well-formed GGUF on a key whose digest nothing knows
// WHEN it is imported with the default rule
// THEN it is copied and indexed with its digest, origin unverified
#[test]
fn test_an_import_of_an_unknown_digest_is_unverified() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("models");
    let source = write(tmp.path(), "key/private.gguf", &gguf_bytes(2));

    import(&source, &dir, ModelImportRule::default()).unwrap();

    let index = model_index::current(&dir).unwrap();
    let entry = index.get("private").unwrap();
    assert!(!entry.origin_verified);
    assert_eq!(entry.sha256.as_ref().map(String::len), Some(64));
}

// GIVEN a text file renamed .gguf on a key
// WHEN it is imported
// THEN it is refused with a message naming the missing header, and nothing is
//      copied into the models folder
#[test]
fn test_a_text_file_renamed_gguf_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("models");
    let source = write(tmp.path(), "key/notes.gguf", b"meeting notes\n");

    let err = import(&source, &dir, ModelImportRule::default()).unwrap_err();

    assert!(err
        .to_string()
        .contains("is not a GGUF model: the file does not start with the GGUF header"));
    assert!(!dir.join("notes.gguf").exists());
}

// GIVEN the rule requiring a verified origin and a GGUF of unknown digest
// WHEN it is imported
// THEN it is refused and nothing is copied
#[test]
fn test_require_verified_refuses_an_unverified_import() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("models");
    let source = write(tmp.path(), "key/private.gguf", &gguf_bytes(3));
    let rule = ModelImportRule {
        require_verified: true,
    };

    let err = import(&source, &dir, rule).unwrap_err();

    assert!(err.to_string().contains("origin cannot be verified"));
    assert!(!dir.join("private.gguf").exists());
}

// GIVEN an MLX folder on a key, with its config and weights
// WHEN it is imported
// THEN the folder is copied whole and indexed as one MLX model
#[test]
fn test_an_mlx_folder_is_imported_whole() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("models");
    let source = tmp.path().join("key/qwen-mlx");
    write(&source, "config.json", br#"{"model_type": "qwen2"}"#);
    write(&source, "model.safetensors", b"weights");
    write(&source, "tokenizer.json", b"{}");

    let dest = import(&source, &dir, ModelImportRule::default()).unwrap();

    assert_eq!(PathBuf::from(dest), dir.join("qwen-mlx"));
    assert!(dir.join("qwen-mlx/tokenizer.json").is_file());
    let index = model_index::current(&dir).unwrap();
    let entry = index.get("qwen-mlx").unwrap();
    assert_eq!(entry.format, ModelFormat::Mlx);
    assert!(!entry.origin_verified);
}

// GIVEN an apollia.toml that sets models.import.require_verified to true
// WHEN the import rule is read
// THEN it requires a verified origin
#[test]
fn test_the_rule_is_read_from_the_settings_file() {
    let tmp = tempfile::tempdir().unwrap();
    write(
        tmp.path(),
        "apollia.toml",
        b"[models.import]\nrequire_verified = true\n",
    );

    let rule = import_rule(tmp.path());

    assert!(rule.require_verified);
}

// GIVEN no apollia.toml, then one without the key
// WHEN the import rule is read
// THEN it is off: unverified imports are accepted by default
#[test]
fn test_the_rule_is_off_by_default() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = import_rule(tmp.path());
    write(tmp.path(), "apollia.toml", b"[llm]\n");

    let without_key = import_rule(tmp.path());

    assert!(!missing.require_verified);
    assert!(!without_key.require_verified);
}

// GIVEN the registry's embeddings entries
// WHEN the import gathers the digests it compares a file with
// THEN the pinned file of each entry is among them, so a copy brought on a key
//      enters verified, by the same check as any model
#[test]
fn test_an_import_knows_the_digests_the_registry_pins() {
    let digests = registry_digests();

    assert!(digests
        .iter()
        .any(|d| d == "0a34067a40f25d3149b36885faa62bee0e5284d0f9edc102acfc00e115d953e8"));
    assert!(digests
        .iter()
        .any(|d| d == "06507c7b42688469c4e7298b0a1e16deff06caf291cf0a5b278c308249c3e439"));
}
