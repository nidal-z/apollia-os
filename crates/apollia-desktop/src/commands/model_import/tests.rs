//! Import tests, on a key folder and a models folder built in a temporary
//! directory.

use std::path::{Path, PathBuf};

use apollia_core::config::PolicySettings;
use apollia_llm::{ModelFormat, ModelImportRule, ModelOrigin};

use super::{
    import_language_data_pinned_by, import_model_file_into, import_model_file_under, import_rule,
    registry_digests, ImportModelError,
};
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
    let dir = apollia_core::config::default_models_dir(tmp.path());

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
    let dir = apollia_core::config::default_models_dir(tmp.path());
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
    let dir = apollia_core::config::default_models_dir(tmp.path());
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
    let dir = apollia_core::config::default_models_dir(tmp.path());
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
    let dir = apollia_core::config::default_models_dir(tmp.path());
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
    let dir = apollia_core::config::default_models_dir(tmp.path());
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

    let rule = import_rule(tmp.path(), &PolicySettings::none());

    assert!(rule.rule.require_verified);
    assert!(!rule.fixed_by_policy);
}

// GIVEN no apollia.toml, then one without the key
// WHEN the import rule is read
// THEN it is off: unverified imports are accepted by default
#[test]
fn test_the_rule_is_off_by_default() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = import_rule(tmp.path(), &PolicySettings::none());
    write(tmp.path(), "apollia.toml", b"[llm]\n");

    let without_key = import_rule(tmp.path(), &PolicySettings::none());

    assert!(!missing.rule.require_verified);
    assert!(!without_key.rule.require_verified);
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

// GIVEN an apollia.toml that leaves unverified imports on, a policy that
//       requires a model hash, and a GGUF of unknown digest
// WHEN the rule is read under the policy and the file is imported
// THEN the rule is fixed to true by the policy, the import is refused with the
//      policy as its reason, and nothing is copied
#[test]
fn test_a_policy_requiring_a_hash_refuses_an_unverified_import() {
    // GIVEN
    let tmp = tempfile::tempdir().unwrap();
    write(
        tmp.path(),
        "apollia.toml",
        b"[models.import]\nrequire_verified = false\n",
    );
    let imposed = PolicySettings::none().impose(
        apollia_permissions::REQUIRE_VERIFIED_SETTING,
        serde_json::json!(true),
    );
    let dir = apollia_core::config::default_models_dir(tmp.path());
    let source = write(tmp.path(), "key/private.gguf", &gguf_bytes(4));

    // WHEN
    let rule = import_rule(tmp.path(), &imposed);
    let err = import_model_file_under(&source.to_string_lossy(), &gguf(), &dir, rule).unwrap_err();

    // THEN
    assert!(rule.rule.require_verified);
    assert!(rule.fixed_by_policy);
    assert!(
        matches!(err, ImportModelError::RefusedByPolicy { .. }),
        "{err}"
    );
    assert!(err.to_string().contains("organisation policy"), "{err}");
    assert!(!dir.join("private.gguf").exists());
}

/// SHA-256 of the bytes `language data, French`.
const PINNED_SHA256: &str = "ece5944eaf99d515fab0a56c2ae2eb5f8a81603ab80f9a3c19b18f225a73b257";

#[test]
fn test_a_language_data_file_enters_the_tessdata_folder_only_when_the_registry_pins_it() {
    // GIVEN, on a key, two language data files, and a registry that pins the
    // digest of the first under the code `fra`
    let key = tempfile::tempdir().unwrap();
    let models = tempfile::tempdir().unwrap();
    let pinned = write(
        key.path(),
        "french copy.TRAINEDDATA",
        b"language data, French",
    );
    let unknown = write(key.path(), "eng.traineddata", b"not a pinned file");
    let catalogue = apollia_llm::recommend::OcrCatalogue::from_toml(&format!(
        "[[ocr]]\nlanguage = \"fra\"\nlabel = \"French\"\npublisher = \"test\"\n\
         released = \"2017-09-14\"\nlicense = \"apache-2.0\"\nrepo = \"owner/tessdata\"\n\
         file = \"fra.traineddata\"\nsize_bytes = 21\nsha256 = \"{PINNED_SHA256}\"\n\
         source = \"https://example.invalid\"\n"
    ))
    .unwrap();

    // WHEN the pinned one is imported against that registry, and the other
    // through the model import, which expects a GGUF
    let imported = import_language_data_pinned_by(&pinned, models.path(), &catalogue);
    let refused = import(&unknown, models.path(), ModelImportRule::default());

    // THEN the pinned file lands under the name the engine reads, with no
    // entry in the model index, and the other is refused before any
    // extension check
    let dest = models.path().join("tessdata").join("fra.traineddata");
    assert_eq!(imported.unwrap(), dest.display().to_string());
    assert!(dest.is_file());
    assert!(model_index::current(models.path())
        .ok()
        .is_none_or(|i| i.verified_digests().is_empty()));
    assert!(matches!(refused, Err(ImportModelError::OcrData(_))));
    assert!(!models
        .path()
        .join("tessdata")
        .join("eng.traineddata")
        .exists());
}
