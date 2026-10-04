//! The language data folder: what counts, what is imported, what is fetched.

use sha2::{Digest, Sha256};
#[cfg(feature = "cloud")]
use wiremock::matchers::{method, path};
#[cfg(feature = "cloud")]
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
#[cfg(feature = "cloud")]
use crate::downloader::{DownloadError, DownloadManager};

const FRENCH: &[u8] = b"language data, French";

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// A catalogue of one language, `fra`, pinned to the digest of `bytes`.
fn catalogue_of(bytes: &[u8]) -> OcrCatalogue {
    OcrCatalogue::from_toml(&format!(
        r#"
[[ocr]]
language = "fra"
label = "French"
publisher = "test"
released = "2017-09-14"
license = "apache-2.0"
repo = "owner/tessdata"
file = "fra.traineddata"
size_bytes = {}
sha256 = "{}"
source = "https://example.invalid"
"#,
        bytes.len(),
        digest(bytes)
    ))
    .expect("the test catalogue must load")
}

#[cfg(feature = "cloud")]
fn loopback_manager() -> DownloadManager {
    DownloadManager::with_client(
        apollia_core::net::loopback_client_builder()
            .build()
            .expect("test client"),
    )
}

#[test]
fn test_the_shipped_entries_load_and_cover_french_and_english() {
    // GIVEN the table shipped in the binary
    // WHEN its recognition entries are loaded
    let catalogue = OcrCatalogue::embedded().expect("the shipped entries must load");

    // THEN French and English are pinned, each to a file named after its code
    let codes: Vec<&str> = catalogue
        .languages()
        .iter()
        .map(|l| l.language.as_str())
        .collect();
    assert_eq!(codes, ["fra", "eng"]);
    assert!(catalogue
        .languages()
        .iter()
        .all(|l| l.file == l.file_name()));
}

#[test]
fn test_a_file_counts_only_with_the_pinned_digest() {
    // GIVEN a folder holding a file of the right name and altered content
    let dir = tempfile::tempdir().unwrap();
    let catalogue = catalogue_of(FRENCH);
    std::fs::write(dir.path().join("fra.traineddata"), b"altered").unwrap();

    // WHEN the verified languages are listed, before and after the file is
    // replaced by the pinned one
    let before = verified_languages(dir.path(), &catalogue);
    std::fs::write(dir.path().join("fra.traineddata"), FRENCH).unwrap();
    let after = verified_languages(dir.path(), &catalogue);

    // THEN only the pinned content counts
    assert!(before.is_empty());
    assert_eq!(after, ["fra"]);
}

#[test]
fn test_a_pinned_file_brought_on_a_disk_is_imported_under_its_language_name() {
    // GIVEN the pinned file on a key, under another name, and an empty folder
    let key = tempfile::tempdir().unwrap();
    let models = tempfile::tempdir().unwrap();
    let dir = tessdata_dir(models.path());
    let catalogue = catalogue_of(FRENCH);
    let source = key.path().join("copy of french.traineddata");
    std::fs::write(&source, FRENCH).unwrap();

    // WHEN it is imported
    let dest = import_language_data(&source, &dir, &catalogue).unwrap();

    // THEN it lands under the name the engine reads, and counts
    assert_eq!(dest, dir.join("fra.traineddata"));
    assert_eq!(verified_languages(&dir, &catalogue), ["fra"]);
    assert!(source.exists());
}

#[test]
fn test_a_file_the_registry_does_not_pin_is_refused_and_nothing_is_written() {
    // GIVEN a language data file whose digest the registry does not pin
    let key = tempfile::tempdir().unwrap();
    let models = tempfile::tempdir().unwrap();
    let dir = tessdata_dir(models.path());
    let source = key.path().join("fra.traineddata");
    std::fs::write(&source, b"something else").unwrap();

    // WHEN it is imported
    let refused = import_language_data(&source, &dir, &catalogue_of(FRENCH));

    // THEN it is refused and the folder is not even created
    assert!(matches!(refused, Err(OcrDataError::UnknownDigest { .. })));
    assert!(!dir.exists());
}

#[cfg(feature = "cloud")]
#[tokio::test]
async fn test_missing_data_is_fetched_from_the_registry_and_checked() {
    // GIVEN a registry serving the pinned file, and an empty folder
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/owner/tessdata/resolve/main/fra.traineddata"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(FRENCH))
        .expect(1)
        .mount(&server)
        .await;
    let models = tempfile::tempdir().unwrap();
    let dir = tessdata_dir(models.path());
    let catalogue = catalogue_of(FRENCH);
    let manager = loopback_manager();

    // WHEN the missing data is fetched, twice
    let first = fetch_missing_from(&dir, &catalogue, &manager, &server.uri())
        .await
        .unwrap();
    let second = fetch_missing_from(&dir, &catalogue, &manager, &server.uri())
        .await
        .unwrap();

    // THEN the file is fetched once, counts, and is not fetched again
    assert_eq!(first, ["fra"]);
    assert!(second.is_empty());
    assert_eq!(verified_languages(&dir, &catalogue), ["fra"]);
}

#[cfg(feature = "cloud")]
#[tokio::test]
async fn test_fetched_data_with_another_digest_is_refused_and_left_off_the_disk() {
    // GIVEN a registry serving other bytes than the pinned ones
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/owner/tessdata/resolve/main/fra.traineddata"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"tampered".to_vec()))
        .mount(&server)
        .await;
    let models = tempfile::tempdir().unwrap();
    let dir = tessdata_dir(models.path());
    let catalogue = catalogue_of(FRENCH);

    // WHEN the missing data is fetched
    let refused = fetch_missing_from(&dir, &catalogue, &loopback_manager(), &server.uri()).await;

    // THEN the download fails on the digest and no file is left
    assert!(matches!(
        refused,
        Err(OcrDataError::Download(
            DownloadError::IntegrityMismatch { .. }
        ))
    ));
    assert!(!dir.join("fra.traineddata").exists());
    assert!(verified_languages(&dir, &catalogue).is_empty());
}
