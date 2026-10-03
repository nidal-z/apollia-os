//! What the dictation settings accept: the language hints and the model files.
//!
//! One definition for the routes of the daemon and for the desktop running the
//! runtime in its own process, so a setting refused in one mode is refused in
//! the other with the same words, and a model listed in one is listed in both.

use std::path::Path;

/// ISO 639-1 codes accepted as the dictation language hint.
///
/// A closed list rather than free text: the field used to accept anything an
/// operator typed, so the same language could be spelled several ways across
/// machines and a typo silently became auto-detection at the model. Documented
/// in `reference/configuration.md`, and offered verbatim by the settings picker.
pub const STT_LANGUAGES: [&str; 13] = [
    "fr", "en", "es", "de", "it", "pt", "nl", "pl", "ru", "zh", "ja", "ko", "ar",
];

/// File extensions the whisper model scan accepts.
///
/// Must stay in step with the import picker of the dictation settings, which
/// offers `bin` and `gguf`. Scanning for `.bin` alone meant an imported `.gguf`
/// was written into the configuration, loaded by the engine, and yet absent
/// from the list: the settings dropdown then had no option matching the
/// configured model and rendered blank.
pub const MODEL_EXTENSIONS: [&str; 2] = ["bin", "gguf"];

/// A language hint outside [`STT_LANGUAGES`].
#[derive(Debug, thiserror::Error)]
#[error(
    "unsupported dictation language '{code}': expected one of {expected}, or none for auto-detection",
    expected = STT_LANGUAGES.join(", ")
)]
pub struct UnsupportedLanguage {
    /// The code refused, trimmed.
    pub code: String,
}

/// Validates the language hint against [`STT_LANGUAGES`].
///
/// `None` and the empty string both mean auto-detection and are accepted.
///
/// # Errors
///
/// [`UnsupportedLanguage`] for any other code.
pub fn validate_language(language: Option<&str>) -> Result<(), UnsupportedLanguage> {
    let Some(code) = language.map(str::trim).filter(|c| !c.is_empty()) else {
        return Ok(());
    };
    if STT_LANGUAGES.contains(&code) {
        return Ok(());
    }
    Err(UnsupportedLanguage {
        code: code.to_owned(),
    })
}

/// Whether a path carries one of the [`MODEL_EXTENSIONS`], in any case.
pub fn is_model_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            let ext = ext.to_ascii_lowercase();
            MODEL_EXTENSIONS.contains(&ext.as_str())
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_hint_accepts_only_the_documented_codes() {
        // GIVEN the closed list documented in reference/configuration.md
        // WHEN each code is validated
        // THEN it is accepted
        for code in STT_LANGUAGES {
            assert!(validate_language(Some(code)).is_ok(), "{code} should pass");
        }
    }

    #[test]
    fn language_hint_rejects_free_text() {
        // GIVEN the kind of value the old free-text field allowed
        // WHEN it is validated
        let err = validate_language(Some("francais"))
            .expect_err("should reject")
            .to_string();
        // THEN it is refused, and the message names the accepted codes
        assert!(err.contains("'francais'"), "{err}");
        assert!(err.contains("fr, en"), "{err}");
    }

    #[test]
    fn absent_language_means_auto_detection() {
        // GIVEN no language, or an empty one left by a cleared field
        // WHEN validated
        // THEN both are accepted as auto-detection
        assert!(validate_language(None).is_ok());
        assert!(validate_language(Some("")).is_ok());
        assert!(validate_language(Some("   ")).is_ok());
    }

    #[test]
    fn model_scan_accepts_every_extension_the_import_picker_offers() {
        // GIVEN the extensions the settings import dialog lets an operator pick
        // WHEN the scan classifies a file of each
        // THEN it is listed, so the model select can show the configured value
        for ext in MODEL_EXTENSIONS {
            let path = std::path::PathBuf::from(format!("/tmp/models/whisper.{ext}"));
            assert!(is_model_file(&path), "{ext} should be scanned");
        }
        assert!(is_model_file(Path::new("/tmp/W.GGUF")));
    }

    #[test]
    fn model_scan_ignores_unrelated_files() {
        // GIVEN files that are not whisper models
        // WHEN the scan classifies them
        // THEN they are skipped
        assert!(!is_model_file(Path::new("/tmp/notes.txt")));
        assert!(!is_model_file(Path::new("/tmp/no-extension")));
    }
}
