//! Documents of a message: decoded, copied, read, and written into the
//! message as text or as a reference.

use std::path::{Path, PathBuf};

use base64::Engine as _;

use super::*;

fn reader_fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../apollia-sources/tests/fixtures")
        .join(relative)
}

fn attachment(name: &str, bytes: &[u8]) -> DocumentAttachment {
    DocumentAttachment {
        name: name.to_owned(),
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
    }
}

fn decoded(name: &str, bytes: &[u8]) -> MessageDocument {
    decode_attachments(&[attachment(name, bytes)])
        .expect("decoded")
        .remove(0)
}

fn invoice() -> MessageDocument {
    let bytes = std::fs::read(reader_fixture("pdf/second-page-blank.pdf")).expect("fixture");
    decoded("facture.pdf", &bytes)
}

/// A CSV file of more than `INLINE_MAX_CHARS` characters.
fn long_csv() -> Vec<u8> {
    let mut text = String::from("reference;montant;libelle\n");
    for row in 0..4000 {
        text.push_str(&format!(
            "REF-{row:05};{row};prestation de conseil numero {row}\n"
        ));
    }
    assert!(text.chars().count() > INLINE_MAX_CHARS);
    text.into_bytes()
}

#[test]
fn test_a_short_document_travels_in_the_message_after_its_name() {
    // GIVEN a PDF of three pages, far under the inline ceiling
    let dir = tempfile::tempdir().expect("tempdir");
    let document = invoice();

    // WHEN it is received
    let (stored, blocks) = receive(dir.path(), &[document]).expect("received");

    // THEN the block names the file before its text, page by page, and the
    // metadata says the text is inline
    let name_at = blocks.find("name=\"facture.pdf\"").expect("the name");
    let text_at = blocks.find("Facture n° 2026-001").expect("the text");
    assert!(name_at < text_at, "{blocks}");
    assert!(blocks.contains("pages=\"3\""), "{blocks}");
    assert!(blocks.contains("--- page 3 ---"), "{blocks}");
    assert!(!blocks.contains("path="), "{blocks}");
    assert_eq!(stored.len(), 1);
    assert!(stored[0].inline);
    assert_eq!((stored[0].unit.as_str(), stored[0].parts), ("page", 3));
    assert!(dir.path().join(&stored[0].file).is_file());
}

#[test]
fn test_a_long_document_travels_as_its_name_its_size_and_a_reference() {
    // GIVEN a document of more than 100 000 characters
    let dir = tempfile::tempdir().expect("tempdir");
    let bytes = long_csv();
    let document = decoded("lignes.csv", &bytes);

    // WHEN it is received
    let (stored, blocks) = receive(dir.path(), &[document]).expect("received");

    // THEN the block carries the name, the size and the path of the copy,
    // names the tool that reads it, and holds none of the text
    let copy = dir.path().join(&stored[0].file);
    assert!(blocks.contains("name=\"lignes.csv\""), "{blocks}");
    assert!(
        blocks.contains(&format!("size=\"{}\"", bytes.len())),
        "{blocks}"
    );
    assert!(
        blocks.contains(&format!("path=\"{}\"", copy.display())),
        "{blocks}"
    );
    assert!(blocks.contains("file_read"), "{blocks}");
    assert!(!blocks.contains("REF-00042"), "{blocks}");
    assert!(!stored[0].inline);
    assert!(stored[0].characters > INLINE_MAX_CHARS as u64);
    assert_eq!(std::fs::read(copy).expect("the copy"), bytes);
}

#[test]
fn test_a_document_at_the_ceiling_is_inline_and_one_character_more_is_not() {
    // GIVEN a CSV file of exactly 100 000 characters, and one of 100 001
    let dir = tempfile::tempdir().expect("tempdir");
    let at = "a".repeat(INLINE_MAX_CHARS);
    let over = "a".repeat(INLINE_MAX_CHARS + 1);

    // WHEN both are received
    let (stored, _) = receive(
        dir.path(),
        &[
            decoded("at.csv", at.as_bytes()),
            decoded("over.csv", over.as_bytes()),
        ],
    )
    .expect("received");

    // THEN the first travels inline and the second as a reference
    assert_eq!(stored[0].characters, INLINE_MAX_CHARS as u64);
    assert!(stored[0].inline);
    assert!(!stored[1].inline);
}

#[test]
fn test_a_document_its_reader_cannot_open_is_refused_and_nothing_is_kept() {
    // GIVEN a readable PDF followed by one protected by a password
    let dir = tempfile::tempdir().expect("tempdir");
    let locked = std::fs::read(reader_fixture("pdf/user-password.pdf")).expect("fixture");
    let documents = [invoice(), decoded("secret.pdf", &locked)];

    // WHEN they are received
    let error = receive(dir.path(), &documents).expect_err("refused");

    // THEN the refusal names the file and the cause, and no copy remains
    match error {
        ChatError::DocumentRejected(reason) => {
            assert!(reason.contains("secret.pdf"), "{reason}");
            assert!(reason.contains("password"), "{reason}");
        }
        other => panic!("expected DocumentRejected, got {other:?}"),
    }
    assert_eq!(std::fs::read_dir(dir.path()).expect("dir").count(), 0);
}

#[test]
fn test_attachments_past_a_limit_are_refused_with_the_limit() {
    // GIVEN five documents, one of a format no reader opens, an empty one,
    // one that is not base64 and one a byte over the ceiling
    let five: Vec<DocumentAttachment> = (0..5)
        .map(|i| attachment(&format!("d{i}.pdf"), b"%PDF"))
        .collect();
    let archive = [attachment("archive.zip", b"PK")];
    let empty = [attachment("blank.pdf", b"")];
    let garbled = [DocumentAttachment {
        name: "a.pdf".into(),
        data: "not base64 !".into(),
    }];
    let heavy = [attachment("lourd.pdf", &vec![0u8; MAX_DOCUMENT_BYTES + 1])];

    // WHEN each is decoded
    let reasons: Vec<String> = [&five[..], &archive, &empty, &garbled, &heavy]
        .iter()
        .map(|attachments| match decode_attachments(attachments) {
            Err(ChatError::DocumentRejected(reason)) => reason,
            other => panic!("expected DocumentRejected, got {other:?}"),
        })
        .collect();

    // THEN each refusal names its cause
    assert!(reasons[0].contains("at most 4"), "{}", reasons[0]);
    assert!(
        reasons[1].contains("archive.zip: not a format"),
        "{}",
        reasons[1]
    );
    assert!(reasons[2].contains("empty"), "{}", reasons[2]);
    assert!(reasons[3].contains("base64"), "{}", reasons[3]);
    assert!(reasons[4].contains("at most"), "{}", reasons[4]);
}

#[test]
fn test_a_name_cannot_leave_the_folder_nor_break_the_tag() {
    // GIVEN a name with a parent path, a quote and an angle bracket
    let document = decoded("../../etc/rap\"port<1>.pdf", b"%PDF");

    // WHEN its name is read
    let name = document.name();

    // THEN it is the last component, without the characters a tag ends on
    assert_eq!(name, "rap_port_1_.pdf");
}

#[test]
fn test_documents_join_the_images_in_the_metadata_of_their_message() {
    // GIVEN the metadata of a message that already lists an image
    let mut metadata = Some(serde_json::json!({ "images": [{ "file": "m-0.png" }] }));
    let documents = MessageDocuments {
        stored: vec![StoredDocument {
            file: "ab12cd34-facture.pdf".into(),
            name: "facture.pdf".into(),
            size: 10,
            unit: "page".into(),
            parts: 3,
            characters: 120,
            inline: true,
        }],
        text_end: 17,
    };

    // WHEN the documents are written into it
    documents.write_into(&mut metadata);

    // THEN the image is still listed, beside the document and where the
    // typed text ends
    let metadata = metadata.expect("metadata");
    assert_eq!(metadata["images"][0]["file"], "m-0.png");
    assert_eq!(metadata[METADATA_KEY][0]["name"], "facture.pdf");
    assert_eq!(metadata[METADATA_KEY][0]["parts"], 3);
    assert_eq!(metadata[TEXT_END_KEY], 17);
}

#[test]
fn test_the_body_limit_makes_room_for_every_document_beside_the_images() {
    // GIVEN the ceilings of the images and of the documents
    let images = crate::chat::images::max_message_body_bytes();

    // WHEN the body limit of a message is computed
    let limit = max_message_body_bytes();

    // THEN it holds the images and four documents of 20 MiB in base64
    assert!(limit >= images + MAX_DOCUMENTS_PER_MESSAGE * MAX_DOCUMENT_BYTES / 3 * 4);
}
