//! SttEngine — Tokio actor orchestrating Speech-to-Text transcriptions.
//!
//! The actor processes [`SttCommand`] messages received via a bounded `mpsc`
//! channel. Callers interact exclusively through [`SttEngineHandle`], which is
//! `Clone + Send + Sync`.
//!
//! Transcription runs in `spawn_blocking` because [`SttBackend::transcribe`]
//! is synchronous (CPU-bound inference). Results are persisted in
//! [`SttRepository`] fire-and-forget and broadcast via the [`EventBus`].

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tracing::{info, warn};

use apollia_core::{EventBusSender, RuntimeEvent, SttConfigRow};
use apollia_stt::{SttBackend, SttError, SttRepository, TranscriptResult};

/// Bounded channel capacity for the actor mailbox.
const CHANNEL_CAPACITY: usize = 32;

// ── Public types ────────────────────────────────────────────────────

/// Source of a transcription request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TranscriptSource {
    /// Triggered by the global hotkey.
    Hotkey,
    /// Transcription of a file at the given path.
    File(String),
    /// Submitted via the REST API.
    Api,
}

impl TranscriptSource {
    /// Returns the string tag persisted in the repository (`"hotkey"`, `"file"`, `"api"`).
    fn as_str(&self) -> &str {
        match self {
            Self::Hotkey => "hotkey",
            Self::File(_) => "file",
            Self::Api => "api",
        }
    }
}

/// Runtime status of the STT engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SttStatus {
    /// Whether STT is enabled in configuration.
    pub enabled: bool,
    /// Whether the model is loaded and ready for inference.
    pub model_loaded: bool,
    /// Filesystem path of the loaded model.
    pub model_path: String,
    /// Short model name (derived from filename without extension).
    pub model_name: String,
    /// Name of the active backend (e.g. `"whisper-cpp"`).
    pub backend_name: String,
    /// `true` when compiled with Apple Metal GPU acceleration.
    pub metal_enabled: bool,
    /// `true` when compiled with NVIDIA CUDA GPU acceleration.
    pub cuda_enabled: bool,
}

/// Errors specific to the [`SttEngineHandle`] lifecycle and communication.
#[derive(Debug, thiserror::Error)]
pub enum SttEngineError {
    /// The underlying STT backend returned an error.
    #[error("STT backend error: {0}")]
    Backend(#[from] SttError),

    /// The actor channel is closed — the engine has been shut down.
    #[error("STT engine channel closed")]
    ChannelClosed,
}

// ── Internal command enum ───────────────────────────────────────────

/// Command sent to the SttEngine actor via its handle.
enum SttCommand {
    /// Request a transcription from an audio buffer.
    Transcribe {
        audio: Vec<f32>,
        sample_rate: u32,
        source: TranscriptSource,
        reply: oneshot::Sender<Result<TranscriptResult, SttError>>,
    },
    /// Query the current engine status.
    GetStatus { reply: oneshot::Sender<SttStatus> },
    /// Request a graceful shutdown.
    Shutdown,
}

// ── Public handle ───────────────────────────────────────────────────

/// Clonable, `Send + Sync` handle to the [`SttEngine`] actor.
///
/// All public methods send a command over the bounded channel and await
/// the response via a `oneshot` reply channel.
#[derive(Clone)]
pub struct SttEngineHandle {
    tx: mpsc::Sender<SttCommand>,
}

impl SttEngineHandle {
    /// Spawn the SttEngine actor and return its handle.
    ///
    /// Emits [`RuntimeEvent::SttModelLoaded`] on the EventBus after the actor
    /// is spawned. The caller is responsible for loading the backend and opening
    /// the repository before calling this function.
    pub fn start(
        backend: Box<dyn SttBackend>,
        repository: SttRepository,
        config: SttConfigRow,
        event_bus: EventBusSender,
    ) -> Self {
        let model_name = std::path::Path::new(&config.model_path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let backend_name = backend.name().to_owned();
        let model_path_display = config.model_path.clone();

        let (tx, rx) = mpsc::channel(CHANNEL_CAPACITY);

        let engine = SttEngine {
            backend: Arc::from(backend),
            repository: std::sync::Mutex::new(repository),
            event_bus: event_bus.clone(),
            config,
            model_name: model_name.clone(),
        };
        tokio::spawn(engine.run(rx));

        let _ = event_bus.send(RuntimeEvent::SttModelLoaded {
            backend: backend_name,
            model_path: model_path_display,
            model_name,
        });

        Self { tx }
    }

    /// Request a transcription of the given audio buffer.
    ///
    /// The actual inference runs in `spawn_blocking`; the result is persisted
    /// and broadcast before the reply is sent.
    pub async fn transcribe(
        &self,
        audio: Vec<f32>,
        sample_rate: u32,
        source: TranscriptSource,
    ) -> Result<TranscriptResult, SttEngineError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SttCommand::Transcribe {
                audio,
                sample_rate,
                source,
                reply: reply_tx,
            })
            .await
            .map_err(|_| SttEngineError::ChannelClosed)?;
        reply_rx
            .await
            .map_err(|_| SttEngineError::ChannelClosed)?
            .map_err(SttEngineError::Backend)
    }

    /// Query the current status of the engine.
    ///
    /// Returns `None` if the actor has shut down.
    pub async fn status(&self) -> Option<SttStatus> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SttCommand::GetStatus { reply: reply_tx })
            .await
            .ok()?;
        reply_rx.await.ok()
    }

    /// Request a graceful shutdown of the actor.
    ///
    /// The backend's [`SttBackend::unload`] is called before the actor exits.
    pub async fn shutdown(&self) {
        let _ = self.tx.send(SttCommand::Shutdown).await;
    }
}

// ── Private actor ───────────────────────────────────────────────────

/// Private actor struct — owns the mutable state.
///
/// Spawned as a Tokio task by [`SttEngineHandle::start`]. Processes one
/// command at a time from the bounded `mpsc` receiver.
///
/// `SttRepository` is wrapped in `std::sync::Mutex` because `rusqlite::Connection`
/// is `Send` but not `Sync`; the Mutex makes `&SttEngine` `Send` across awaits.
/// Since the actor is single-threaded, the lock is never contended.
struct SttEngine {
    backend: Arc<dyn SttBackend>,
    repository: std::sync::Mutex<SttRepository>,
    event_bus: EventBusSender,
    config: SttConfigRow,
    model_name: String,
}

impl SttEngine {
    /// Main actor loop — processes commands until `Shutdown` or channel close.
    async fn run(self, mut rx: mpsc::Receiver<SttCommand>) {
        info!("SttEngine started");
        while let Some(cmd) = rx.recv().await {
            match cmd {
                SttCommand::Transcribe {
                    audio,
                    sample_rate,
                    source,
                    reply,
                } => {
                    let result = self.handle_transcribe(audio, sample_rate, &source).await;
                    let _ = reply.send(result);
                }
                SttCommand::GetStatus { reply } => {
                    let _ = reply.send(SttStatus {
                        enabled: self.config.enabled,
                        model_loaded: true,
                        model_path: self.config.model_path.clone(),
                        model_name: self.model_name.clone(),
                        backend_name: self.backend.name().to_owned(),
                        metal_enabled: cfg!(feature = "stt-metal"),
                        cuda_enabled: cfg!(feature = "stt-cuda"),
                    });
                }
                SttCommand::Shutdown => {
                    self.backend.unload();
                    break;
                }
            }
        }
        info!("SttEngine stopped");
    }

    /// Runs transcription in `spawn_blocking`, persists result, and emits event.
    async fn handle_transcribe(
        &self,
        audio: Vec<f32>,
        sample_rate: u32,
        source: &TranscriptSource,
    ) -> Result<TranscriptResult, SttError> {
        let backend = Arc::clone(&self.backend);
        let language_hint = self.config.language.clone();

        let result = tokio::task::spawn_blocking(move || {
            backend.transcribe(&audio, sample_rate, language_hint.as_deref())
        })
        .await
        .map_err(|e| SttError::Internal(format!("spawn_blocking join failed: {e}")))?;

        match &result {
            Ok(transcript) => {
                let source_str = source.as_str();
                match self.repository.lock() {
                    Ok(repo) => {
                        if let Err(e) = repo.insert(source_str, transcript, Some(&self.model_name))
                        {
                            warn!(error = %e, "failed to persist STT transcription");
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "SttRepository lock poisoned — skipping persistence");
                    }
                }
                let _ = self.event_bus.send(RuntimeEvent::SttTranscribed {
                    text: transcript.full_text.clone(),
                    language: transcript.language.clone(),
                    source: source_str.to_owned(),
                    duration_ms: transcript.audio_duration_ms,
                    processing_time_ms: transcript.processing_time_ms,
                });
            }
            Err(e) => {
                let _ = self.event_bus.send(RuntimeEvent::SttTranscriptionFailed {
                    reason: e.to_string(),
                });
            }
        }

        result
    }
}

/// Attempts to load an STT backend from the given model path.
///
/// When compiled with `stt-whisper-cpp`, loads the model via
/// [`WhisperCppBackend`](apollia_stt::WhisperCppBackend). Otherwise returns
/// [`SttError::BackendUnavailable`].
pub(crate) fn try_load_backend(model_path: &str) -> Result<Box<dyn SttBackend>, SttError> {
    #[cfg(feature = "stt-whisper-cpp")]
    {
        let backend = apollia_stt::WhisperCppBackend::load(model_path)?;
        Ok(Box::new(backend))
    }
    #[cfg(not(feature = "stt-whisper-cpp"))]
    {
        let _ = model_path;
        Err(SttError::BackendUnavailable {
            backend: "no STT backend compiled (enable stt-whisper-cpp feature)".to_owned(),
        })
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use apollia_stt::TranscriptSegment;
    use tokio::sync::broadcast;

    /// Fake backend for unit tests — returns a fixed transcript.
    struct FakeBackend {
        should_fail: bool,
    }

    impl SttBackend for FakeBackend {
        fn name(&self) -> &str {
            "fake"
        }

        fn transcribe(
            &self,
            _audio: &[f32],
            _sample_rate: u32,
            _language_hint: Option<&str>,
        ) -> Result<TranscriptResult, SttError> {
            if self.should_fail {
                return Err(SttError::TranscriptionFailed {
                    reason: "forced failure".to_owned(),
                });
            }
            Ok(TranscriptResult {
                full_text: "Bonjour le monde".to_owned(),
                segments: vec![TranscriptSegment {
                    text: "Bonjour le monde".to_owned(),
                    start_ms: 0,
                    end_ms: 1500,
                    confidence: 0.95,
                }],
                language: Some("fr".to_owned()),
                audio_duration_ms: 1500,
                processing_time_ms: 200,
            })
        }
    }

    fn test_config() -> SttConfigRow {
        SttConfigRow {
            enabled: true,
            model_path: "/tmp/test-model.bin".to_owned(),
            language: Some("fr".to_owned()),
            ..SttConfigRow::default()
        }
    }

    fn start_test_engine(
        should_fail: bool,
    ) -> (SttEngineHandle, broadcast::Receiver<RuntimeEvent>) {
        let (event_tx, event_rx) = broadcast::channel(64);
        let repo_dir = tempfile::tempdir().expect("tempdir");
        let repo = SttRepository::open(&repo_dir.path().join("stt.db")).expect("open repo");
        let backend = Box::new(FakeBackend { should_fail });
        let handle = SttEngineHandle::start(backend, repo, test_config(), event_tx);
        // Leak tempdir to keep it alive (tests are short-lived)
        std::mem::forget(repo_dir);
        (handle, event_rx)
    }

    #[tokio::test]
    async fn transcribe_success_returns_transcript_and_emits_event() {
        // GIVEN
        let (handle, mut event_rx) = start_test_engine(false);

        // Drain the SttModelLoaded event
        let model_event = event_rx.recv().await.expect("model event");
        assert!(matches!(model_event, RuntimeEvent::SttModelLoaded { .. }));

        // WHEN
        let result = handle
            .transcribe(vec![0.0; 16000], 16000, TranscriptSource::Hotkey)
            .await;

        // THEN
        let transcript = result.expect("transcribe should succeed");
        assert_eq!(transcript.full_text, "Bonjour le monde");
        assert_eq!(transcript.language.as_deref(), Some("fr"));

        let event = event_rx.recv().await.expect("transcribed event");
        match event {
            RuntimeEvent::SttTranscribed { text, source, .. } => {
                assert_eq!(text, "Bonjour le monde");
                assert_eq!(source, "hotkey");
            }
            other => unreachable!("unexpected event in test mock: {other:?}"),
        }
    }

    #[tokio::test]
    async fn transcribe_error_emits_failure_event() {
        // GIVEN
        let (handle, mut event_rx) = start_test_engine(true);
        let _ = event_rx.recv().await; // drain SttModelLoaded

        // WHEN
        let result = handle
            .transcribe(vec![0.0; 16000], 16000, TranscriptSource::Api)
            .await;

        // THEN
        assert!(result.is_err());
        let event = event_rx.recv().await.expect("error event");
        assert!(matches!(event, RuntimeEvent::SttTranscriptionFailed { .. }));
    }

    #[tokio::test]
    async fn status_returns_engine_info() {
        // GIVEN
        let (handle, _rx) = start_test_engine(false);

        // WHEN
        let status = handle.status().await.expect("status should succeed");

        // THEN
        assert!(status.enabled);
        assert!(status.model_loaded);
        assert_eq!(status.backend_name, "fake");
        assert!(status.model_path.contains("test-model"));
        assert_eq!(status.model_name, "test-model");
    }

    #[tokio::test]
    async fn shutdown_stops_actor_gracefully() {
        // GIVEN
        let (handle, _rx) = start_test_engine(false);

        // WHEN
        handle.shutdown().await;

        // THEN — subsequent calls fail with ChannelClosed
        let result = handle.status().await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn transcribe_after_shutdown_returns_channel_closed() {
        // GIVEN
        let (handle, _rx) = start_test_engine(false);
        handle.shutdown().await;
        // Allow actor task to exit
        tokio::task::yield_now().await;

        // WHEN
        let result = handle
            .transcribe(vec![0.0; 100], 16000, TranscriptSource::Api)
            .await;

        // THEN
        assert!(matches!(result, Err(SttEngineError::ChannelClosed)));
    }

    #[tokio::test]
    async fn model_loaded_event_emitted_on_start() {
        // GIVEN / WHEN
        let (_handle, mut event_rx) = start_test_engine(false);

        // THEN
        let event = event_rx.recv().await.expect("model loaded event");
        match event {
            RuntimeEvent::SttModelLoaded {
                backend,
                model_name,
                ..
            } => {
                assert_eq!(backend, "fake");
                assert_eq!(model_name, "test-model");
            }
            other => unreachable!("unexpected event in test mock: {other:?}"),
        }
    }

    #[tokio::test]
    async fn transcript_source_serializes_correctly() {
        // GIVEN
        let sources = vec![
            (TranscriptSource::Hotkey, "hotkey"),
            (TranscriptSource::File("/tmp/audio.wav".into()), "file"),
            (TranscriptSource::Api, "api"),
        ];

        // THEN
        for (source, expected) in sources {
            assert_eq!(source.as_str(), expected);
        }
    }
}
