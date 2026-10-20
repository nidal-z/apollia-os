//! Reading an `.apo` archive: manifest, entry list, signature, extraction.

use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::Path;

use apollia_signing::{sha256, DigestList, PublicKey, SignatureFile, Verdict};
use zip::ZipArchive;

use super::{
    check_entry_path, ApoError, ArchiveLimits, SignatureState, SIGNATURE_ENTRY, SIGNATURE_KEY_ENTRY,
};
use crate::apollia_manifest::{self, ApolliaManifest, MANIFEST_FILE_NAME};

/// Largest `SIGNATURE` or `SIGNATURE.key` entry read, in bytes. The version 1
/// signature file is under 600 bytes.
const MAX_SIGNATURE_BYTES: u64 = 4096;

/// A file entry of an archive, as its central directory declares it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EntryInfo {
    /// Path relative to the package root, `/`-separated.
    pub path: String,
    /// Declared decompressed size, in bytes.
    pub size: u64,
}

/// An opened `.apo` archive whose entry names and manifest have been checked.
///
/// Opening reads the central directory and the `apollia.toml` entry, and
/// nothing else: no other entry is decompressed until [`Self::signature_state`]
/// or [`Self::extract_to`] is called.
pub struct ApoArchive<R: Read + Seek> {
    zip: ZipArchive<R>,
    manifest: ApolliaManifest,
    manifest_text: String,
    entries: Vec<EntryInfo>,
    limits: ArchiveLimits,
}

impl<R: Read + Seek> std::fmt::Debug for ApoArchive<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApoArchive")
            .field("package", &self.manifest.package.name)
            .field("entries", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl ApoArchive<File> {
    /// Opens the archive at `path` with the default [`ArchiveLimits`].
    ///
    /// # Errors
    ///
    /// See [`ApoArchive::from_reader`]; [`ApoError::Io`] when the file cannot
    /// be opened.
    pub fn open(path: &Path) -> Result<Self, ApoError> {
        Self::open_with_limits(path, ArchiveLimits::default())
    }

    /// Opens the archive at `path` with explicit `limits`.
    ///
    /// # Errors
    ///
    /// See [`ApoArchive::from_reader`]; [`ApoError::Io`] when the file cannot
    /// be opened.
    pub fn open_with_limits(path: &Path, limits: ArchiveLimits) -> Result<Self, ApoError> {
        let io = |source| ApoError::Io {
            path: path.to_path_buf(),
            source,
        };
        let file = File::open(path).map_err(io)?;
        let len = file.metadata().map_err(io)?.len();
        if len > limits.max_archive_bytes {
            return Err(ApoError::LimitExceeded {
                limit: "archive size",
                max: limits.max_archive_bytes,
            });
        }
        Self::from_reader(file, limits)
    }
}

impl<R: Read + Seek> ApoArchive<R> {
    /// Reads the central directory of `reader`, checks every entry name, and
    /// reads and validates `apollia.toml`.
    ///
    /// # Errors
    ///
    /// [`ApoError::Zip`] when the input is not a zip archive;
    /// [`ApoError::LimitExceeded`] when the entry count or a declared size
    /// crosses `limits`; [`ApoError::UnsafePath`], [`ApoError::Symlink`] or
    /// [`ApoError::DuplicateEntry`] for an entry that cannot be extracted
    /// safely; [`ApoError::MissingManifest`], [`ApoError::ManifestEncoding`]
    /// or [`ApoError::Manifest`] when the manifest is absent or invalid.
    pub fn from_reader(reader: R, limits: ArchiveLimits) -> Result<Self, ApoError> {
        let mut zip = ZipArchive::new(reader)?;
        if zip.len() > limits.max_entries {
            return Err(ApoError::LimitExceeded {
                limit: "entry count",
                max: limits.max_entries as u64,
            });
        }

        let mut seen = HashSet::new();
        let mut entries = Vec::new();
        let mut declared_total: u64 = 0;
        for index in 0..zip.len() {
            let entry = zip.by_index_raw(index)?;
            let name = entry.name().to_owned();
            check_entry_path(&name)?;
            if entry.is_symlink() {
                return Err(ApoError::Symlink { path: name });
            }
            let key = name.trim_end_matches('/').to_owned();
            if !seen.insert(key) {
                return Err(ApoError::DuplicateEntry { path: name });
            }
            if entry.is_dir() {
                continue;
            }
            let size = entry.size();
            if size > limits.max_entry_bytes {
                return Err(ApoError::LimitExceeded {
                    limit: "entry size",
                    max: limits.max_entry_bytes,
                });
            }
            declared_total = declared_total.saturating_add(size);
            if declared_total > limits.max_total_bytes {
                return Err(ApoError::LimitExceeded {
                    limit: "decompressed size",
                    max: limits.max_total_bytes,
                });
            }
            entries.push(EntryInfo { path: name, size });
        }

        let bytes = match zip.by_name(MANIFEST_FILE_NAME) {
            Ok(mut file) => read_bounded(&mut file, limits.max_manifest_bytes, "manifest size")?,
            Err(zip::result::ZipError::FileNotFound) => return Err(ApoError::MissingManifest),
            Err(other) => return Err(other.into()),
        };
        let manifest_text = String::from_utf8(bytes).map_err(|_| ApoError::ManifestEncoding)?;
        let manifest = apollia_manifest::parse(&manifest_text)?;

        Ok(Self {
            zip,
            manifest,
            manifest_text,
            entries,
            limits,
        })
    }

    /// The validated manifest.
    pub fn manifest(&self) -> &ApolliaManifest {
        &self.manifest
    }

    /// The manifest as stored in the archive.
    pub fn manifest_text(&self) -> &str {
        &self.manifest_text
    }

    /// The file entries, in archive order, directories left out.
    pub fn entries(&self) -> &[EntryInfo] {
        &self.entries
    }

    /// Checks the signature of the archive against its content.
    ///
    /// Every entry other than the two signature entries is decompressed and
    /// hashed, under the archive's limits. The signature is checked against
    /// the key carried in `SIGNATURE.key`, whose fingerprint must be the one
    /// the signature file names.
    ///
    /// # Errors
    ///
    /// [`ApoError::InvalidSignature`] when the signature entries are
    /// malformed or the signature does not hold over the current content;
    /// [`ApoError::LimitExceeded`] or [`ApoError::Zip`] while reading entries.
    pub fn signature_state(&mut self) -> Result<SignatureState, ApoError> {
        let has_signature = self.has_entry(SIGNATURE_ENTRY);
        let has_key = self.has_entry(SIGNATURE_KEY_ENTRY);
        match (has_signature, has_key) {
            (false, false) => return Ok(SignatureState::Unsigned),
            (true, false) => return Err(invalid("SIGNATURE without SIGNATURE.key")),
            (false, true) => return Err(invalid("SIGNATURE.key without SIGNATURE")),
            (true, true) => {}
        }

        let signature_text = self.read_small_text(SIGNATURE_ENTRY)?;
        let signature =
            SignatureFile::parse(&signature_text).map_err(|e| invalid(&e.to_string()))?;
        let key_text = self.read_small_text(SIGNATURE_KEY_ENTRY)?;
        let public_key = parse_public_key(&key_text)?;

        let list = self.digest_list()?;
        match signature.verify(&list, &public_key) {
            Verdict::Valid => Ok(SignatureState::Signed {
                publisher: signature.publisher().to_owned(),
                key_fingerprint: signature.key_fingerprint().to_owned(),
            }),
            Verdict::Invalid => Err(invalid(
                "the signature does not match the content of the package",
            )),
        }
    }

    /// The content digest of the archive: [`super::content_digest`] over
    /// every file entry except the two signature entries, so signing a
    /// package does not change it.
    ///
    /// # Errors
    ///
    /// [`ApoError::LimitExceeded`] or [`ApoError::Zip`] while reading entries.
    pub fn content_digest(&mut self) -> Result<String, ApoError> {
        Ok(super::digest_of(&self.digest_list()?))
    }

    /// The public key that travels with the signature, as 64 lowercase
    /// hexadecimal characters; `None` for an archive without `SIGNATURE.key`.
    ///
    /// The key alone proves nothing: [`Self::signature_state`] says whether
    /// the signature it checks holds. It is what a reader offers to trust
    /// when the user decides to trust the publisher of a package.
    ///
    /// # Errors
    ///
    /// [`ApoError::InvalidSignature`] when the key entry is malformed;
    /// [`ApoError::LimitExceeded`] or [`ApoError::Zip`] while reading it.
    pub fn signing_public_key(&mut self) -> Result<Option<String>, ApoError> {
        if !self.has_entry(SIGNATURE_KEY_ENTRY) {
            return Ok(None);
        }
        let text = self.read_small_text(SIGNATURE_KEY_ENTRY)?;
        let key = parse_public_key(&text)?;
        Ok(Some(render_public_key(&key).trim_end().to_owned()))
    }

    /// Extracts every entry under `target`, which must not exist yet.
    ///
    /// Entry names were checked when the archive was opened and are checked
    /// again here; the size limits apply to the bytes actually decompressed.
    /// Files are created without the permissions the archive may carry. On
    /// failure, the partially written `target` is removed.
    ///
    /// Extraction does not check the signature: call
    /// [`Self::signature_state`] first.
    ///
    /// # Errors
    ///
    /// [`ApoError::TargetExists`], [`ApoError::Io`], and any refusal of
    /// [`ApoArchive::from_reader`] or of the size limits.
    pub fn extract_to(&mut self, target: &Path) -> Result<(), ApoError> {
        if target.exists() {
            return Err(ApoError::TargetExists {
                path: target.to_path_buf(),
            });
        }
        let result = self.extract_inner(target);
        if result.is_err() {
            let _ = std::fs::remove_dir_all(target);
        }
        result
    }

    fn extract_inner(&mut self, target: &Path) -> Result<(), ApoError> {
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |source| ApoError::Io { path, source }
        };
        std::fs::create_dir_all(target).map_err(io(target))?;
        let mut total: u64 = 0;
        for index in 0..self.zip.len() {
            let mut entry = self.zip.by_index(index)?;
            let name = entry.name().to_owned();
            check_entry_path(&name)?;
            if entry.is_symlink() {
                return Err(ApoError::Symlink { path: name });
            }
            let dest = target.join(name.trim_end_matches('/'));
            if entry.is_dir() {
                std::fs::create_dir_all(&dest).map_err(io(&dest))?;
                continue;
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(io(parent))?;
            }
            let bytes = read_bounded(&mut entry, self.limits.max_entry_bytes, "entry size")?;
            total = total.saturating_add(bytes.len() as u64);
            if total > self.limits.max_total_bytes {
                return Err(ApoError::LimitExceeded {
                    limit: "decompressed size",
                    max: self.limits.max_total_bytes,
                });
            }
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dest)
                .map_err(io(&dest))?;
            file.write_all(&bytes).map_err(io(&dest))?;
        }
        Ok(())
    }

    fn has_entry(&self, name: &str) -> bool {
        self.entries.iter().any(|e| e.path == name)
    }

    fn read_small_text(&mut self, name: &str) -> Result<String, ApoError> {
        let mut file = self.zip.by_name(name)?;
        let bytes = read_bounded(&mut file, MAX_SIGNATURE_BYTES, "signature size")?;
        String::from_utf8(bytes).map_err(|_| invalid("signature entry is not UTF-8"))
    }

    /// The digest list of every file entry except the two signature entries.
    fn digest_list(&mut self) -> Result<DigestList, ApoError> {
        let mut list = DigestList::new();
        let mut total: u64 = 0;
        let names: Vec<String> = self
            .entries
            .iter()
            .map(|e| e.path.clone())
            .filter(|p| p != SIGNATURE_ENTRY && p != SIGNATURE_KEY_ENTRY)
            .collect();
        for name in names {
            let mut file = self.zip.by_name(&name)?;
            let bytes = read_bounded(&mut file, self.limits.max_entry_bytes, "entry size")?;
            total = total.saturating_add(bytes.len() as u64);
            if total > self.limits.max_total_bytes {
                return Err(ApoError::LimitExceeded {
                    limit: "decompressed size",
                    max: self.limits.max_total_bytes,
                });
            }
            list.insert(name, sha256(&bytes))
                .map_err(|e| invalid(&e.to_string()))?;
        }
        Ok(list)
    }
}

/// Reads at most `max` bytes from `reader`; one byte more is a refusal.
pub(super) fn read_bounded(
    reader: &mut impl Read,
    max: u64,
    limit: &'static str,
) -> Result<Vec<u8>, ApoError> {
    let mut buf = Vec::new();
    reader
        .take(max.saturating_add(1))
        .read_to_end(&mut buf)
        .map_err(|e| ApoError::Zip {
            message: e.to_string(),
        })?;
    if buf.len() as u64 > max {
        return Err(ApoError::LimitExceeded { limit, max });
    }
    Ok(buf)
}

/// Parses the content of `SIGNATURE.key`: 64 lowercase hex characters and a
/// line feed.
pub(super) fn parse_public_key(text: &str) -> Result<PublicKey, ApoError> {
    let hex_key = text
        .strip_suffix('\n')
        .ok_or_else(|| invalid("SIGNATURE.key lacks its final line feed"))?;
    if hex_key.len() != 64
        || !hex_key
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(invalid(
            "SIGNATURE.key is not 64 lowercase hexadecimal characters",
        ));
    }
    let mut bytes = [0u8; 32];
    for (i, chunk) in hex_key.as_bytes().chunks(2).enumerate() {
        let pair = std::str::from_utf8(chunk).map_err(|_| invalid("SIGNATURE.key is not hex"))?;
        bytes[i] = u8::from_str_radix(pair, 16).map_err(|_| invalid("SIGNATURE.key is not hex"))?;
    }
    PublicKey::from_bytes(&bytes).map_err(|e| invalid(&e.to_string()))
}

/// Renders a public key as the content of `SIGNATURE.key`.
pub(super) fn render_public_key(key: &PublicKey) -> String {
    let mut out = String::with_capacity(65);
    for byte in key.to_bytes() {
        out.push_str(&format!("{byte:02x}"));
    }
    out.push('\n');
    out
}

fn invalid(reason: &str) -> ApoError {
    ApoError::InvalidSignature {
        reason: reason.to_owned(),
    }
}
