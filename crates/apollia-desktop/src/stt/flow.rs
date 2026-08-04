//! End-to-end orchestration of the desktop STT pipeline.
//!
//! Coordinates hotkey press → audio capture → resample → silence trim →
//! transcription → clipboard injection / desktop notification for seamless
//! dictation from any application.
//!
//! The audio stream lives on a dedicated thread because `cpal::Stream` is
//! not `Send` on macOS (Core Audio thread affinity). The shared
//! [`CaptureBuffer`] is `Send + Sync` and drained from the async context.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use apollia_core::{EventBusSender, RuntimeEvent, SttConfigRow};
use apollia_stt::{
    peak_amplitude, to_whisper_format, trim_silence, AudioCapture, CaptureBuffer, SttError,
};
use serde::Serialize;
use tauri::Emitter;
use tauri_plugin_notification::NotificationExt;

use apollia_runtime::api::server::SharedSttEngine;
use apollia_runtime::stt::TranscriptSource;

use crate::stt::clipboard;

/// Whisper model sample rate in Hz.
const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// Minimum audio duration in samples before calling the STT engine.
///
/// 100 ms at 16 kHz = 1 600 samples. Shorter recordings are discarded
/// to avoid wasting inference on noise or accidental hotkey presses.
const MIN_SAMPLES: usize = 1_600;

/// Tauri event carrying the terminal outcome of a dictation that produced no text.
///
/// The webview flips its microphone button to "recording" the moment
/// `start_tour_recording` returns and clears it when a transcription arrives.
/// Every path that ends a dictation without a transcription therefore has to say
/// so, or the button stays lit forever and the user has no way back. Capture
/// runs on a detached thread, so a failure to open the device has no return
/// value to travel back through either.
const DICTATION_FAILED_EVENT: &str = "stt-dictation-failed";

/// Machine-readable reason a dictation ended without producing text.
///
/// The webview maps each variant onto a localised sentence; the payload carries
/// no prose so the two locales stay in the frontend catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DictationFailure {
    /// No input device exists on this host.
    NoMicrophone,
    /// The configured device could not be opened or the stream could not start.
    CaptureFailed,
    /// Capture never published its buffer, so there is nothing to transcribe.
    NoAudioCaptured,
    /// The captured audio could not be converted to the Whisper format.
    AudioUnusable,
    /// Every 10 ms window sat below the silence threshold.
    NothingAudible,
    /// The audible span was shorter than [`MIN_SAMPLES`].
    TooShort,
    /// STT is enabled but no model is loaded.
    NoModel,
    /// The engine returned an error.
    TranscriptionFailed,
    /// The engine returned an empty transcript.
    EmptyTranscript,
    /// The user aborted the recording (Escape on the overlay).
    Cancelled,
}

/// Payload of [`DICTATION_FAILED_EVENT`].
#[derive(Debug, Clone, Serialize)]
struct DictationFailedPayload {
    reason: DictationFailure,
}

/// Which surface started the recording currently in progress.
///
/// The delivery of the text depends on how the dictation *started*, never on
/// which control stopped it. Starting from the in-app microphone and stopping
/// with the global hotkey used to reach the clipboard-paste branch while the
/// webview was still waiting on `stt-transcribed`, so the text landed in the
/// composer twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingOrigin {
    /// Global hotkey: the text is pasted into whatever application is focused.
    Hotkey,
    /// In-app microphone button or onboarding test: the webview consumes the
    /// broadcast `stt-transcribed` event, so no OS paste must happen.
    InApp,
}

/// Emits the terminal dictation-failure event to every webview.
fn emit_dictation_failed(app: &tauri::AppHandle, reason: DictationFailure) {
    if let Err(e) = app.emit(DICTATION_FAILED_EVENT, DictationFailedPayload { reason }) {
        tracing::warn!(error = %e, "failed to emit stt-dictation-failed");
    }
}

/// Orchestrates the full STT flow: recording → processing → output.
///
/// Created once during Tauri setup when STT is enabled. Shared between
/// the hotkey start/stop callbacks via [`Arc`].
///
/// The audio stream is owned by a dedicated OS thread (Core Audio thread
/// affinity on macOS prevents `cpal::Stream` from being `Send`). The
/// capture buffer is shared via [`Arc<Mutex<_>>`] and drained from the
/// async runtime side.
pub struct SttFlow {
    /// STT configuration (thresholds, clipboard mode, etc.).
    config: SttConfigRow,
    /// Shared, swappable handle to the SttEngine actor. Read on each trigger so
    /// a model brought online mid-session (via `reload_stt`) is picked up
    /// without rebuilding the flow or re-registering the hotkey.
    stt_engine: SharedSttEngine,
    /// EventBus sender for lifecycle event broadcasting.
    event_bus: EventBusSender,
    /// Tauri application handle for desktop notifications.
    app: tauri::AppHandle,
    /// Shared flag tracking whether recording is currently active.
    recording: Arc<AtomicBool>,
    /// Surface that started the recording in progress, set by
    /// [`SttFlow::start_recording`] and read when the result is dispatched.
    origin: Arc<Mutex<RecordingOrigin>>,
    /// Active capture buffer accumulating microphone samples.
    active_buffer: Arc<Mutex<Option<CaptureBuffer>>>,
    /// Sender to signal the audio thread to stop. Dropping the sender
    /// unblocks the `recv()` on the audio thread, releasing the stream.
    stop_tx: Arc<Mutex<Option<std::sync::mpsc::Sender<()>>>>,
}

impl SttFlow {
    /// Configuration snapshot this flow was armed with.
    ///
    /// The reload path compares it against the persisted row to decide whether
    /// the flow has to be rebuilt. Every field the flow reads at trigger time
    /// comes from this snapshot, so the comparison has to cover the whole row
    /// rather than the hotkey alone.
    pub fn config(&self) -> &SttConfigRow {
        &self.config
    }

    /// Creates a new orchestrator from the STT configuration and runtime handles.
    pub fn new(
        config: SttConfigRow,
        stt_engine: SharedSttEngine,
        event_bus: EventBusSender,
        app: tauri::AppHandle,
    ) -> Self {
        Self {
            config,
            stt_engine,
            event_bus,
            app,
            recording: Arc::new(AtomicBool::new(false)),
            origin: Arc::new(Mutex::new(RecordingOrigin::Hotkey)),
            active_buffer: Arc::new(Mutex::new(None)),
            stop_tx: Arc::new(Mutex::new(None)),
        }
    }

    /// Returns the shared recording flag for external state queries.
    pub fn recording_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.recording)
    }

    /// Starts audio capture from the default microphone.
    ///
    /// Spawns a dedicated OS thread that owns the `cpal::Stream` (required
    /// because Core Audio has thread affinity on macOS). The shared
    /// [`CaptureBuffer`] is accessible from the async runtime for draining.
    ///
    /// Emits [`RuntimeEvent::SttRecordingStarted`] once the capture begins.
    /// If already recording or the capture fails, logs a warning and returns.
    ///
    /// `origin` records which surface asked for the recording; it decides how
    /// the transcription is delivered once the dictation ends.
    pub fn start_recording(&self, origin: RecordingOrigin) {
        if self.recording.swap(true, Ordering::SeqCst) {
            tracing::warn!("start_recording called while already recording");
            return;
        }

        if let Ok(mut guard) = self.origin.lock() {
            *guard = origin;
        }

        // Prepare the stop channel before spawning the thread so that
        // stop_and_transcribe can always find it.
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        if let Ok(mut guard) = self.stop_tx.lock() {
            *guard = Some(stop_tx);
        }

        let active_buffer = Arc::clone(&self.active_buffer);
        let recording = Arc::clone(&self.recording);
        let event_bus = self.event_bus.clone();
        let app = self.app.clone();
        let input_device = self.config.input_device.clone();

        std::thread::spawn(move || {
            let capture = match AudioCapture::open(input_device.as_deref()) {
                Ok(c) => c,
                Err(SttError::NoInputDevice) => {
                    recording.store(false, Ordering::SeqCst);
                    tracing::warn!("STT recording requested but no microphone is available");
                    notify_no_microphone(&app);
                    emit_dictation_failed(&app, DictationFailure::NoMicrophone);
                    return;
                }
                Err(e) => {
                    recording.store(false, Ordering::SeqCst);
                    tracing::warn!(error = %e, "failed to open audio input device");
                    emit_dictation_failed(&app, DictationFailure::CaptureFailed);
                    return;
                }
            };

            // Name the device that was actually opened, with the format it
            // imposed. Without this line a capture that yields silence is
            // indistinguishable from a capture that never happened, and the
            // configured device name cannot be checked against what the host
            // really handed over.
            tracing::info!(
                device = %capture.device_name(),
                sample_rate = capture.sample_rate(),
                channels = capture.channels(),
                sample_format = %capture.sample_format(),
                requested = input_device.as_deref().unwrap_or("<system default>"),
                "stt.capture.device_opened"
            );

            let (stream, buffer) = match capture.start() {
                Ok(pair) => pair,
                Err(e) => {
                    recording.store(false, Ordering::SeqCst);
                    tracing::warn!(error = %e, "failed to start audio capture");
                    emit_dictation_failed(&app, DictationFailure::CaptureFailed);
                    return;
                }
            };

            // Clone the lock-free level handle before the buffer is handed to
            // the drain side, so the metering loop can poll input amplitude.
            let level = buffer.level_handle();

            // Publish the buffer so the async side can drain it later.
            if let Ok(mut guard) = active_buffer.lock() {
                *guard = Some(buffer);
            }

            let _ = event_bus.send(RuntimeEvent::SttRecordingStarted);
            tracing::info!("STT recording started");

            // Meter the real captured level at ~30 Hz and push it to the overlay
            // so the waveform reflects the same audio the STT engine records.
            // recv_timeout returns Disconnected when signal_stop drops the
            // sender, which ends both the loop and the stream.
            loop {
                match stop_rx.recv_timeout(std::time::Duration::from_millis(33)) {
                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if let Err(e) = app.emit("stt-audio-level", level.get()) {
                            tracing::trace!(error = %e, "failed to emit stt-audio-level");
                        }
                    }
                }
            }

            // Flatten the meter once capture ends.
            let _ = app.emit("stt-audio-level", 0.0_f32);

            // stream is dropped here, releasing the audio device.
            drop(stream);
        });
    }

    /// Cancels recording and discards all captured audio without transcribing.
    ///
    /// Emits [`RuntimeEvent::SttRecordingStopped`] so the overlay closes, but
    /// skips the STT engine entirely. Use this for user-initiated abort (Escape).
    pub fn cancel_recording(&self) {
        if !self.recording.swap(false, Ordering::SeqCst) {
            tracing::debug!("cancel_recording called while not recording");
            return;
        }
        // Discard buffered samples without draining for transcription.
        if let Ok(mut guard) = self.active_buffer.lock() {
            let _ = guard.take();
        }
        self.signal_stop();
        let _ = self.event_bus.send(RuntimeEvent::SttRecordingStopped {
            audio_duration_ms: 0,
        });
        // An abort is still an end of course for the webview: without it the
        // microphone button stays lit after Escape.
        self.fail(DictationFailure::Cancelled);
        tracing::info!("STT recording cancelled (audio discarded)");
    }

    /// Stops recording, processes the audio, and dispatches the result.
    ///
    /// Pipeline: drain buffer → signal audio thread to stop → resample to
    /// 16 kHz mono → trim silence → transcribe via SttEngine → inject into
    /// clipboard and/or send notification depending on `clipboard_mode`.
    ///
    /// Delivery follows the [`RecordingOrigin`] recorded at start, not the
    /// control that stopped the dictation. A hotkey recording is pasted into
    /// whatever application is focused; an in-app recording is delivered only
    /// through the broadcast `stt-transcribed` event, because an additional OS
    /// paste would double-insert the text when the Apollia window is focused.
    ///
    /// Emits [`RuntimeEvent::SttRecordingStopped`] with the raw audio duration.
    /// Recordings shorter than 100 ms, and recordings whose every window sits
    /// below the silence threshold, are discarded without calling the STT
    /// engine. Every such path emits [`DICTATION_FAILED_EVENT`] so the webview
    /// clears its recording state instead of waiting forever for a
    /// transcription that will not come.
    pub async fn stop_and_transcribe(&self) {
        if !self.recording.swap(false, Ordering::SeqCst) {
            // The capture thread already cleared the flag after failing to open
            // the device, and emitted its own failure. Nothing left to report.
            tracing::debug!("stop_and_transcribe called while not recording");
            return;
        }

        // Drain the buffer BEFORE stopping the audio thread. The cpal callback
        // may still push a few samples between drain and stop; that is fine.
        let (raw_samples, sample_rate, channels) = match self.drain_buffer() {
            Some(data) => data,
            None => {
                self.signal_stop();
                // The overlay hides on SttRecordingStopped; skipping it here
                // left the indicator on screen with Escape still captured.
                let _ = self.event_bus.send(RuntimeEvent::SttRecordingStopped {
                    audio_duration_ms: 0,
                });
                tracing::warn!("no active buffer - recording may have failed to start");
                self.fail(DictationFailure::NoAudioCaptured);
                return;
            }
        };

        // Signal the audio thread to drop the stream and exit.
        self.signal_stop();

        let audio_duration_ms = compute_duration_ms(raw_samples.len(), sample_rate, channels);

        let _ = self
            .event_bus
            .send(RuntimeEvent::SttRecordingStopped { audio_duration_ms });
        tracing::info!(duration_ms = audio_duration_ms, "STT recording stopped");

        let final_audio = match prepare_audio(
            &raw_samples,
            sample_rate,
            channels,
            self.config.silence_threshold_db,
            self.config.max_recording_sec,
        ) {
            Ok(audio) => audio,
            Err(reason) => {
                self.fail(reason);
                return;
            }
        };

        // Read the current engine from the shared cell. `None` means STT was
        // enabled in config but no model is loaded yet (download pending or a
        // reload that resolved to disabled). Notify and bail rather than fail.
        let Some(engine) = self.stt_engine.read().await.clone() else {
            tracing::warn!("STT hotkey pressed but no engine is loaded - skipping transcription");
            let result = self
                .app
                .notification()
                .builder()
                .title("Aucun mod\u{00e8}le STT charg\u{00e9}")
                .body("Activez la dict\u{00e9}e et chargez un mod\u{00e8}le dans les R\u{00e9}glages.")
                .show();
            if let Err(e) = result {
                tracing::warn!(error = %e, "failed to send STT unavailable notification");
            }
            self.fail(DictationFailure::NoModel);
            return;
        };

        // Transcribe via the SttEngine actor.
        let transcript = match engine
            .transcribe(final_audio, WHISPER_SAMPLE_RATE, TranscriptSource::Hotkey)
            .await
        {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "STT transcription failed");
                self.fail(DictationFailure::TranscriptionFailed);
                return;
            }
        };

        if transcript.full_text.trim().is_empty() {
            tracing::info!("transcription result is empty - no output");
            self.fail(DictationFailure::EmptyTranscript);
            return;
        }

        // In-app recordings rely on the broadcast `stt-transcribed` event to
        // fill the composer, so the OS paste is skipped for them.
        if self.origin() == RecordingOrigin::Hotkey {
            self.dispatch_result(&transcript.full_text).await;
        }
    }

    /// Surface that started the recording being dispatched.
    ///
    /// Falls back to [`RecordingOrigin::Hotkey`] on a poisoned lock, which
    /// matches the behaviour before the origin was tracked.
    fn origin(&self) -> RecordingOrigin {
        self.origin
            .lock()
            .map(|guard| *guard)
            .unwrap_or(RecordingOrigin::Hotkey)
    }

    /// Broadcasts the terminal failure of a dictation to the webviews.
    ///
    /// Every `stop_and_transcribe` path that returns without a transcription
    /// goes through here, so the microphone button always has an end of course.
    fn fail(&self, reason: DictationFailure) {
        emit_dictation_failed(&self.app, reason);
    }

    /// Takes and drains the active capture buffer.
    fn drain_buffer(&self) -> Option<(Vec<f32>, u32, u16)> {
        let buffer = self
            .active_buffer
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())?;
        let samples = buffer.drain();
        let sr = buffer.sample_rate();
        let ch = buffer.channels();
        Some((samples, sr, ch))
    }

    /// Signals the audio thread to stop by dropping the stop sender.
    fn signal_stop(&self) {
        if let Ok(mut guard) = self.stop_tx.lock() {
            let _ = guard.take();
        }
    }

    /// Dispatches the transcription result based on `clipboard_mode`.
    ///
    /// | mode          | action                                                          |
    /// |---------------|-----------------------------------------------------------------|
    /// | `"paste"`     | clipboard → osascript Cmd+V (macOS) / enigo Ctrl+V (Linux)     |
    /// | `"clipboard"` | clipboard only, user pastes manually                            |
    /// | `"memo"`      | desktop notification only                                       |
    /// | `"both"`      | clipboard → paste + notification                                |
    async fn dispatch_result(&self, text: &str) {
        let mode = self.config.clipboard_mode.as_str();
        tracing::debug!(mode, len = text.len(), "dispatching transcription result");

        match mode {
            "paste" | "both" => self.inject_clipboard(text).await,
            "clipboard" => self.write_clipboard(text),
            _ => {}
        }

        if mode == "memo" || mode == "both" {
            self.send_notification(text);
        }
    }

    /// Writes text to the clipboard without simulating a paste keystroke.
    fn write_clipboard(&self, text: &str) {
        if let Err(e) = clipboard::write_only(text) {
            tracing::warn!(error = %e, "clipboard write failed");
        }
    }

    /// Injects transcribed text via clipboard + paste from a subprocess.
    ///
    /// Step 1: write text to clipboard (arboard, fast).
    /// Step 2: async sleep, lets the WindowServer propagate the clipboard.
    /// Step 3: osascript subprocess sends Cmd+V (macOS) / enigo Ctrl+V (Linux).
    ///
    /// Using a subprocess for the paste keystroke avoids two bugs specific to
    /// macOS + Tauri:
    ///  - `enigo` Meta+V CGEvent crashes when WebKit processes it on the main
    ///    thread while the Tokio runtime is live.
    ///  - `enigo::text()` HID events loop through Tauri's global-shortcut tap,
    ///    causing the transcription to be retyped as progressively shorter
    ///    suffixes each time a matching key is encountered in the text.
    async fn inject_clipboard(&self, text: &str) {
        let restore = self.config.clipboard_restore;

        let previous = match clipboard::prepare_paste(text, restore) {
            Ok(prev) => prev,
            Err(e) => {
                tracing::warn!(error = %e, "clipboard write failed");
                return;
            }
        };

        tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;

        if let Err(e) = clipboard::paste_via_subprocess() {
            tracing::warn!(error = %e, "paste failed");
            return;
        }

        if let Some(prev) = previous {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            if let Err(e) = clipboard::restore_clipboard(&prev) {
                tracing::warn!(error = %e, "clipboard restore failed");
            }
        }
    }

    /// Sends a desktop notification with the transcription preview (80 chars max).
    fn send_notification(&self, text: &str) {
        let preview: String = text.chars().take(80).collect();
        let result = self
            .app
            .notification()
            .builder()
            .title("Transcription pr\u{00ea}te")
            .body(&preview)
            .show();
        if let Err(e) = result {
            tracing::warn!(error = %e, "failed to send transcription notification");
        }
    }
}

/// Sends a desktop notification explaining that no microphone is connected.
///
/// Called from the capture thread when [`AudioCapture::open`] reports
/// [`SttError::NoInputDevice`] so a user whose dictation does nothing
/// understands why instead of the overlay silently never appearing.
fn notify_no_microphone(app: &tauri::AppHandle) {
    let result = app
        .notification()
        .builder()
        .title("Aucun microphone d\u{00e9}tect\u{00e9}")
        .body("Branchez un microphone pour utiliser la dict\u{00e9}e vocale.")
        .show();
    if let Err(e) = result {
        tracing::warn!(error = %e, "failed to send no-microphone notification");
    }
}

/// Turns a raw capture buffer into the audio the STT engine should see.
///
/// Resamples to 16 kHz mono, keeps only the audible span, then applies the
/// duration bounds. An `Err` means the recording must not reach the engine, and
/// carries the reason to report to the user.
///
/// Kept free of `self` so the decision is testable without a Tauri app handle:
/// the silence case in particular is what used to reach Whisper and come back as
/// invented filler.
fn prepare_audio(
    raw_samples: &[f32],
    sample_rate: u32,
    channels: u16,
    silence_threshold_db: f32,
    max_recording_sec: u32,
) -> Result<Vec<f32>, DictationFailure> {
    let whisper_audio = to_whisper_format(raw_samples, sample_rate, channels).map_err(|e| {
        tracing::warn!(error = %e, "audio resampling failed");
        DictationFailure::AudioUnusable
    })?;

    // `None` means no 10 ms window reached the threshold: the buffer is
    // silence. Whisper answers silence with training filler rather than an
    // empty string, so it must not be sent.
    let Some(trimmed) = trim_silence(&whisper_audio, silence_threshold_db) else {
        tracing::info!(
            samples = whisper_audio.len(),
            peak = peak_amplitude(&whisper_audio),
            threshold_db = silence_threshold_db,
            "stt.audio.nothing_audible"
        );
        return Err(DictationFailure::NothingAudible);
    };

    if trimmed.len() < MIN_SAMPLES {
        tracing::info!(
            samples = trimmed.len(),
            "audio too short (< 100 ms) - skipping transcription"
        );
        return Err(DictationFailure::TooShort);
    }

    let max_samples = max_recording_sec as usize * WHISPER_SAMPLE_RATE as usize;
    if trimmed.len() > max_samples {
        tracing::warn!(
            samples = trimmed.len(),
            max_samples,
            "audio exceeds max_recording_sec - truncating"
        );
        return Ok(trimmed[..max_samples].to_vec());
    }

    Ok(trimmed.to_vec())
}

/// Computes audio duration in milliseconds from sample count, rate, and channels.
fn compute_duration_ms(sample_count: usize, sample_rate: u32, channels: u16) -> u64 {
    let divisor = u64::from(sample_rate) * u64::from(channels);
    if divisor == 0 {
        return 0;
    }
    (sample_count as u64 * 1000) / divisor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_duration_ms_basic() {
        // GIVEN 16000 mono samples at 16 kHz
        // WHEN computing duration
        // THEN result is 1000 ms (1 second)
        assert_eq!(compute_duration_ms(16_000, 16_000, 1), 1_000);
    }

    #[test]
    fn compute_duration_ms_stereo() {
        // GIVEN 32000 interleaved stereo samples at 16 kHz
        // WHEN computing duration
        // THEN result is 1000 ms (each channel has 16000 samples)
        assert_eq!(compute_duration_ms(32_000, 16_000, 2), 1_000);
    }

    #[test]
    fn compute_duration_ms_zero_rate() {
        // GIVEN zero sample rate
        // WHEN computing duration
        // THEN returns 0 without panicking
        assert_eq!(compute_duration_ms(16_000, 0, 1), 0);
    }

    #[test]
    fn compute_duration_ms_zero_channels() {
        // GIVEN zero channels
        // WHEN computing duration
        // THEN returns 0 without panicking
        assert_eq!(compute_duration_ms(16_000, 16_000, 0), 0);
    }

    #[test]
    fn three_seconds_of_silence_never_reach_the_engine() {
        // GIVEN three seconds of digital silence at 16 kHz mono, long enough to
        // clear the 100 ms duration gate on its own
        let silence = vec![0.0_f32; 3 * 16_000];

        // WHEN the capture is prepared for the engine
        let outcome = prepare_audio(&silence, 16_000, 1, -40.0, 60);

        // THEN it is refused as nothing audible, so Whisper is never asked to
        // transcribe silence and cannot answer with invented filler
        assert_eq!(outcome, Err(DictationFailure::NothingAudible));
    }

    #[test]
    fn room_tone_below_the_threshold_never_reaches_the_engine() {
        // GIVEN three seconds of steady tone sitting under the -40 dB threshold,
        // which is what an open but unheard microphone produces
        let below = f32::powf(10.0, -40.0 / 20.0) * 0.5;
        let tone: Vec<f32> = (0..3 * 16_000)
            .map(|i| if i % 2 == 0 { below } else { -below })
            .collect();

        // WHEN the capture is prepared for the engine
        let outcome = prepare_audio(&tone, 16_000, 1, -40.0, 60);

        // THEN it is refused as nothing audible
        assert_eq!(outcome, Err(DictationFailure::NothingAudible));
    }

    #[test]
    fn audible_speech_reaches_the_engine_trimmed() {
        // GIVEN one second of silence, one second of signal, one of silence
        let mut audio = vec![0.0_f32; 3 * 16_000];
        audio[16_000..32_000].fill(0.4);

        // WHEN the capture is prepared for the engine
        let prepared = prepare_audio(&audio, 16_000, 1, -40.0, 60).expect("audible");

        // THEN only the audible second is kept
        assert_eq!(prepared.len(), 16_000);
        assert!(prepared.iter().all(|&s| s == 0.4));
    }

    #[test]
    fn audible_but_shorter_than_100ms_is_refused() {
        // GIVEN a 50 ms burst of signal padded with silence
        let mut audio = vec![0.0_f32; 16_000];
        audio[1_000..1_800].fill(0.4);

        // WHEN the capture is prepared for the engine
        let outcome = prepare_audio(&audio, 16_000, 1, -40.0, 60);

        // THEN it is refused as too short rather than transcribed
        assert_eq!(outcome, Err(DictationFailure::TooShort));
    }

    #[test]
    fn audio_longer_than_max_recording_sec_is_truncated() {
        // GIVEN two seconds of continuous signal with a one second cap
        let audio = vec![0.4_f32; 2 * 16_000];

        // WHEN the capture is prepared for the engine
        let prepared = prepare_audio(&audio, 16_000, 1, -40.0, 1).expect("audible");

        // THEN it is cut to the cap
        assert_eq!(prepared.len(), 16_000);
    }

    #[test]
    fn empty_capture_is_refused_as_unusable() {
        // GIVEN a capture that produced no samples at all
        // WHEN it is prepared for the engine
        let outcome = prepare_audio(&[], 16_000, 1, -40.0, 60);

        // THEN it is refused before any resampling or inference
        assert_eq!(outcome, Err(DictationFailure::AudioUnusable));
    }

    #[test]
    fn every_failure_reason_serialises_to_a_stable_key() {
        // GIVEN the reasons the webview has to map onto localised sentences
        // WHEN each is serialised for the Tauri event
        // THEN it is the snake_case key the frontend catalogue keys off
        let cases = [
            (DictationFailure::NoMicrophone, "no_microphone"),
            (DictationFailure::CaptureFailed, "capture_failed"),
            (DictationFailure::NoAudioCaptured, "no_audio_captured"),
            (DictationFailure::AudioUnusable, "audio_unusable"),
            (DictationFailure::NothingAudible, "nothing_audible"),
            (DictationFailure::TooShort, "too_short"),
            (DictationFailure::NoModel, "no_model"),
            (
                DictationFailure::TranscriptionFailed,
                "transcription_failed",
            ),
            (DictationFailure::EmptyTranscript, "empty_transcript"),
            (DictationFailure::Cancelled, "cancelled"),
        ];
        for (reason, expected) in cases {
            let json = serde_json::to_value(reason).expect("serialize");
            assert_eq!(json, serde_json::Value::String(expected.to_owned()));
        }
    }

    #[test]
    fn min_samples_matches_100ms_at_16khz() {
        // GIVEN the Whisper sample rate of 16 kHz
        // WHEN computing 100 ms worth of samples
        // THEN MIN_SAMPLES is correct
        assert_eq!(MIN_SAMPLES, 16_000 / 10);
    }
}
