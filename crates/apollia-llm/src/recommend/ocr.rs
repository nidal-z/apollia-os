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
    /// Upstream repository carrying the file, `owner/name`.
    pub repo: String,
    /// Commit of `repo` the file is taken at, 40 lowercase hexadecimal digits:
    /// a fixed commit, so the bytes behind the address are the pinned ones.
    pub commit: String,
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
    /// SHA-256, whose commit is not a full commit hash, whose size is zero, or
    /// whose file is not `<language>.traineddata`.
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
            if !is_lower_hex(&entry.sha256, 64) {
                return Err(invalid("sha256 is not 64 lowercase hexadecimal digits"));
            }
            if !is_lower_hex(&entry.commit, 40) {
                return Err(invalid("commit is not 40 lowercase hexadecimal digits"));
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

fn is_lower_hex(text: &str, len: usize) -> bool {
    text.len() == len
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl OcrLanguage {
    /// The name the engine looks the file up by: `<language>.traineddata`.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("{}.traineddata", self.language)
    }

    /// The address the file is downloaded from, under `base`:
    /// `<base>/<repo>/<commit>/<file>`, the raw-file layout of the upstream
    /// host.
    #[must_use]
    pub fn download_url(&self, base: &str) -> String {
        format!(
            "{}/{}/{}/{}",
            base.trim_end_matches('/'),
            self.repo,
            self.commit,
            self.file
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_french_downloads_from_the_upstream_set_at_its_pinned_commit() {
        // GIVEN the French entry shipped in the binary
        let catalogue = OcrCatalogue::embedded().expect("the shipped entries must load");
        let french = catalogue
            .languages()
            .iter()
            .find(|l| l.language == "fra")
            .expect("French is pinned");

        // WHEN its address is composed under the shipped base
        let url = french.download_url("https://raw.githubusercontent.com");

        // THEN it is the upstream file at the commit whose digest is pinned
        assert_eq!(
            url,
            "https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/\
             923915d4ced2a7235221788285785a29c4a42d4a/fra.traineddata"
        );
    }

    #[test]
    fn test_an_entry_whose_commit_is_not_a_full_hash_is_refused() {
        // GIVEN an entry pinned to a branch name rather than a commit
        let text = r#"
[[ocr]]
language = "fra"
label = "French"
publisher = "test"
released = "2017-09-14"
license = "apache-2.0"
repo = "owner/tessdata"
commit = "main"
file = "fra.traineddata"
size_bytes = 1
sha256 = "ced037562e8c80c13122dece28dd477d399af80911a28791a66a63ac1e3445ca"
source = "https://example.invalid"
"#;

        // WHEN the table is loaded
        let refused = OcrCatalogue::from_toml(text);

        // THEN the entry is refused on its commit
        assert!(matches!(
            refused,
            Err(ManifestError::InvalidOcr { ref reason, .. }) if reason.contains("commit")
        ));
    }
}
