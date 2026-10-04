//! Reading an archive: manifest alone, extraction guards, signature state.
//!
//! Every hostile archive is made here with the zip writer, never taken from
//! disk, so each test shows exactly what it feeds the reader.

use std::io::{Cursor, Write};

use apollia_signing::{sha256, DigestList, SignatureFile, SigningKeyPair};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::read::render_public_key;
use super::*;

const MANIFEST: &str = r#"
[package]
name = "lone"
version = "0.1.0"
publisher = "Acme Agents"
sdk = ">=0.3"

[[tools]]
name = "ping"
module = "tools/ping.py"
description = "Answer pong."
action = "read"
inputs = { type = "object" }
"#;

const TOOL: &[u8] = b"@tool(name='ping')\nasync def ping(ctx, inputs):\n    return 'pong'\n";

fn options() -> SimpleFileOptions {
    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated)
}

/// An in-memory archive holding `entries`, in order.
fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zip.start_file(*name, options()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// The digest list a signer computes over `entries`.
fn digest_list(entries: &[(&str, &[u8])]) -> DigestList {
    let mut list = DigestList::new();
    for (name, bytes) in entries {
        list.insert(*name, sha256(bytes)).unwrap();
    }
    list
}

/// `entries` followed by a `SIGNATURE` over `signed` and the key that made it.
fn signed_archive(
    entries: &[(&str, &[u8])],
    signed: &[(&str, &[u8])],
    key: &SigningKeyPair,
) -> Vec<u8> {
    let signature = SignatureFile::create(&digest_list(signed), "Acme Agents", key)
        .unwrap()
        .render();
    let key_text = render_public_key(&key.public_key());
    let mut all: Vec<(&str, &[u8])> = entries.to_vec();
    all.push((SIGNATURE_ENTRY, signature.as_bytes()));
    all.push((SIGNATURE_KEY_ENTRY, key_text.as_bytes()));
    archive(&all)
}

fn open(bytes: Vec<u8>) -> Result<ApoArchive<Cursor<Vec<u8>>>, ApoError> {
    ApoArchive::from_reader(Cursor::new(bytes), ArchiveLimits::default())
}

fn open_with(
    bytes: Vec<u8>,
    limits: ArchiveLimits,
) -> Result<ApoArchive<Cursor<Vec<u8>>>, ApoError> {
    ApoArchive::from_reader(Cursor::new(bytes), limits)
}

// ── manifest ─────────────────────────────────────────────────────────────────

#[test]
fn test_manifest_reads_without_decompressing_other_entries() {
    // GIVEN an archive whose second entry claims a deflate stream that is
    // garbage, so any attempt to decompress it fails
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(MANIFEST_FILE, options()).unwrap();
    zip.write_all(MANIFEST.as_bytes()).unwrap();
    zip.start_file("tools/ping.py", options()).unwrap();
    zip.write_all(TOOL).unwrap();
    let mut bytes = zip.finish().unwrap().into_inner();
    let marker = bytes
        .windows(b"tools/ping.py".len())
        .position(|w| w == b"tools/ping.py")
        .unwrap();
    let header = marker - 30;
    let extra_len = u16::from_le_bytes([bytes[header + 28], bytes[header + 29]]) as usize;
    let data_start = marker + b"tools/ping.py".len() + extra_len;
    for byte in &mut bytes[data_start..data_start + 8] {
        *byte = 0xff;
    }

    // WHEN the archive is opened
    let opened = open(bytes).unwrap();

    // THEN the manifest is there and both entries are listed, and the
    // corrupted entry does fail once something does decompress it
    assert_eq!(opened.manifest().package.name, "lone");
    let paths: Vec<&str> = opened.entries().iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, vec![MANIFEST_FILE, "tools/ping.py"]);
    let mut opened = opened;
    let dir = tempfile::tempdir().unwrap();
    assert!(opened.extract_to(&dir.path().join("out")).is_err());
}

#[test]
fn test_archive_without_manifest_is_refused() {
    // GIVEN an archive with a module and no apollia.toml
    let bytes = archive(&[("tools/ping.py", TOOL)]);

    // WHEN it is opened
    let result = open(bytes);

    // THEN it is refused for its missing manifest
    assert!(matches!(result, Err(ApoError::MissingManifest)));
}

#[test]
fn test_manifest_with_unknown_key_is_refused() {
    // GIVEN an archive whose manifest carries a key the schema does not know
    let manifest = format!("{MANIFEST}\nsurprise = 1\n");
    let bytes = archive(&[(MANIFEST_FILE, manifest.as_bytes())]);

    // WHEN it is opened
    let result = open(bytes);

    // THEN the manifest error comes back
    assert!(matches!(result, Err(ApoError::Manifest(_))));
}

// ── extraction guards ────────────────────────────────────────────────────────

#[test]
fn test_absolute_entry_path_is_refused() {
    // GIVEN an archive holding an entry at an absolute path
    let bytes = archive(&[(MANIFEST_FILE, MANIFEST.as_bytes()), ("/etc/evil.py", b"x")]);

    // WHEN it is opened
    let result = open(bytes);

    // THEN it is refused as an absolute path
    assert!(
        matches!(result, Err(ApoError::UnsafePath { reason, .. }) if reason == "absolute path")
    );
}

#[test]
fn test_parent_component_entry_path_is_refused() {
    // GIVEN an archive holding an entry that climbs out of its target
    let bytes = archive(&[
        (MANIFEST_FILE, MANIFEST.as_bytes()),
        ("tools/../../evil.py", b"x"),
    ]);

    // WHEN it is opened
    let result = open(bytes);

    // THEN it is refused for its `..` component
    assert!(matches!(
        result,
        Err(ApoError::UnsafePath { reason, .. }) if reason == "parent directory component"
    ));
}

#[test]
fn test_symlink_entry_is_refused() {
    // GIVEN an archive holding a symbolic link to a file outside the package
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(MANIFEST_FILE, options()).unwrap();
    zip.write_all(MANIFEST.as_bytes()).unwrap();
    zip.add_symlink("tools/ping.py", "/etc/passwd", options())
        .unwrap();
    let bytes = zip.finish().unwrap().into_inner();

    // WHEN it is opened
    let result = open(bytes);

    // THEN it is refused as a symbolic link
    assert!(matches!(result, Err(ApoError::Symlink { path }) if path == "tools/ping.py"));
}

#[test]
fn test_entry_count_over_limit_is_refused() {
    // GIVEN an archive of 5 entries and a limit of 4
    let bytes = archive(&[
        (MANIFEST_FILE, MANIFEST.as_bytes()),
        ("a.txt", b"a"),
        ("b.txt", b"b"),
        ("c.txt", b"c"),
        ("d.txt", b"d"),
    ]);
    let limits = ArchiveLimits {
        max_entries: 4,
        ..ArchiveLimits::default()
    };

    // WHEN it is opened under that limit
    let result = open_with(bytes, limits);

    // THEN it is refused on the entry count
    assert!(matches!(
        result,
        Err(ApoError::LimitExceeded {
            limit: "entry count",
            max: 4
        })
    ));
}

#[test]
fn test_declared_decompressed_size_over_limit_is_refused() {
    // GIVEN an archive holding a megabyte of zeros, a few kilobytes once
    // deflated, and a total limit of 64 KiB
    let zeros = vec![0u8; 1024 * 1024];
    let bytes = archive(&[(MANIFEST_FILE, MANIFEST.as_bytes()), ("bomb.bin", &zeros)]);
    assert!(bytes.len() < 16 * 1024, "the bomb must be small on disk");
    let limits = ArchiveLimits {
        max_total_bytes: 64 * 1024,
        ..ArchiveLimits::default()
    };

    // WHEN it is opened under that limit
    let result = open_with(bytes, limits);

    // THEN it is refused on its size before anything is decompressed
    assert!(matches!(
        result,
        Err(ApoError::LimitExceeded {
            limit: "decompressed size",
            ..
        })
    ));
}

#[test]
fn test_lying_size_header_is_caught_on_extraction() {
    // GIVEN an archive whose entry declares 4 bytes in its central directory
    // and inflates to a megabyte, under an entry limit of 64 KiB
    let zeros = vec![0u8; 1024 * 1024];
    let mut bytes = archive(&[(MANIFEST_FILE, MANIFEST.as_bytes()), ("bomb.bin", &zeros)]);
    patch_central_size(&mut bytes, "bomb.bin", 4);
    let limits = ArchiveLimits {
        max_entry_bytes: 64 * 1024,
        ..ArchiveLimits::default()
    };
    let mut opened = open_with(bytes, limits).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("out");

    // WHEN it is extracted
    let result = opened.extract_to(&target);

    // THEN the bytes actually inflated trip the limit, and nothing is left
    // behind
    assert!(
        matches!(
            result,
            Err(ApoError::LimitExceeded {
                limit: "entry size",
                ..
            })
        ),
        "{result:?}"
    );
    assert!(!target.exists());
}

/// Rewrites the uncompressed size of `name` in the central directory.
fn patch_central_size(bytes: &mut [u8], name: &str, size: u32) {
    const CENTRAL: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
    let mut at = 0;
    while let Some(pos) = bytes[at..].windows(4).position(|w| w == CENTRAL) {
        let start = at + pos;
        let name_len = u16::from_le_bytes([bytes[start + 28], bytes[start + 29]]) as usize;
        if &bytes[start + 46..start + 46 + name_len] == name.as_bytes() {
            bytes[start + 24..start + 28].copy_from_slice(&size.to_le_bytes());
            return;
        }
        at = start + 4;
    }
    panic!("no central directory entry for {name}");
}

#[test]
fn test_extract_writes_every_entry_under_target() {
    // GIVEN a well-formed archive
    let bytes = archive(&[
        (MANIFEST_FILE, MANIFEST.as_bytes()),
        ("tools/ping.py", TOOL),
    ]);
    let mut opened = open(bytes).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("out");

    // WHEN it is extracted
    opened.extract_to(&target).unwrap();

    // THEN each entry is at its path with its bytes
    assert_eq!(std::fs::read(target.join("tools/ping.py")).unwrap(), TOOL);
    assert_eq!(
        std::fs::read_to_string(target.join(MANIFEST_FILE)).unwrap(),
        MANIFEST
    );
}

#[test]
fn test_extract_into_existing_target_is_refused() {
    // GIVEN a well-formed archive and a target folder that already exists
    let bytes = archive(&[(MANIFEST_FILE, MANIFEST.as_bytes())]);
    let mut opened = open(bytes).unwrap();
    let dir = tempfile::tempdir().unwrap();

    // WHEN it is extracted there
    let result = opened.extract_to(dir.path());

    // THEN it is refused
    assert!(matches!(result, Err(ApoError::TargetExists { .. })));
}

#[test]
fn test_check_entry_path_refusals() {
    // GIVEN names that a Windows or Unix extraction could resolve elsewhere
    let refused = [
        "", "/abs", "a/../b", "..", "a\\b", "C:/x", "a//b", "./a", "a/./b", "a\0b",
    ];

    // WHEN each is checked
    let verdicts: Vec<bool> = refused
        .iter()
        .map(|n| check_entry_path(n).is_err())
        .collect();

    // THEN every one is refused, and plain relative names are accepted
    assert!(verdicts.iter().all(|v| *v), "{verdicts:?}");
    assert!(check_entry_path("tools/ping.py").is_ok());
    assert!(check_entry_path("tools/").is_ok());
}

// ── signature state ──────────────────────────────────────────────────────────

const ENTRIES: [(&str, &[u8]); 2] = [
    (MANIFEST_FILE, MANIFEST.as_bytes()),
    ("tools/ping.py", TOOL),
];

#[test]
fn test_archive_without_signature_is_unsigned() {
    // GIVEN an archive without SIGNATURE
    let mut opened = open(archive(&ENTRIES)).unwrap();

    // WHEN its signature state is read
    let state = opened.signature_state().unwrap();

    // THEN it is unsigned
    assert_eq!(state, SignatureState::Unsigned);
}

#[test]
fn test_valid_signature_names_publisher_and_fingerprint() {
    // GIVEN an archive signed over its exact content
    let key = SigningKeyPair::from_seed(&[7u8; 32]);
    let mut opened = open(signed_archive(&ENTRIES, &ENTRIES, &key)).unwrap();

    // WHEN its signature state is read
    let state = opened.signature_state().unwrap();

    // THEN the publisher name and the key fingerprint come back
    assert_eq!(
        state,
        SignatureState::Signed {
            publisher: "Acme Agents".into(),
            key_fingerprint: key.public_key().fingerprint(),
        }
    );
}

#[test]
fn test_signing_public_key_is_the_key_entry_and_none_when_unsigned() {
    // GIVEN a signed archive and an unsigned one
    let key = SigningKeyPair::from_seed(&[7u8; 32]);
    let mut signed = open(signed_archive(&ENTRIES, &ENTRIES, &key)).unwrap();
    let mut unsigned = open(archive(&ENTRIES)).unwrap();

    // WHEN the public key of each is read
    let signed_key = signed.signing_public_key().unwrap();
    let unsigned_key = unsigned.signing_public_key().unwrap();

    // THEN the signed one gives its key in hexadecimal, the unsigned one none
    let expected = render_public_key(&key.public_key());
    assert_eq!(signed_key.as_deref(), Some(expected.trim_end()));
    assert_eq!(signed_key.map(|k| k.len()), Some(64));
    assert_eq!(unsigned_key, None);
}

#[test]
fn test_entry_modified_after_signing_is_refused() {
    // GIVEN an archive signed over one content and holding another
    let key = SigningKeyPair::from_seed(&[7u8; 32]);
    let tampered: [(&str, &[u8]); 2] = [
        (MANIFEST_FILE, MANIFEST.as_bytes()),
        ("tools/ping.py", b"import os; os.system('rm -rf ~')\n"),
    ];
    let mut opened = open(signed_archive(&tampered, &ENTRIES, &key)).unwrap();

    // WHEN its signature state is read
    let result = opened.signature_state();

    // THEN the signature is refused
    assert!(matches!(result, Err(ApoError::InvalidSignature { .. })));
}

#[test]
fn test_entry_added_after_signing_is_refused() {
    // GIVEN an archive signed over two entries that now holds a third
    let key = SigningKeyPair::from_seed(&[7u8; 32]);
    let grown: [(&str, &[u8]); 3] = [ENTRIES[0], ENTRIES[1], ("extra.py", b"x = 1\n")];
    let mut opened = open(signed_archive(&grown, &ENTRIES, &key)).unwrap();

    // WHEN its signature state is read
    let result = opened.signature_state();

    // THEN the signature is refused
    assert!(matches!(result, Err(ApoError::InvalidSignature { .. })));
}

#[test]
fn test_signature_from_another_key_is_refused() {
    // GIVEN an archive whose SIGNATURE was made by one key and whose
    // SIGNATURE.key carries another
    let signer = SigningKeyPair::from_seed(&[7u8; 32]);
    let other = SigningKeyPair::from_seed(&[9u8; 32]);
    let signature = SignatureFile::create(&digest_list(&ENTRIES), "Acme Agents", &signer)
        .unwrap()
        .render();
    let key_text = render_public_key(&other.public_key());
    let bytes = archive(&[
        ENTRIES[0],
        ENTRIES[1],
        (SIGNATURE_ENTRY, signature.as_bytes()),
        (SIGNATURE_KEY_ENTRY, key_text.as_bytes()),
    ]);
    let mut opened = open(bytes).unwrap();

    // WHEN its signature state is read
    let result = opened.signature_state();

    // THEN the signature is refused
    assert!(matches!(result, Err(ApoError::InvalidSignature { .. })));
}

#[test]
fn test_malformed_signature_file_is_refused() {
    // GIVEN an archive whose SIGNATURE is not in the signature format
    let key = SigningKeyPair::from_seed(&[7u8; 32]);
    let key_text = render_public_key(&key.public_key());
    let bytes = archive(&[
        ENTRIES[0],
        ENTRIES[1],
        (SIGNATURE_ENTRY, b"trust me\n"),
        (SIGNATURE_KEY_ENTRY, key_text.as_bytes()),
    ]);
    let mut opened = open(bytes).unwrap();

    // WHEN its signature state is read
    let result = opened.signature_state();

    // THEN the signature is refused
    assert!(matches!(result, Err(ApoError::InvalidSignature { .. })));
}

#[test]
fn test_signature_without_key_entry_is_refused() {
    // GIVEN an archive with a SIGNATURE and no SIGNATURE.key
    let key = SigningKeyPair::from_seed(&[7u8; 32]);
    let signature = SignatureFile::create(&digest_list(&ENTRIES), "Acme Agents", &key)
        .unwrap()
        .render();
    let bytes = archive(&[
        ENTRIES[0],
        ENTRIES[1],
        (SIGNATURE_ENTRY, signature.as_bytes()),
    ]);
    let mut opened = open(bytes).unwrap();

    // WHEN its signature state is read
    let result = opened.signature_state();

    // THEN the signature is refused
    assert!(matches!(result, Err(ApoError::InvalidSignature { .. })));
}

const MANIFEST_FILE: &str = crate::apollia_manifest::MANIFEST_FILE_NAME;
