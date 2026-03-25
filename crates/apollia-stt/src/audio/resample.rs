//! Audio resampling to the 16 kHz mono f32 format expected by Whisper.
//!
//! Uses `rubato::SincFixedIn` for high-quality sample-rate conversion and
//! simple averaging for channel down-mixing.

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};

use crate::types::SttError;

/// Target sample rate expected by Whisper models.
const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// Convert interleaved PCM samples to Whisper's expected format: mono 16 kHz f32.
///
/// When the source is already mono 16 kHz the input is returned as-is (passthrough).
/// Otherwise channels are mixed down to mono first, then resampled via a sinc
/// interpolator.
///
/// # Errors
///
/// Returns [`SttError::InvalidAudio`] if the input is empty, `channels` is zero,
/// or the resampler encounters an internal error.
pub fn to_whisper_format(
    samples: &[f32],
    from_sample_rate: u32,
    channels: u16,
) -> Result<Vec<f32>, SttError> {
    if samples.is_empty() {
        return Err(SttError::InvalidAudio {
            reason: "audio buffer is empty".to_owned(),
        });
    }
    if channels == 0 {
        return Err(SttError::InvalidAudio {
            reason: "channel count must be >= 1".to_owned(),
        });
    }

    let mono = if channels == 1 {
        samples.to_vec()
    } else {
        mix_to_mono(samples, channels)
    };

    if from_sample_rate == WHISPER_SAMPLE_RATE && channels == 1 {
        return Ok(samples.to_vec());
    }

    if from_sample_rate == WHISPER_SAMPLE_RATE {
        return Ok(mono);
    }

    resample(&mono, from_sample_rate)
}

/// Average interleaved multi-channel samples down to a single mono channel.
fn mix_to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels as usize;
    samples
        .chunks_exact(ch)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Resample mono audio from `from_sr` to 16 kHz using a sinc interpolator.
fn resample(mono: &[f32], from_sr: u32) -> Result<Vec<f32>, SttError> {
    let ratio = f64::from(WHISPER_SAMPLE_RATE) / f64::from(from_sr);
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let chunk_size = mono.len();
    let mut resampler =
        SincFixedIn::<f32>::new(ratio, 2.0, params, chunk_size, 1).map_err(|e| {
            SttError::InvalidAudio {
                reason: format!("failed to create resampler: {e}"),
            }
        })?;

    let input = vec![mono.to_vec()];
    let output = resampler
        .process(&input, None)
        .map_err(|e| SttError::InvalidAudio {
            reason: format!("resampling failed: {e}"),
        })?;

    output
        .into_iter()
        .next()
        .ok_or_else(|| SttError::InvalidAudio {
            reason: "resampler produced no output channels".to_owned(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN mono 16kHz audio
    // WHEN to_whisper_format is called
    // THEN it returns an identical copy (passthrough)
    #[test]
    fn passthrough_mono_16khz() {
        let input: Vec<f32> = (0..1600).map(|i| (i as f32 * 0.001).sin()).collect();
        let out = to_whisper_format(&input, 16_000, 1).expect("passthrough should succeed");
        assert_eq!(out.len(), input.len());
        assert_eq!(out, input);
    }

    // GIVEN stereo 16kHz audio
    // WHEN to_whisper_format is called
    // THEN it mixes to mono (half the samples) without resampling
    #[test]
    fn stereo_to_mono_no_resample() {
        let frames = 800;
        let mut stereo = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let v = (i as f32 * 0.01).sin();
            stereo.push(v);
            stereo.push(v * 0.5);
        }
        let out = to_whisper_format(&stereo, 16_000, 2).expect("stereo mix should succeed");
        assert_eq!(out.len(), frames);
        for (i, &s) in out.iter().enumerate() {
            let v = (i as f32 * 0.01).sin();
            let expected = (v + v * 0.5) / 2.0;
            assert!((s - expected).abs() < 1e-5, "frame {i} mismatch");
        }
    }

    // GIVEN mono 48kHz audio (3x oversampled)
    // WHEN to_whisper_format is called
    // THEN the output length is approximately input_len / 3
    #[test]
    fn resample_48khz_to_16khz() {
        let input: Vec<f32> = (0..48_000).map(|i| (i as f32 * 0.001).sin()).collect();
        let out = to_whisper_format(&input, 48_000, 1).expect("resample should succeed");
        let expected_len = 16_000;
        let tolerance = (expected_len as f32 * 0.05) as usize;
        assert!(
            out.len().abs_diff(expected_len) <= tolerance,
            "expected ~{expected_len} samples, got {}",
            out.len()
        );
    }

    // GIVEN stereo 44.1kHz audio
    // WHEN to_whisper_format is called
    // THEN it mixes to mono AND resamples to 16kHz
    #[test]
    fn stereo_44100_to_mono_16khz() {
        let frames = 44_100;
        let mut stereo = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let v = (i as f32 * 0.001).sin();
            stereo.push(v);
            stereo.push(-v);
        }
        let out = to_whisper_format(&stereo, 44_100, 2).expect("resample+mix should succeed");
        let expected_len = 16_000;
        let tolerance = (expected_len as f32 * 0.05) as usize;
        assert!(
            out.len().abs_diff(expected_len) <= tolerance,
            "expected ~{expected_len} samples, got {}",
            out.len()
        );
    }

    // GIVEN an empty buffer
    // WHEN to_whisper_format is called
    // THEN it returns InvalidAudio
    #[test]
    fn empty_buffer_returns_error() {
        let err = to_whisper_format(&[], 16_000, 1).unwrap_err();
        assert!(
            matches!(err, SttError::InvalidAudio { .. }),
            "expected InvalidAudio, got {err:?}"
        );
    }

    // GIVEN zero channels
    // WHEN to_whisper_format is called
    // THEN it returns InvalidAudio
    #[test]
    fn zero_channels_returns_error() {
        let err = to_whisper_format(&[0.1, 0.2], 16_000, 0).unwrap_err();
        assert!(
            matches!(err, SttError::InvalidAudio { .. }),
            "expected InvalidAudio, got {err:?}"
        );
    }
}
