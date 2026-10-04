use apollia_llm::{Capabilities, VisionEngine};
use base64::Engine as _;

use super::*;

fn attachment(mime: &str, bytes: &[u8]) -> ImageAttachment {
    ImageAttachment {
        mime_type: mime.to_owned(),
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
    }
}

#[test]
fn decoding_keeps_order_and_normalises_the_media_type() {
    // GIVEN a PNG and a JPEG sent under the alias `image/jpg`
    let sent = vec![
        attachment("image/png", &[1, 2, 3]),
        attachment("image/jpg", &[4, 5]),
    ];

    // WHEN they are decoded
    let images = decode_attachments(&sent).expect("valid images");

    // THEN both come back in order with their bytes and a canonical type
    assert_eq!(images.len(), 2);
    assert_eq!(images[0].mime(), "image/png");
    assert_eq!(images[0].bytes(), &[1, 2, 3]);
    assert_eq!(images[1].mime(), "image/jpeg");
}

#[test]
fn decoding_refuses_more_images_than_the_ceiling() {
    // GIVEN one image more than a message may carry
    let sent = vec![attachment("image/png", &[1]); MAX_IMAGES_PER_MESSAGE + 1];

    // WHEN they are decoded
    let result = decode_attachments(&sent);

    // THEN the message is refused
    assert!(matches!(result, Err(ChatError::ImageRejected(_))));
}

#[test]
fn decoding_refuses_an_image_above_the_size_ceiling() {
    // GIVEN an image one byte above the ceiling
    let sent = vec![attachment("image/png", &vec![0u8; MAX_IMAGE_BYTES + 1])];

    // WHEN it is decoded
    let result = decode_attachments(&sent);

    // THEN it is refused
    assert!(matches!(result, Err(ChatError::ImageRejected(_))));
}

#[test]
fn decoding_refuses_an_unknown_type_bad_base64_and_an_empty_image() {
    // GIVEN three faulty attachments
    let unknown = vec![attachment("application/pdf", &[1])];
    let garbled = vec![ImageAttachment {
        mime_type: "image/png".into(),
        data: "***".into(),
    }];
    let empty = vec![attachment("image/png", &[])];

    // WHEN each is decoded
    // THEN each is refused
    for sent in [unknown, garbled, empty] {
        assert!(matches!(
            decode_attachments(&sent),
            Err(ChatError::ImageRejected(_))
        ));
    }
}

#[test]
fn a_model_without_vision_or_unknown_capabilities_refuses_images() {
    // GIVEN one image, and a model that reads none or whose capabilities are
    // unknown (a remote server)
    let images = vec![MessageImage::new("image/png", vec![1u8])];
    let blind = Capabilities::default();

    // WHEN the runtime checks whether the model reads them
    let without_vision = check_model_reads(Some("text-only"), Some(&blind), &images);
    let unknown = check_model_reads(Some("remote"), None, &images);

    // THEN both are refused with the typed error naming the model
    assert!(matches!(
        without_vision,
        Err(ChatError::ImagesNotSupported { model: Some(ref m) }) if m == "text-only"
    ));
    assert!(matches!(unknown, Err(ChatError::ImagesNotSupported { .. })));
}

#[test]
fn a_model_with_vision_accepts_its_formats_and_refuses_the_others() {
    // GIVEN a GGUF model served by llama-server, which does not decode WebP
    let caps = Capabilities::default().with_vision(VisionEngine::LlamaServer);
    let png = vec![MessageImage::new("image/png", vec![1u8])];
    let webp = vec![MessageImage::new("image/webp", vec![1u8])];

    // WHEN the runtime checks each
    // THEN the PNG passes and the WebP is refused
    assert!(check_model_reads(Some("m"), Some(&caps), &png).is_ok());
    assert!(matches!(
        check_model_reads(Some("m"), Some(&caps), &webp),
        Err(ChatError::ImageRejected(_))
    ));
}

#[test]
fn stored_images_are_read_back_from_their_metadata() {
    // GIVEN two images written for a message
    let dir = tempfile::tempdir().expect("tempdir");
    let folder = session_images_dir(dir.path(), "session-1");
    let images = vec![
        MessageImage::new("image/png", vec![1u8, 2]),
        MessageImage::new("image/jpeg", vec![3u8]),
    ];
    let stored = store_images(&folder, "msg-1", &images).expect("written");

    // WHEN the metadata is written, parsed and the images loaded
    let metadata = images_metadata(&stored);
    let listed = stored_images(Some(&metadata));
    let loaded = load_images(&folder, &listed);

    // THEN the files exist and the same images come back, in order
    assert_eq!(listed, stored);
    assert!(listed.iter().all(|s| folder.join(&s.file).is_file()));
    assert_eq!(loaded, images);
}

#[test]
fn a_file_name_leaving_the_folder_is_never_read() {
    // GIVEN metadata naming a file outside the conversation folder
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("secret.png"), b"x").expect("write");
    let folder = dir.path().join("chat_images").join("s");
    let stored = vec![StoredImage {
        file: "../../secret.png".into(),
        mime_type: "image/png".into(),
        size: 1,
    }];

    // WHEN it is loaded
    let loaded = load_images(&folder, &stored);

    // THEN nothing is read
    assert!(loaded.is_empty());
}

#[test]
fn a_session_identifier_cannot_name_a_folder_outside_the_images() {
    // GIVEN a hostile session identifier
    // WHEN it is turned into a folder name
    let name = session_folder("../../etc");

    // THEN no separator nor parent step survives
    assert!(!name.contains('/'));
    assert!(!name.contains(".."));
}

#[test]
fn an_attachment_debug_never_prints_its_data() {
    // GIVEN an attachment whose data is a recognisable marker
    let sent = ImageAttachment {
        mime_type: "image/png".into(),
        data: "U0VDUkVU".into(),
    };

    // WHEN it is formatted with Debug
    let printed = format!("{sent:?}");

    // THEN the data does not appear
    assert!(!printed.contains("U0VDUkVU"));
}

#[test]
fn the_body_ceiling_holds_every_image_at_its_largest() {
    // GIVEN the ceilings on count and size
    // WHEN the body limit of the send route is derived
    let limit = max_message_body_bytes();

    // THEN it holds the images in base64 plus room for the text
    assert!(limit > MAX_IMAGES_PER_MESSAGE * MAX_IMAGE_BYTES * 4 / 3);
}
