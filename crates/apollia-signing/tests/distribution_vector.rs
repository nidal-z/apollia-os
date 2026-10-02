//! Minisign verification against a committed vector.
//!
//! The files under `tests/vectors/minisign/` were produced by the release
//! tooling with throwaway test keys whose private halves were never kept:
//! `cargo tauri signer generate` for `test-dist.key.pub` and `other.key.pub`,
//! then `cargo tauri signer sign` over `manifest.json` for `manifest.json.sig`.
//! `manifest.json.minisig` and `test-dist.pub` are the same signature and key
//! with the outer base64 wrapping of the tooling removed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use apollia_signing::distribution::{verify_minisign, DistributionError};

fn vector(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/vectors/minisign")
        .join(name);
    std::fs::read_to_string(&path).unwrap()
}

#[test]
fn test_verify_minisign_tooling_output_is_valid() {
    // GIVEN the content, signature and public key exactly as the tooling wrote them
    let content = vector("manifest.json");
    let signature = vector("manifest.json.sig");
    let public_key = vector("test-dist.key.pub");

    // WHEN the signature is verified
    let trusted = verify_minisign(content.as_bytes(), &signature, &public_key);

    // THEN it holds and returns the trusted comment
    assert_eq!(trusted.unwrap(), "timestamp:1790853842\tfile:manifest.json");
}

#[test]
fn test_verify_minisign_unwrapped_text_is_valid() {
    // GIVEN the plain minisign text of the signature, and the bare key line
    let content = vector("manifest.json");
    let signature = vector("manifest.json.minisig");
    let key_file = vector("test-dist.pub");
    let bare_key = key_file.lines().nth(1).unwrap();

    // WHEN the signature is verified with the key file and with the bare key
    let with_file = verify_minisign(content.as_bytes(), &signature, &key_file);
    let with_bare = verify_minisign(content.as_bytes(), &signature, bare_key);

    // THEN both hold
    assert!(with_file.is_ok(), "{with_file:?}");
    assert!(with_bare.is_ok(), "{with_bare:?}");
}

#[test]
fn test_verify_minisign_one_content_byte_changed_is_rejected() {
    // GIVEN the signed content with a single byte changed
    let mut content = vector("manifest.json").into_bytes();
    content[10] ^= 0x01;
    let signature = vector("manifest.json.sig");
    let public_key = vector("test-dist.key.pub");

    // WHEN the signature is verified
    let result = verify_minisign(&content, &signature, &public_key);

    // THEN it is rejected
    assert!(
        matches!(result, Err(DistributionError::Rejected { .. })),
        "{result:?}"
    );
}

#[test]
fn test_verify_minisign_other_key_is_rejected() {
    // GIVEN a public key that did not sign
    let content = vector("manifest.json");
    let signature = vector("manifest.json.sig");
    let other_key = vector("other.key.pub");

    // WHEN the signature is verified with it
    let result = verify_minisign(content.as_bytes(), &signature, &other_key);

    // THEN it is rejected
    assert!(
        matches!(result, Err(DistributionError::Rejected { .. })),
        "{result:?}"
    );
}

#[test]
fn test_verify_minisign_altered_trusted_comment_is_rejected() {
    // GIVEN the signature with its trusted comment rewritten
    let content = vector("manifest.json");
    let signature = vector("manifest.json.minisig").replace(
        "trusted comment: timestamp:1790853842\tfile:manifest.json",
        "trusted comment: timestamp:1790853842\tfile:other.json",
    );
    assert!(signature.contains("file:other.json"));
    let public_key = vector("test-dist.key.pub");

    // WHEN the signature is verified
    let result = verify_minisign(content.as_bytes(), &signature, &public_key);

    // THEN it is rejected
    assert!(
        matches!(result, Err(DistributionError::Rejected { .. })),
        "{result:?}"
    );
}

#[test]
fn test_verify_minisign_garbage_signature_is_invalid_wrapping() {
    // GIVEN a signature that is neither minisign text nor base64
    let content = vector("manifest.json");
    let public_key = vector("test-dist.key.pub");

    // WHEN it is verified
    let result = verify_minisign(content.as_bytes(), "not a signature!", &public_key);

    // THEN it is refused before any cryptography
    assert!(
        matches!(
            result,
            Err(DistributionError::InvalidWrapping { what: "signature" })
        ),
        "{result:?}"
    );
}

#[test]
fn test_verify_minisign_malformed_public_key_is_invalid_public_key() {
    // GIVEN a bare key line that is too short
    let content = vector("manifest.json");
    let signature = vector("manifest.json.sig");

    // WHEN the signature is verified with it
    let result = verify_minisign(content.as_bytes(), &signature, "RWQshort");

    // THEN the key is refused
    assert!(
        matches!(result, Err(DistributionError::InvalidPublicKey { .. })),
        "{result:?}"
    );
}

#[test]
fn test_verify_minisign_malformed_signature_is_invalid_signature() {
    // GIVEN minisign text whose payload line is truncated
    let content = vector("manifest.json");
    let public_key = vector("test-dist.key.pub");
    let signature = "untrusted comment: x\nRUQshort\ntrusted comment: y\nAAAA\n";

    // WHEN it is verified
    let result = verify_minisign(content.as_bytes(), signature, &public_key);

    // THEN the signature is refused
    assert!(
        matches!(result, Err(DistributionError::InvalidSignature { .. })),
        "{result:?}"
    );
}

#[test]
fn test_compiled_distribution_key_is_absent_or_decodable() {
    // GIVEN the distribution key compiled into this build
    let compiled = apollia_signing::distribution::distribution_public_key();

    // WHEN it is used to check a signature it did not make
    let checked = compiled.map(|key| {
        verify_minisign(
            vector("manifest.json").as_bytes(),
            &vector("manifest.json.sig"),
            key,
        )
    });

    // THEN either no key is compiled, or the key decodes and only the
    // signature is refused
    match checked {
        None => {}
        Some(result) => assert!(matches!(result, Err(DistributionError::Rejected { .. }))),
    }
}
