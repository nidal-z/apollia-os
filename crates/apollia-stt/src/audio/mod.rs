//! Audio pipeline: capture, resample, and silence trimming.
//!
//! This module provides the three stages of audio pre-processing required
//! before passing samples to an [`super::SttBackend`]:
//!
//! 1. **Capture** ([`capture::AudioCapture`]) - record from the default mic via `cpal`.
//! 2. **Resample** ([`resample::to_whisper_format`]) - convert to 16 kHz mono f32.
//! 3. **Silence trim** ([`silence::trim_silence`]) - remove leading/trailing
//!    silence, or report that nothing audible was captured at all.

pub mod capture;
pub mod resample;
pub mod silence;

pub use capture::{list_input_devices, AudioCapture, AudioLevel, CaptureBuffer};
pub use resample::to_whisper_format;
pub use silence::{peak_amplitude, trim_silence};
