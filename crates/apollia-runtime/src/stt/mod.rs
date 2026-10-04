//! STT (Speech-to-Text) engine, Tokio actor for transcription orchestration.
//!
//! The [`SttEngine`] actor wraps a [`SttBackend`](apollia_stt::SttBackend)
//! behind a bounded `mpsc` channel. Transcription inference runs in
//! `spawn_blocking`; results are persisted via [`SttRepository`](apollia_stt::SttRepository)
//! and broadcast on the [`EventBus`](crate::eventbus::EventBus).

mod builder;
pub(crate) mod engine;
mod settings;

pub use builder::build_stt_engine;
pub use engine::{SttEngineError, SttEngineHandle, SttStatus, TranscriptSource};
pub use settings::{
    is_model_file, validate_language, UnsupportedLanguage, MODEL_EXTENSIONS, STT_LANGUAGES,
};
