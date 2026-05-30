//! STT (Speech-to-Text) engine, Tokio actor for transcription orchestration.
//!
//! The [`SttEngine`] actor wraps a [`SttBackend`](apollia_stt::SttBackend)
//! behind a bounded `mpsc` channel. Transcription inference runs in
//! `spawn_blocking`; results are persisted via [`SttRepository`](apollia_stt::SttRepository)
//! and broadcast on the [`EventBus`](crate::eventbus::EventBus).

pub(crate) mod engine;

pub use engine::{SttEngineError, SttEngineHandle, SttStatus, TranscriptSource};
