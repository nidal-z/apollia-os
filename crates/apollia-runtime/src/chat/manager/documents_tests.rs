//! Documents sent with a message: read on reception, carried by the message,
//! reachable by the file tools of their conversation, erased with it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use apollia_llm::types::{MessageContent, Role};
use base64::Engine as _;

use super::documents::with_session_documents;
use super::images_tests::{free_chat, spawn, wait_idle};
use crate::chat::documents::{
    decode_attachments, session_documents_dir, DocumentAttachment, MessageDocument,
    INLINE_MAX_CHARS, METADATA_KEY, TEXT_END_KEY,
};
use crate::chat::repository::ChatSessionRepository;
use crate::chat::types::ChatError;
use crate::test_support::poll_until;

fn reader_fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../apollia-sources/tests/fixtures")
        .join(relative)
}

fn document(name: &str, bytes: &[u8]) -> MessageDocument {
    decode_attachments(&[DocumentAttachment {
        name: name.to_owned(),
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
    }])
    .expect("decoded")
    .remove(0)
}

fn invoice() -> MessageDocument {
    let bytes = std::fs::read(reader_fixture("pdf/second-page-blank.pdf")).expect("fixture");
    document("facture.pdf", &bytes)
}

fn text_of(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(text) => text.clone(),
        other => panic!("expected a text message, got {other:?}"),
    }
}

#[tokio::test]
async fn test_a_pdf_sent_with_a_message_reaches_the_model_as_its_text() {
    // GIVEN a free chat
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, requests) = spawn(&dir);
    let id = free_chat(&handle).await;

    // WHEN a message is sent with a PDF
    let question = "What is the invoice number?";
    let message_id = handle
        .send_message_with_attachments(id.clone(), question.into(), Vec::new(), vec![invoice()])
        .await
        .expect("accepted");

    // THEN the model receives the question, then the name of the file, then
    // its text
    let reached = poll_until(Duration::from_secs(10), || {
        requests.lock().map(|r| !r.is_empty()).unwrap_or(false)
    })
    .await;
    assert!(reached, "the turn never reached the model");
    let first = requests.lock().expect("lock")[0].clone();
    let last = first.last().expect("a user message");
    assert_eq!(last.role, Role::User);
    let text = text_of(&last.content);
    let question_at = text.find(question).expect("the question");
    let name_at = text.find("facture.pdf").expect("the name");
    let body_at = text.find("Facture n° 2026-001").expect("the text");
    assert!(question_at < name_at && name_at < body_at, "{text}");

    // AND the stored message lists the document and where the typed text ends
    let repo = ChatSessionRepository::open(&dir.path().join("chat.db")).expect("open");
    let row = repo
        .get_messages(&id, None)
        .expect("messages")
        .into_iter()
        .find(|m| m.id == message_id)
        .expect("the user message");
    let metadata: serde_json::Value =
        serde_json::from_str(row.metadata.as_deref().expect("metadata")).expect("json");
    assert_eq!(metadata[METADATA_KEY][0]["name"], "facture.pdf");
    assert_eq!(metadata[METADATA_KEY][0]["inline"], true);
    assert_eq!(
        metadata[TEXT_END_KEY],
        question.encode_utf16().count() as u64
    );

    handle.shutdown().await;
}

#[tokio::test]
async fn test_a_long_document_is_referenced_and_its_folder_is_a_reading_root() {
    // GIVEN a free chat and a document of more than 100 000 characters
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, requests) = spawn(&dir);
    let id = free_chat(&handle).await;
    let long = "one line of the log\n".repeat(INLINE_MAX_CHARS / 10);

    // WHEN it is sent with a message
    handle
        .send_message_with_attachments(
            id.clone(),
            "Summarise this log".into(),
            Vec::new(),
            vec![document("journal.csv", long.as_bytes())],
        )
        .await
        .expect("accepted");

    // THEN the model receives the name and the path of the copy, not the text
    let reached = poll_until(Duration::from_secs(10), || {
        requests.lock().map(|r| !r.is_empty()).unwrap_or(false)
    })
    .await;
    assert!(reached, "the turn never reached the model");
    let first = requests.lock().expect("lock")[0].clone();
    let text = text_of(&first.last().expect("a user message").content);
    let folder = session_documents_dir(dir.path(), &id);
    assert!(text.contains("name=\"journal.csv\""), "{text}");
    assert!(
        text.contains(&format!("path=\"{}", folder.display())),
        "{text}"
    );
    assert!(text.len() < 2000, "the text of the document was sent");

    // AND the folder of the conversation is among what its file tools read,
    // after the granted folders, and never among what they write
    let granted = apollia_tools::FolderRoots {
        read: vec![PathBuf::from("/granted")],
        write: vec![PathBuf::from("/granted")],
    };
    let folders = with_session_documents(granted, Some(dir.path()), &id);
    assert_eq!(folders.read, [PathBuf::from("/granted"), folder]);
    assert_eq!(folders.write, [PathBuf::from("/granted")]);

    handle.shutdown().await;
}

#[tokio::test]
async fn test_a_conversation_without_documents_gains_no_reading_root() {
    // GIVEN a free chat that received no document, and another that did
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, _requests) = spawn(&dir);
    let with = free_chat(&handle).await;
    let without = free_chat(&handle).await;
    handle
        .send_message_with_attachments(with.clone(), "lis".into(), Vec::new(), vec![invoice()])
        .await
        .expect("accepted");

    // WHEN the reading roots of the other conversation are resolved
    let folders = with_session_documents(
        apollia_tools::FolderRoots {
            read: Vec::new(),
            write: Vec::new(),
        },
        Some(dir.path()),
        &without,
    );

    // THEN it reaches nothing: the documents belong to their conversation
    assert!(folders.read.is_empty());

    handle.shutdown().await;
}

#[tokio::test]
async fn test_a_refused_message_keeps_no_copy_of_its_documents() {
    // GIVEN a conversation still answering a first message
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, _requests) = spawn(&dir);
    let id = free_chat(&handle).await;
    let locked = std::fs::read(reader_fixture("pdf/user-password.pdf")).expect("fixture");

    // WHEN a message is sent with a document protected by a password, and
    // another to a conversation that does not exist
    let unreadable = handle
        .send_message_with_attachments(
            id.clone(),
            "lis".into(),
            Vec::new(),
            vec![document("secret.pdf", &locked)],
        )
        .await;
    let unknown = handle
        .send_message_with_attachments("absent".into(), "lis".into(), Vec::new(), vec![invoice()])
        .await;

    // THEN both are refused with their cause, and no copy is kept
    assert!(matches!(unreadable, Err(ChatError::DocumentRejected(_))));
    assert!(matches!(unknown, Err(ChatError::SessionNotFound(_))));
    let folder = session_documents_dir(dir.path(), &id);
    let kept = std::fs::read_dir(&folder).map(Iterator::count).unwrap_or(0);
    assert_eq!(kept, 0);
    assert!(!session_documents_dir(dir.path(), "absent").exists());

    handle.shutdown().await;
}

#[tokio::test]
async fn test_deleting_a_conversation_deletes_its_documents() {
    // GIVEN a conversation holding a message with a document
    let dir = tempfile::tempdir().expect("tempdir");
    let (handle, _requests) = spawn(&dir);
    let id = free_chat(&handle).await;
    handle
        .send_message_with_attachments(id.clone(), "garde".into(), Vec::new(), vec![invoice()])
        .await
        .expect("accepted");
    assert!(wait_idle(&handle, &id).await, "the turn never ended");
    let folder = session_documents_dir(dir.path(), &id);
    assert!(folder.is_dir());

    // WHEN the conversation is deleted
    handle.delete_session(id.clone()).await.expect("deleted");

    // THEN its documents folder is gone
    assert!(!folder.exists());

    handle.shutdown().await;
}
