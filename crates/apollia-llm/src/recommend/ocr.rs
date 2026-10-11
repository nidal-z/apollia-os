//! The language data of optical character recognition, as the registry pins it.
//!
//! The engine that reads a scanned page is compiled into the binary; what it
//! reads with is one `<language>.traineddata` file per language. The `[[ocr]]`
//! entries of `pinned_models.toml` pin each of those files by its SHA-256, so it
//! downloads through the same verified path as any model, and a copy brought
//! from a disk is recognised by its digest. A file whose digest is not pinned
//! here is never handed to the engine.

use serde::{Deserialize, Serialize};

use super::manifest::{ManifestError, EMBEDDED_TOML};

/// The language data of one language, pinned to one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrLanguage {
    /// The language code the engine knows it by, for example `fra`.
    pub language: String,
    /// What the operator reads.
    pub label: String,
    /// The organisation that trained it.
    pub publisher: String,
    /// ISO date of its first public release.
    pub released: String,
    /// SPDX identifier.
    pub license: String,
    /// HuggingFace repository carrying the file, `owner/name`.
    pub repo: String,
    /// The file within the repository, `<language>.traineddata`.
    pub file: String,
    /// Size of the file in bytes.
    pub size_bytes: u64,
    /// SHA-256 of the file, lowercase hex.
    pub sha256: String,
    /// URL the facts were read from.
    pub source: String,
}

#[derive(Debug, Deserialize)]
struct RawOcr {
    #[serde(default, rename = "ocr")]
    languages: Vec<OcrLanguage>,
}

/// The optical character recognition category of the registry.
#[derive(Debug, Clone)]
pub struct OcrCatalogue {
    languages: Vec<OcrLanguage>,
}

impl OcrCatalogue {
    /// Load the entries embedded in the binary.
    ///
    /// # Errors
    /// See [`ManifestError`]; a failure is a defect in a shipped file.
    pub fn embedded() -> Result<Self, ManifestError> {
        Self::from_toml(EMBEDDED_TOML)
    }

    /// Load the entries of a TOML table.
    ///
    /// # Errors
    /// [`ManifestError::Parse`] when the table does not parse,
    /// [`ManifestError::DuplicateId`] when two entries share a language, and
    /// [`ManifestError::InvalidOcr`] for an entry whose digest is not a
    /// SHA-256, whose size is zero, or whose file is not
    /// `<language>.traineddata`.
    pub fn from_toml(text: &str) -> Result<Self, ManifestError> {
        let raw: RawOcr = toml::from_str(text)?;
        let mut seen = std::collections::BTreeSet::new();
        for entry in &raw.languages {
            if !seen.insert(entry.language.as_str()) {
                return Err(ManifestError::DuplicateId(entry.language.clone()));
            }
            let invalid = |reason: &str| ManifestError::InvalidOcr {
                language: entry.language.clone(),
                reason: reason.to_owned(),
            };
            let code_ok = !entry.language.is_empty()
                && entry.language.bytes().all(|b| b.is_ascii_lowercase());
            if !code_ok {
                return Err(invalid("language is not a lowercase ASCII code"));
            }
            let digest_ok = entry.sha256.len() == 64
                && entry
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
            if !digest_ok {
                return Err(invalid("sha256 is not 64 lowercase hexadecimal digits"));
            }
            if entry.size_bytes == 0 {
                return Err(invalid("size_bytes is zero"));
            }
            if entry.file != entry.file_name() {
                return Err(invalid("file is not <language>.traineddata"));
            }
        }
        Ok(Self {
            languages: raw.languages,
        })
    }

    /// Every entry, in file order.
    #[must_use]
    pub fn languages(&self) -> &[OcrLanguage] {
        &self.languages
    }

    /// The entry whose pinned file has this SHA-256, in any case.
    #[must_use]
    pub fn by_sha256(&self, sha256: &str) -> Option<&OcrLanguage> {
        self.languages
            .iter()
            .find(|l| l.sha256.eq_ignore_ascii_case(sha256.trim()))
    }
}

impl OcrLanguage {
    /// The name the engine looks the file up by: `<language>.traineddata`.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("{}.traineddata", self.language)
    }

    /// The address the file is downloaded from, under `base`.
    #[must_use]
    pub fn download_url(&self, base: &str) -> String {
        format!(
            "{}/{}/resolve/main/{}",
            base.trim_end_matches('/'),
            self.repo,
            self.file
        )
    }
}
