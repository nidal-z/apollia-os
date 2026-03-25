//! Audio capture from the default system microphone via `cpal`.
//!
//! [`AudioCapture`] opens the default input device and [`CaptureBuffer`]
//! accumulates PCM samples in a thread-safe buffer that the caller drains
//! at its own pace.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};

use crate::types::SttError;

/// Handle to the default audio input device and its selected configuration.
///
/// Created via [`AudioCapture::default_input`], then activated with
/// [`AudioCapture::start`] which returns a live [`Stream`] and a
/// [`CaptureBuffer`] for reading captured samples.
pub struct AudioCapture {
    device: cpal::Device,
    config: StreamConfig,
    sample_format: SampleFormat,
}

/// Thread-safe accumulator for captured PCM samples.
///
/// The cpal callback pushes samples into the internal buffer; the consumer
/// calls [`CaptureBuffer::drain`] to retrieve and clear accumulated data.
pub struct CaptureBuffer {
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    channels: u16,
}

impl AudioCapture {
    /// Open the default system input device with its preferred configuration.
    ///
    /// Returns [`SttError::InvalidAudio`] if no input device is available or
    /// the device configuration cannot be read.
    pub fn default_input() -> Result<Self, SttError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| SttError::InvalidAudio {
                reason: "no default audio input device found".to_owned(),
            })?;
        let supported = device
            .default_input_config()
            .map_err(|e| SttError::InvalidAudio {
                reason: format!("failed to get default input config: {e}"),
            })?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();

        Ok(Self {
            device,
            config,
            sample_format,
        })
    }

    /// Start capturing audio from the device.
    ///
    /// Returns the live [`Stream`] (must be kept alive for recording to
    /// continue) and a [`CaptureBuffer`] that accumulates incoming samples.
    /// Supports `F32` and `I16` sample formats; other formats yield
    /// [`SttError::InvalidAudio`].
    pub fn start(&self) -> Result<(Stream, CaptureBuffer), SttError> {
        let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
        let capture_buffer = CaptureBuffer {
            samples: Arc::clone(&buffer),
            sample_rate: self.config.sample_rate.0,
            channels: self.config.channels,
        };

        let err_fn = |err: cpal::StreamError| {
            tracing::error!(error = %err, "audio capture stream error");
        };

        let stream = match self.sample_format {
            SampleFormat::F32 => {
                let buf = Arc::clone(&buffer);
                self.device.build_input_stream(
                    &self.config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if let Ok(mut guard) = buf.lock() {
                            guard.extend_from_slice(data);
                        }
                    },
                    err_fn,
                    None,
                )
            }
            SampleFormat::I16 => {
                let buf = Arc::clone(&buffer);
                self.device.build_input_stream(
                    &self.config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if let Ok(mut guard) = buf.lock() {
                            guard.extend(data.iter().map(|&s| f32::from(s) / f32::from(i16::MAX)));
                        }
                    },
                    err_fn,
                    None,
                )
            }
            other => {
                return Err(SttError::InvalidAudio {
                    reason: format!("unsupported sample format: {other:?}"),
                });
            }
        }
        .map_err(|e| SttError::InvalidAudio {
            reason: format!("failed to build input stream: {e}"),
        })?;

        stream.play().map_err(|e| SttError::InvalidAudio {
            reason: format!("failed to start audio stream: {e}"),
        })?;

        Ok((stream, capture_buffer))
    }
}

impl CaptureBuffer {
    /// Drain and return all accumulated samples, clearing the internal buffer.
    pub fn drain(&self) -> Vec<f32> {
        self.samples
            .lock()
            .map(|mut guard| std::mem::take(&mut *guard))
            .unwrap_or_default()
    }

    /// Sample rate of the captured audio in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Number of interleaved channels in the captured audio.
    pub fn channels(&self) -> u16 {
        self.channels
    }
}
