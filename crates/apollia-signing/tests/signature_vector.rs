//! Frozen test vector for the digest list encoding and the `SIGNATURE` format.
//!
//! The key is the first test key of RFC 8032, section 7.1. The expected
//! canonical bytes and signature were computed outside this crate (Python
//! `hashlib` for the encoding, OpenSSL `pkeyutl -sign -rawin` for the
//! signature), so a change to either side of the format fails here.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use apollia_signing::{sha256, DigestList, SignatureFile, SigningKeyPair, Verdict};

const SEED_HEX: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
const PUBLIC_KEY_HEX: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
const CANONICAL_HEX: &str = concat!(
    "61706f6c6c69612d6469676573742d6c69737400763100",
    "0000000000000003",
    "00000008",
    "6167656e742e7079",
    "5d6943532bcacce45a14f1fcf2bbc1e43f86bf05ff0eac70d3ea18bd6e68c720",
    "0000000d",
    "6d616e69666573742e746f6d6c",
    "2364dc3e3b8f6ec3e7c237a77e86b3f85a06633f6d60cd78300dc62c639397d4",
    "0000000f",
    "736b696c6c732f68656c6c6f2e7079",
    "49b6a5a036b16656be0b47bcc2d1dde778342ac03f6a5009e177b5e301151fe4",
);
const PUBLISHER: &str = "Apollia Test Vector";

fn vector_key() -> SigningKeyPair {
    let mut seed = [0u8; 32];
    hex::decode_to_slice(SEED_HEX, &mut seed).unwrap();
    SigningKeyPair::from_seed(&seed)
}

fn vector_list() -> DigestList {
    let mut list = DigestList::new();
    list.insert(
        "skills/hello.py",
        sha256(b"def hello():\n    return 'hi'\n"),
    )
    .unwrap();
    list.insert("agent.py", sha256(b"class Demo:\n    pass\n"))
        .unwrap();
    list.insert("manifest.toml", sha256(b"name = \"demo\"\n"))
        .unwrap();
    list
}

fn committed_signature_file() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/vectors/ed25519/SIGNATURE");
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn test_vector_public_key_matches_rfc8032() {
    // GIVEN the RFC 8032 test seed
    let pair = vector_key();

    // WHEN its public half is derived
    let public_key = hex::encode(pair.public_key().to_bytes());

    // THEN it is the public key the RFC publishes
    assert_eq!(public_key, PUBLIC_KEY_HEX);
}

#[test]
fn test_vector_canonical_encoding_is_frozen() {
    // GIVEN the vector list
    let list = vector_list();

    // WHEN it is encoded
    let encoded = hex::encode(list.canonical_bytes());

    // THEN the bytes are exactly the committed ones
    assert_eq!(encoded, CANONICAL_HEX);
}

#[test]
fn test_vector_signature_file_is_frozen() {
    // GIVEN the vector key, list and publisher
    let file = SignatureFile::create(&vector_list(), PUBLISHER, &vector_key()).unwrap();

    // WHEN the signature file is rendered
    let rendered = file.render();

    // THEN it is byte-identical to the committed SIGNATURE file
    assert_eq!(rendered, committed_signature_file());
}

#[test]
fn test_vector_committed_file_verifies() {
    // GIVEN the committed SIGNATURE file, parsed
    let file = SignatureFile::parse(&committed_signature_file()).unwrap();

    // WHEN it is verified against the vector list and the vector public key
    let verdict = file.verify(&vector_list(), &vector_key().public_key());

    // THEN it holds and names the declared publisher
    assert_eq!(verdict, Verdict::Valid);
    assert_eq!(file.publisher(), PUBLISHER);
}
