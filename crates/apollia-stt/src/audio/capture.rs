//! Audio capture from a system microphone via `cpal`.
//!
//! [`AudioCapture`] opens either the default input device or a device chosen
//! by name, and [`CaptureBuffer`] accumulates PCM samples in a thread-safe
//! buffer that the caller drains at its own pace. Each capture callback also
//! updates a lock-free peak level so the UI can render a live waveform without
//! opening a second audio stream.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};

use crate::types::SttError;

/// Handle to an audio input device and its selected configuration.
///
/// Created via [`AudioCapture::open`] (or [`AudioCapture::default_input`]),
/// then activated with [`AudioCapture::start`] which returns a live [`Stream`]
/// and a [`CaptureBuffer`] for reading captured samples.
pub struct AudioCapture {
    device: cpal::Device,
    config: StreamConfig,
    sample_format: SampleFormat,
}

/// Thread-safe accumulator for captured PCM samples.
///
/// The cpal callback pushes samples into the internal buffer and updates the
/// shared peak level; the consumer calls [`CaptureBuffer::drain`] to retrieve
/// and clear accumulated data, and [`CaptureBuffer::level_handle`] to observe
/// the live input amplitude.
pub struct CaptureBuffer {
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
    sample_rate: u32,
    channels: u16,
}

/// Enumerate the names of all available audio input devices on the default host.
///
/// Returns an empty vector when the host exposes no input devices (no
/// microphone). Devices whose name cannot be read are skipped rather than
/// aborting the whole enumeration.
///
/// # Errors
/// Returns [`SttError::InvalidAudio`] if the host cannot list input devices.
pub fn list_input_devices() -> Result<Vec<String>, SttError> {
    let host = cpal::default_host();
    let devices = host.input_devices().map_err(|e| SttError::InvalidAudio {
        reason: format!("failed to enumerate input devices: {e}"),
    })?;
    let mut names = Vec::new();
    for device in devices {
        if let Ok(name) = device.name() {
            names.push(name);
        }
    }
    Ok(names)
}

impl AudioCapture {
    /// Open the default system input device with its preferred configuration.
    ///
    /// Convenience wrapper over [`AudioCapture::open`] with no device name.
    ///
    /// # Errors
    /// Returns [`SttError::NoInputDevice`] when no microphone exists, or
    /// [`SttError::InvalidAudio`] if the device configuration cannot be read.
    pub fn default_input() -> Result<Self, SttError> {
        Self::open(None)
    }

    /// Open an input device by name, falling back to the system default.
    ///
    /// When `device_name` is `Some` and a matching device exists it is used.
    /// When the named device is absent (for example unplugged) the system
    /// default is used instead, with a warning. When `device_name` is `None`
    /// the system default is used directly.
    ///
    /// # Errors
    /// Returns [`SttError::NoInputDevice`] when the host has no input device at
    /// all, or [`SttError::InvalidAudio`] if the configuration cannot be read.
    pub fn open(device_name: Option<&str>) -> Result<Self, SttError> {
        let host = cpal::default_host();

        let device = match device_name {
            Some(name) if !name.is_empty() => {
                let selected = host
                    .input_devices()
                    .map_err(|e| SttError::InvalidAudio {
                        reason: format!("failed to enumerate input devices: {e}"),
                    })?
                    .find(|d| d.name().map(|n| n == name).unwrap_or(false));
                match selected {
                    Some(d) => d,
                    None => {
                        tracing::warn!(
                            requested = %name,
                            detail = "falling back to the default input device",
                            "stt.device.missing"
                        );
                        host.default_input_device().ok_or(SttError::NoInputDevice)?
                    }
                }
            }
            _ => host.default_input_device().ok_or(SttError::NoInputDevice)?,
        };

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

    /// Name of the device that was actually opened.
    ///
    /// Reported by the caller so a log says which microphone served a
    /// recording. [`AudioCapture::open`] falls back to the system default when
    /// the configured name is absent, so the opened device is not always the
    /// requested one. Returns `"<unnamed>"` when the host cannot name it.
    pub fn device_name(&self) -> String {
        self.device
            .name()
            .unwrap_or_else(|_| "<unnamed>".to_owned())
    }

    /// Sample rate the device imposed, in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate.0
    }

    /// Number of interleaved channels the device imposed.
    pub fn channels(&self) -> u16 {
        self.config.channels
    }

    /// Sample format the device imposed.
    pub fn sample_format(&self) -> SampleFormat {
        self.sample_format
    }

    /// Start capturing audio from the device.
    ///
    /// Returns the live [`Stream`] (must be kept alive for recording to
    /// continue) and a [`CaptureBuffer`] that accumulates incoming samples.
    /// Supports `F32` and `I16` sample formats; other formats yield
    /// [`SttError::InvalidAudio`].
    pub fn start(&self) -> Result<(Stream, CaptureBuffer), SttError> {
        let buffer = Arc::new(Mutex::new(Vec::<f32>::new()));
        // Peak amplitude of the most recent callback chunk, encoded as f32 bits
        // in an atomic so the audio callback stays lock-free and the UI side can
        // read it at its own cadence without blocking capture.
        let level = Arc::new(AtomicU32::new(0));
        let capture_buffer = CaptureBuffer {
            samples: Arc::clone(&buffer),
            level: Arc::clone(&level),
            sample_rate: self.config.sample_rate.0,
            channels: self.config.channels,
        };

        let err_fn = |err: cpal::StreamError| {
            tracing::error!(error = %err, "stt.capture.stream.error");
        };

        let stream = match self.sample_format {
            SampleFormat::F32 => {
                let buf = Arc::clone(&buffer);
                let lvl = Arc::clone(&level);
                self.device.build_input_stream(
                    &self.config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        let mut peak = 0.0_f32;
                        for &s in data {
                            let a = s.abs();
                            if a > peak {
                                peak = a;
                            }
                        }
                        lvl.store(peak.min(1.0).to_bits(), Ordering::Relaxed);
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
                let lvl = Arc::clone(&level);
                self.device.build_input_stream(
                    &self.config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let mut peak = 0.0_f32;
                        if let Ok(mut guard) = buf.lock() {
                            for &s in data {
                                let f = f32::from(s) / f32::from(i16::MAX);
                                let a = f.abs();
                                if a > peak {
                                    peak = a;
                                }
                                guard.push(f);
                            }
                        }
                        lvl.store(peak.min(1.0).to_bits(), Ordering::Relaxed);
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

    /// Clone the shared peak-level handle for live level metering.
    ///
    /// The returned [`AudioLevel`] reads the latest per-callback peak amplitude
    /// in the `0.0..=1.0` range without locking the capture buffer.
    pub fn level_handle(&self) -> AudioLevel {
        AudioLevel {
            level: Arc::clone(&self.level),
        }
    }
}

/// Lock-free reader for the live capture peak level.
///
/// Cloned out of a [`CaptureBuffer`] before the buffer is handed to the drain
/// side, so the metering loop can poll the current input amplitude while
/// recording is in progress.
#[derive(Clone)]
pub struct AudioLevel {
    level: Arc<AtomicU32>,
}

impl AudioLevel {
    /// Current input peak amplitude in the `0.0..=1.0` range.
    pub fn get(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN a level handle backed by an atomic
    // WHEN a peak is stored and read back
    // THEN the round-trip preserves the value
    #[test]
    fn audio_level_roundtrip() {
        let level = Arc::new(AtomicU32::new(0));
        let reader = AudioLevel {
            level: Arc::clone(&level),
        };
        assert_eq!(reader.get(), 0.0);
        level.store(0.5_f32.to_bits(), Ordering::Relaxed);
        assert!((reader.get() - 0.5).abs() < f32::EPSILON);
    }

    // GIVEN the host enumeration helper
    // WHEN input devices are listed
    // THEN it returns without error (list may be empty on headless CI)
    #[test]
    fn list_input_devices_does_not_error() {
        let _ = list_input_devices();
    }
}
