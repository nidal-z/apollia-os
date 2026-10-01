//! The versioned `SIGNATURE` file that carries a detached signature.

use crate::digest_list::DigestList;
use crate::error::SigningError;
use crate::keys::{self, PublicKey, Signature, SigningKeyPair, Verdict};

/// The format version this build writes and reads.
pub const SIGNATURE_FORMAT_VERSION: u32 = 1;

/// Longest publisher name accepted, in characters.
const MAX_PUBLISHER_CHARS: usize = 128;

/// A parsed `SIGNATURE` file.
///
/// # Format, version 1
///
/// UTF-8 text, four lines, each ended by a line feed, in this order:
///
/// ```text
/// apollia-signature: 1
/// key-fingerprint: <64 lowercase hex characters>
/// publisher: <declared publisher name>
/// signature: <128 lowercase hex characters>
/// ```
///
/// - `apollia-signature` is the format version. It comes first so a reader
///   can refuse a version it does not know before reading anything else.
/// - `key-fingerprint` is [`PublicKey::fingerprint`] of the signing key.
/// - `publisher` is the name the author declared, one line, 1 to 128
///   characters, no control character, no surrounding whitespace.
/// - `signature` is the 64-byte Ed25519 signature over
///   [`DigestList::canonical_bytes`].
///
/// The signature covers the digest list only. The publisher name is a
/// declaration: what identifies the author is the key fingerprint, and the
/// name a reader can trust is the one it has associated with that fingerprint.
///
/// The format is frozen by a committed test vector: a fixed seed, a fixed
/// list, and the exact file they produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureFile {
    key_fingerprint: String,
    publisher: String,
    signature: Signature,
}

impl SignatureFile {
    /// Signs `list` with `key` and records the declared `publisher`.
    ///
    /// # Errors
    ///
    /// Returns [`SigningError::InvalidPublisher`] when `publisher` cannot be
    /// written on one line of the format.
    pub fn create(
        list: &DigestList,
        publisher: &str,
        key: &SigningKeyPair,
    ) -> Result<Self, SigningError> {
        validate_publisher(publisher)?;
        Ok(Self {
            key_fingerprint: key.public_key().fingerprint(),
            publisher: publisher.to_owned(),
            signature: keys::sign(list, key),
        })
    }

    /// Returns the fingerprint of the key that signed.
    pub fn key_fingerprint(&self) -> &str {
        &self.key_fingerprint
    }

    /// Returns the declared publisher name.
    pub fn publisher(&self) -> &str {
        &self.publisher
    }

    /// Returns the detached signature.
    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    /// Checks the signature against `list` and `public_key`.
    ///
    /// The verdict is [`Verdict::Invalid`] when the file names another key
    /// than `public_key`, or when the signature does not hold.
    pub fn verify(&self, list: &DigestList, public_key: &PublicKey) -> Verdict {
        if public_key.fingerprint() != self.key_fingerprint {
            return Verdict::Invalid;
        }
        keys::verify(list, &self.signature, public_key)
    }

    /// Writes the file in the version 1 format.
    pub fn render(&self) -> String {
        format!(
            "apollia-signature: {SIGNATURE_FORMAT_VERSION}\nkey-fingerprint: {}\npublisher: {}\nsignature: {}\n",
            self.key_fingerprint,
            self.publisher,
            hex::encode(self.signature.to_bytes()),
        )
    }

    /// Reads a file in the version 1 format.
    ///
    /// # Errors
    ///
    /// Returns [`SigningError::UnsupportedVersion`] when the first line names
    /// another version, [`SigningError::InvalidPublisher`] when the publisher
    /// line breaks the format, and [`SigningError::MalformedSignature`] for any
    /// other deviation from the format.
    pub fn parse(text: &str) -> Result<Self, SigningError> {
        let body = text
            .strip_suffix('\n')
            .ok_or(SigningError::MalformedSignature {
                reason: "missing final line feed",
            })?;
        let mut lines = body.split('\n');

        let version = field(lines.next(), "apollia-signature")?;
        if version != SIGNATURE_FORMAT_VERSION.to_string() {
            return Err(SigningError::UnsupportedVersion {
                version: version.to_owned(),
            });
        }
        let key_fingerprint = field(lines.next(), "key-fingerprint")?;
        let publisher = field(lines.next(), "publisher")?;
        let signature_hex = field(lines.next(), "signature")?;
        if lines.next().is_some() {
            return Err(SigningError::MalformedSignature {
                reason: "unexpected line after signature",
            });
        }

        if !is_lower_hex(key_fingerprint, 64) {
            return Err(SigningError::MalformedSignature {
                reason: "key fingerprint is not 64 lowercase hex characters",
            });
        }
        validate_publisher(publisher)?;
        if !is_lower_hex(signature_hex, 128) {
            return Err(SigningError::MalformedSignature {
                reason: "signature is not 128 lowercase hex characters",
            });
        }
        let mut bytes = [0u8; 64];
        hex::decode_to_slice(signature_hex, &mut bytes).map_err(|_| {
            SigningError::MalformedSignature {
                reason: "signature is not hexadecimal",
            }
        })?;

        Ok(Self {
            key_fingerprint: key_fingerprint.to_owned(),
            publisher: publisher.to_owned(),
            signature: Signature::from_bytes(bytes),
        })
    }
}

fn field<'a>(line: Option<&'a str>, name: &'static str) -> Result<&'a str, SigningError> {
    let line = line.ok_or(SigningError::MalformedSignature {
        reason: "missing line",
    })?;
    line.strip_prefix(name)
        .and_then(|rest| rest.strip_prefix(": "))
        .ok_or(SigningError::MalformedSignature {
            reason: "unexpected field name or order",
        })
}

fn is_lower_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn validate_publisher(name: &str) -> Result<(), SigningError> {
    if name.is_empty() {
        return Err(SigningError::InvalidPublisher {
            reason: "empty name",
        });
    }
    if name.chars().count() > MAX_PUBLISHER_CHARS {
        return Err(SigningError::InvalidPublisher {
            reason: "longer than 128 characters",
        });
    }
    if name.chars().any(char::is_control) {
        return Err(SigningError::InvalidPublisher {
            reason: "contains a control character",
        });
    }
    if name.trim() != name {
        return Err(SigningError::InvalidPublisher {
            reason: "leading or trailing whitespace",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::digest_list::sha256;

    fn sample() -> (DigestList, SigningKeyPair) {
        let mut list = DigestList::new();
        list.insert("agent.py", sha256(b"body")).unwrap();
        (list, SigningKeyPair::from_seed(&[2u8; 32]))
    }

    #[test]
    fn test_render_then_parse_round_trip() {
        // GIVEN a signature file over a list
        let (list, pair) = sample();
        let file = SignatureFile::create(&list, "Acme Agents", &pair).unwrap();

        // WHEN it is rendered and parsed back
        let parsed = SignatureFile::parse(&file.render()).unwrap();

        // THEN the parsed file is identical and still verifies
        assert_eq!(parsed, file);
        assert_eq!(parsed.verify(&list, &pair.public_key()), Verdict::Valid);
    }

    #[test]
    fn test_verify_against_other_key_is_invalid() {
        // GIVEN a signature file made by one key
        let (list, pair) = sample();
        let file = SignatureFile::create(&list, "Acme", &pair).unwrap();

        // WHEN it is checked against another key
        let other = SigningKeyPair::from_seed(&[4u8; 32]).public_key();

        // THEN the verdict is invalid
        assert_eq!(file.verify(&list, &other), Verdict::Invalid);
    }

    #[test]
    fn test_parse_unknown_version_is_unsupported() {
        // GIVEN a file that declares version 2
        let (list, pair) = sample();
        let text = SignatureFile::create(&list, "Acme", &pair)
            .unwrap()
            .render()
            .replacen("apollia-signature: 1", "apollia-signature: 2", 1);

        // WHEN it is parsed
        let result = SignatureFile::parse(&text);

        // THEN the version is reported as unsupported
        assert!(
            matches!(result, Err(SigningError::UnsupportedVersion { version }) if version == "2")
        );
    }

    #[test]
    fn test_parse_reordered_fields_is_malformed() {
        // GIVEN a file whose publisher and fingerprint lines are swapped
        let (list, pair) = sample();
        let rendered = SignatureFile::create(&list, "Acme", &pair)
            .unwrap()
            .render();
        let lines: Vec<&str> = rendered.lines().collect();
        let text = format!("{}\n{}\n{}\n{}\n", lines[0], lines[2], lines[1], lines[3]);

        // WHEN it is parsed
        let result = SignatureFile::parse(&text);

        // THEN it is refused as malformed
        assert!(matches!(
            result,
            Err(SigningError::MalformedSignature { .. })
        ));
    }

    #[test]
    fn test_parse_short_signature_is_malformed() {
        // GIVEN a file whose signature lost its last byte
        let (list, pair) = sample();
        let rendered = SignatureFile::create(&list, "Acme", &pair)
            .unwrap()
            .render();
        let text = format!("{}\n", &rendered[..rendered.len() - 3]);

        // WHEN it is parsed
        let result = SignatureFile::parse(&text);

        // THEN it is refused as malformed
        assert!(matches!(
            result,
            Err(SigningError::MalformedSignature { .. })
        ));
    }

    #[test]
    fn test_parse_missing_final_line_feed_is_malformed() {
        // GIVEN a rendered file without its final line feed
        let (list, pair) = sample();
        let rendered = SignatureFile::create(&list, "Acme", &pair)
            .unwrap()
            .render();

        // WHEN it is parsed
        let result = SignatureFile::parse(rendered.trim_end());

        // THEN it is refused as malformed
        assert!(matches!(
            result,
            Err(SigningError::MalformedSignature { .. })
        ));
    }

    #[test]
    fn test_create_with_multiline_publisher_is_refused() {
        // GIVEN a publisher name that would inject a line
        let (list, pair) = sample();

        // WHEN a signature file is created with it
        let result = SignatureFile::create(&list, "Acme\nsignature: 00", &pair);

        // THEN the name is refused
        assert!(matches!(result, Err(SigningError::InvalidPublisher { .. })));
    }

    #[test]
    fn test_create_with_empty_or_padded_publisher_is_refused() {
        // GIVEN an empty name and a name with trailing whitespace
        let (list, pair) = sample();

        // WHEN signature files are created with them
        let empty = SignatureFile::create(&list, "", &pair);
        let padded = SignatureFile::create(&list, "Acme ", &pair);

        // THEN both are refused
        assert!(matches!(empty, Err(SigningError::InvalidPublisher { .. })));
        assert!(matches!(padded, Err(SigningError::InvalidPublisher { .. })));
    }
}
