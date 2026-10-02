//! Import check tests, on files built in a temporary directory.

use std::path::{Path, PathBuf};

use super::{check_import, check_structure, weights_sha256, ImportCheckError, ImportRule};
use crate::model_index::ModelIndex;

fn write(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, bytes).unwrap();
    path
}

fn gguf_bytes() -> Vec<u8> {
    let mut out = b"GGUF".to_vec();
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    out
}

#[test]
fn test_a_known_digest_makes_the_origin_verified() {
    // GIVEN a GGUF on a key whose digest the machine already knows
    let tmp = tempfile::tempdir().unwrap();
    let source = write(tmp.path(), "key/model.gguf", &gguf_bytes());
    let known = weights_sha256(&source).unwrap();

    // WHEN it is checked for import
    let check = check_import(&source, &[known.as_str()], ImportRule::default()).unwrap();

    // THEN its origin is verified and its digest is the one known
    assert!(check.origin_verified);
    assert_eq!(check.sha256, known);
}

#[test]
fn test_an_unknown_digest_passes_the_structure_check_unverified() {
    // GIVEN a well-formed GGUF whose digest nothing on the machine knows
    let tmp = tempfile::tempdir().unwrap();
    let source = write(tmp.path(), "key/private.gguf", &gguf_bytes());
    let other = "0".repeat(64);

    // WHEN it is checked for import, with the default rule
    let check = check_import(&source, &[other.as_str()], ImportRule::default()).unwrap();

    // THEN it is accepted, with an unverified origin and its digest recorded
    assert!(!check.origin_verified);
    assert_eq!(check.sha256.len(), 64);
}

#[test]
fn test_a_text_file_renamed_gguf_is_refused_with_a_readable_error() {
    // GIVEN a text file renamed .gguf
    let tmp = tempfile::tempdir().unwrap();
    let source = write(tmp.path(), "notes.gguf", b"these are meeting notes\n");

    // WHEN it is checked for import
    let err = check_import(&source, &[], ImportRule::default()).unwrap_err();

    // THEN it is refused as not a GGUF model, with a message naming the header
    assert!(matches!(err, ImportCheckError::NotGguf { .. }));
    assert!(err
        .to_string()
        .contains("does not start with the GGUF header"));
}

#[test]
fn test_a_file_shorter_than_the_header_is_refused() {
    // GIVEN an empty file named .gguf
    let tmp = tempfile::tempdir().unwrap();
    let source = write(tmp.path(), "empty.gguf", b"");

    // WHEN its structure is checked
    let err = check_structure(&source).unwrap_err();

    // THEN it is refused as not a GGUF model rather than as a read failure
    assert!(matches!(err, ImportCheckError::NotGguf { .. }));
}

#[test]
fn test_an_mlx_folder_with_config_and_weights_passes() {
    // GIVEN an MLX folder with a readable config.json and a weights file
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("qwen-mlx");
    write(&dir, "config.json", br#"{"model_type": "qwen2"}"#);
    write(&dir, "model.safetensors", b"weights");

    // WHEN it is checked for import
    let check = check_import(&dir, &[], ImportRule::default()).unwrap();

    // THEN it is accepted, unverified
    assert!(!check.origin_verified);
}

#[test]
fn test_an_mlx_folder_without_weights_is_refused() {
    // GIVEN a folder with a config.json and no .safetensors file
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("half");
    write(&dir, "config.json", b"{}");
    write(&dir, "tokenizer.json", b"{}");

    // WHEN its structure is checked
    let err = check_structure(&dir).unwrap_err();

    // THEN it is refused for lack of weights
    assert!(matches!(err, ImportCheckError::MlxNoWeights { .. }));
}

#[test]
fn test_an_mlx_folder_with_an_unreadable_config_is_refused() {
    // GIVEN a folder whose config.json is not JSON
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("broken");
    write(&dir, "config.json", b"not json");
    write(&dir, "model.safetensors", b"weights");

    // WHEN its structure is checked
    let err = check_structure(&dir).unwrap_err();

    // THEN it is refused for its config
    assert!(matches!(err, ImportCheckError::MlxConfig { .. }));
}

#[test]
fn test_the_folder_digest_does_not_depend_on_where_the_folder_is() {
    // GIVEN the same MLX folder in two places
    let tmp = tempfile::tempdir().unwrap();
    for root in ["a/model", "b/elsewhere/model"] {
        let dir = tmp.path().join(root);
        write(&dir, "config.json", b"{}");
        write(&dir, "model-00001.safetensors", b"one");
        write(&dir, "sub/extra.json", b"two");
    }

    // WHEN both digests are computed
    let a = weights_sha256(&tmp.path().join("a/model")).unwrap();
    let b = weights_sha256(&tmp.path().join("b/elsewhere/model")).unwrap();

    // THEN they are equal
    assert_eq!(a, b);
}

#[test]
fn test_require_verified_refuses_an_unverified_import() {
    // GIVEN a well-formed GGUF of unknown digest and the rule requiring a
    //       verified origin
    let tmp = tempfile::tempdir().unwrap();
    let source = write(tmp.path(), "private.gguf", &gguf_bytes());
    let rule = ImportRule {
        require_verified: true,
    };

    // WHEN it is checked for import
    let err = check_import(&source, &[], rule).unwrap_err();

    // THEN it is refused as unverified
    assert!(matches!(err, ImportCheckError::Unverified { .. }));
}

#[test]
fn test_require_verified_lets_a_known_digest_through() {
    // GIVEN a GGUF whose digest is known, and the rule requiring a verified
    //       origin
    let tmp = tempfile::tempdir().unwrap();
    let source = write(tmp.path(), "known.gguf", &gguf_bytes());
    let known = weights_sha256(&source).unwrap();
    let rule = ImportRule {
        require_verified: true,
    };

    // WHEN it is checked for import
    let check = check_import(&source, &[known.as_str()], rule).unwrap();

    // THEN it is accepted as verified
    assert!(check.origin_verified);
}

#[test]
fn test_the_index_offers_only_verified_digests() {
    // GIVEN an index with one verified and one unverified model
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write(dir, "good.gguf", &gguf_bytes());
    let mut other = gguf_bytes();
    other.push(1);
    write(dir, "other.gguf", &other);
    let mut index = ModelIndex::default();
    index.rebuild(dir).unwrap();
    index
        .set_digest(&dir.join("good.gguf"), "a".repeat(64), true)
        .unwrap();
    index
        .set_digest(&dir.join("other.gguf"), "b".repeat(64), false)
        .unwrap();

    // WHEN the known digests are read
    let digests = index.verified_digests();

    // THEN only the verified one is there
    assert_eq!(digests, vec!["a".repeat(64).as_str()]);
}

#[test]
fn test_an_embeddings_model_enters_through_the_same_import_path() {
    // GIVEN an embeddings GGUF on a key: a bert header pooled by mean
    let tmp = tempfile::tempdir().unwrap();
    let mut bytes = b"GGUF".to_vec();
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(&2u64.to_le_bytes());
    for (key, tag, value) in [
        ("general.architecture", 8u32, b"bert".to_vec()),
        ("bert.pooling_type", 4u32, 1u32.to_le_bytes().to_vec()),
    ] {
        bytes.extend_from_slice(&(key.len() as u64).to_le_bytes());
        bytes.extend_from_slice(key.as_bytes());
        bytes.extend_from_slice(&tag.to_le_bytes());
        if tag == 8 {
            bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        }
        bytes.extend_from_slice(&value);
    }
    let source = write(tmp.path(), "key/e5.gguf", &bytes);
    let models = tmp.path().join("models");
    std::fs::create_dir_all(&models).unwrap();

    // WHEN it goes through the import every model goes through: the check,
    //      the copy, the record and the digest
    let check = check_import(&source, &[], ImportRule::default()).unwrap();
    let dest = models.join("e5.gguf");
    std::fs::copy(&source, &dest).unwrap();
    let mut index = ModelIndex::default();
    index
        .record(&models, &dest, crate::model_index::ModelOrigin::Import)
        .unwrap();
    index
        .set_digest(&dest, check.sha256, check.origin_verified)
        .unwrap();

    // THEN it is indexed as an embeddings model, with its digest
    let entry = index.get("e5").unwrap();
    assert_eq!(
        entry.category,
        crate::model_index::ModelCategory::Embeddings
    );
    assert_eq!(entry.sha256.as_deref().map(str::len), Some(64));
}
