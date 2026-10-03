//! Where a dictation captured by this window is transcribed: the engine of the
//! runtime in this process, or the daemon the window is attached to.
//!
//! The microphone is always the one of the machine that shows the window; only
//! the engine, and the history the transcription lands in, move with the
//! runtime.

use std::io::Cursor;

use apollia_runtime::api::server::SharedSttEngine;
use apollia_runtime::stt::TranscriptSource;
use serde::Deserialize;

use crate::attached::{AttachedClient, AttachedError};

/// Whisper model sample rate in Hz, the rate a prepared dictation is at.
const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// The engine a dictation goes to.
pub(crate) enum Transcriber {
    /// The shared engine cell of the runtime in this process, read on each
    /// dictation so a model brought online mid-session is picked up.
    Local(SharedSttEngine),
    /// The daemon this window is attached to, sent the audio as a WAV.
    Daemon(AttachedClient),
}

/// What became of a dictation handed to the engine.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Transcribed {
    /// The text the engine heard.
    Text(String),
    /// No engine is running: no model is loaded, or dictation is off.
    NoModel,
    /// The engine was reached and failed.
    Failed,
}

/// The part of the daemon's answer a dictation reads.
#[derive(Deserialize)]
struct Row {
    full_text: String,
}

impl Transcriber {
    /// Transcribes 16 kHz mono audio, kept in the history as a dictation.
    pub(crate) async fn transcribe(&self, audio: Vec<f32>) -> Transcribed {
        match self {
            Self::Local(cell) => {
                let Some(engine) = cell.read().await.clone() else {
                    return Transcribed::NoModel;
                };
                match engine
                    .transcribe(audio, WHISPER_SAMPLE_RATE, TranscriptSource::Hotkey)
                    .await
                {
                    Ok(transcript) => Transcribed::Text(transcript.full_text),
                    Err(e) => {
                        tracing::warn!(error = %e, "stt.transcription.failed");
                        Transcribed::Failed
                    }
                }
            }
            Self::Daemon(remote) => {
                let wav = match encode_wav(&audio) {
                    Ok(wav) => wav,
                    Err(e) => {
                        tracing::warn!(error = %e, "stt.dictation.encode.failed");
                        return Transcribed::Failed;
                    }
                };
                daemon_outcome(
                    remote
                        .transcribe::<Row>(&wav, "dictation.wav", "hotkey")
                        .await,
                )
            }
        }
    }
}

/// Reads the daemon's answer to a dictation. A 503 is the daemon saying no
/// engine runs, which the window reports as it reports a missing model.
fn daemon_outcome(answer: Result<Row, AttachedError>) -> Transcribed {
    match answer {
        Ok(row) => Transcribed::Text(row.full_text),
        Err(AttachedError::Refused { status: 503, .. }) => Transcribed::NoModel,
        Err(e) => {
            tracing::warn!(error = %e, "stt.transcription.failed");
            Transcribed::Failed
        }
    }
}

/// Encodes prepared audio as a 32-bit float mono WAV at the Whisper rate, the
/// form the daemon's transcription route decodes without loss.
fn encode_wav(audio: &[f32]) -> Result<Vec<u8>, hound::Error> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: WHISPER_SAMPLE_RATE,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut wav = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut wav, spec)?;
    for sample in audio {
        writer.write_sample(*sample)?;
    }
    writer.finalize()?;
    Ok(wav.into_inner())
}

#[cfg(test)]
mod tests;
