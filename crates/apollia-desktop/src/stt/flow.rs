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

use apollia_core::{EventBusSender, RuntimeEvent, SttConfig};
use apollia_stt::{to_whisper_format, trim_silence, AudioCapture, CaptureBuffer};
use tauri_plugin_notification::NotificationExt;

use apollia_runtime::stt::{SttEngineHandle, TranscriptSource};

use crate::stt::clipboard;

/// Whisper model sample rate in Hz.
const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// Minimum audio duration in samples before calling the STT engine.
///
/// 100 ms at 16 kHz = 1 600 samples. Shorter recordings are discarded
/// to avoid wasting inference on noise or accidental hotkey presses.
const MIN_SAMPLES: usize = 1_600;

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
    config: SttConfig,
    /// Handle to the SttEngine actor for transcription requests.
    stt_engine: SttEngineHandle,
    /// EventBus sender for lifecycle event broadcasting.
    event_bus: EventBusSender,
    /// Tauri application handle for desktop notifications.
    app: tauri::AppHandle,
    /// Shared flag tracking whether recording is currently active.
    recording: Arc<AtomicBool>,
    /// Active capture buffer accumulating microphone samples.
    active_buffer: Arc<Mutex<Option<CaptureBuffer>>>,
    /// Sender to signal the audio thread to stop. Dropping the sender
    /// unblocks the `recv()` on the audio thread, releasing the stream.
    stop_tx: Arc<Mutex<Option<std::sync::mpsc::Sender<()>>>>,
}

impl SttFlow {
    /// Creates a new orchestrator from the STT configuration and runtime handles.
    pub fn new(
        config: SttConfig,
        stt_engine: SttEngineHandle,
        event_bus: EventBusSender,
        app: tauri::AppHandle,
    ) -> Self {
        Self {
            config,
            stt_engine,
            event_bus,
            app,
            recording: Arc::new(AtomicBool::new(false)),
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
    pub fn start_recording(&self) {
        if self.recording.swap(true, Ordering::SeqCst) {
            tracing::warn!("start_recording called while already recording");
            return;
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

        std::thread::spawn(move || {
            let capture = match AudioCapture::default_input() {
                Ok(c) => c,
                Err(e) => {
                    recording.store(false, Ordering::SeqCst);
                    tracing::warn!(error = %e, "failed to open audio input device");
                    return;
                }
            };

            let (stream, buffer) = match capture.start() {
                Ok(pair) => pair,
                Err(e) => {
                    recording.store(false, Ordering::SeqCst);
                    tracing::warn!(error = %e, "failed to start audio capture");
                    return;
                }
            };

            // Publish the buffer so the async side can drain it later.
            if let Ok(mut guard) = active_buffer.lock() {
                *guard = Some(buffer);
            }

            let _ = event_bus.send(RuntimeEvent::SttRecordingStarted);
            tracing::info!("STT recording started");

            // Block until stop signal, keeping the stream alive. When the
            // sender is dropped (or an explicit () is sent), recv() unblocks.
            let _ = stop_rx.recv();

            // stream is dropped here, releasing the audio device.
            drop(stream);
        });
    }

    /// Stops recording, processes the audio, and dispatches the result.
    ///
    /// Pipeline: drain buffer → signal audio thread to stop → resample to
    /// 16 kHz mono → trim silence → transcribe via SttEngine → inject into
    /// clipboard and/or send notification depending on `clipboard_mode`.
    ///
    /// Emits [`RuntimeEvent::SttRecordingStopped`] with the raw audio duration.
    /// Recordings shorter than 100 ms are discarded without calling the STT engine.
    pub async fn stop_and_transcribe(&self) {
        if !self.recording.swap(false, Ordering::SeqCst) {
            tracing::debug!("stop_and_transcribe called while not recording");
            return;
        }

        // Drain the buffer BEFORE stopping the audio thread. The cpal callback
        // may still push a few samples between drain and stop — that is fine.
        let (raw_samples, sample_rate, channels) = match self.drain_buffer() {
            Some(data) => data,
            None => {
                self.signal_stop();
                tracing::warn!("no active buffer — recording may have failed to start");
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

        // Resample to Whisper format (16 kHz mono f32).
        let whisper_audio = match to_whisper_format(&raw_samples, sample_rate, channels) {
            Ok(audio) => audio,
            Err(e) => {
                tracing::warn!(error = %e, "audio resampling failed");
                return;
            }
        };

        // Trim leading and trailing silence.
        let trimmed = trim_silence(&whisper_audio, self.config.silence_threshold_db);

        // Discard recordings shorter than 100 ms.
        if trimmed.len() < MIN_SAMPLES {
            tracing::info!(
                samples = trimmed.len(),
                "audio too short (< 100 ms) — skipping transcription"
            );
            return;
        }

        // Truncate if exceeding the configured maximum recording duration.
        let max_samples = self.config.max_recording_sec as usize * WHISPER_SAMPLE_RATE as usize;
        let final_audio = if trimmed.len() > max_samples {
            tracing::warn!(
                samples = trimmed.len(),
                max_samples,
                "audio exceeds max_recording_sec — truncating"
            );
            &trimmed[..max_samples]
        } else {
            trimmed
        };

        // Transcribe via the SttEngine actor.
        let transcript = match self
            .stt_engine
            .transcribe(
                final_audio.to_vec(),
                WHISPER_SAMPLE_RATE,
                TranscriptSource::Hotkey,
            )
            .await
        {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "STT transcription failed");
                return;
            }
        };

        if transcript.full_text.trim().is_empty() {
            tracing::info!("transcription result is empty — no output");
            return;
        }

        self.dispatch_result(&transcript.full_text).await;
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
    /// | `"clipboard"` | clipboard only — user pastes manually                           |
    /// | `"memo"`      | desktop notification only                                       |
    /// | `"both"`      | clipboard → paste + notification                                |
    async fn dispatch_result(&self, text: &str) {
        let mode = self.config.clipboard_mode.as_str();
        tracing::debug!(mode, len = text.len(), "dispatching transcription result");

        match mode {
            "paste" | "both" => self.inject_clipboard(text).await,
            "clipboard"      => self.write_clipboard(text),
            _                => {}
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
    /// Step 2: async sleep — lets the WindowServer propagate the clipboard.
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
    fn min_samples_matches_100ms_at_16khz() {
        // GIVEN the Whisper sample rate of 16 kHz
        // WHEN computing 100 ms worth of samples
        // THEN MIN_SAMPLES is correct
        assert_eq!(MIN_SAMPLES, 16_000 / 10);
    }
}
