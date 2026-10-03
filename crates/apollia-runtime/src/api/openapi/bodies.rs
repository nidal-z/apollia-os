//! Request bodies no Rust type carries, so their schemas are written by hand.

use utoipa::openapi::schema::{KnownFormat, ObjectBuilder, Schema, SchemaFormat, Type};
use utoipa::openapi::RefOr;
use utoipa::ToSchema;

/// Request body of `POST /api/v1/stt/transcribe`: a multipart form.
///
/// The handler reads the parts off axum's `Multipart` stream, so no Rust type
/// carries the form and the schema is written by hand. Without one, the Python
/// generator skips the operation and the client offers no way to call it.
pub enum TranscribeForm {}

impl utoipa::PartialSchema for TranscribeForm {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .property(
                "audio",
                ObjectBuilder::new()
                    .schema_type(Type::String)
                    .format(Some(SchemaFormat::KnownFormat(KnownFormat::Binary)))
                    .description(Some("WAV audio file")),
            )
            .required("audio")
            .property(
                "language",
                ObjectBuilder::new()
                    .schema_type(Type::String)
                    .description(Some("Language hint, an ISO 639-1 code")),
            )
            .property(
                "source",
                ObjectBuilder::new()
                    .schema_type(Type::String)
                    .enum_values(Some(["api", "file", "hotkey"]))
                    .description(Some(
                        "What the audio is, the tag the history shows: api (default), file or hotkey",
                    )),
            )
            .into()
    }
}

impl ToSchema for TranscribeForm {}

/// Request body of `POST /webhooks/{id}`: the raw bytes the HMAC is computed on.
///
/// The handler takes `Bytes`, which carries no schema of its own.
pub enum RawWebhookBody {}

impl utoipa::PartialSchema for RawWebhookBody {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .format(Some(SchemaFormat::KnownFormat(KnownFormat::Binary)))
            .into()
    }
}

impl ToSchema for RawWebhookBody {}
