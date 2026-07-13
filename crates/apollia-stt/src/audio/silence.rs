//! Silence detection and trimming for pre-processed PCM audio.
//!
//! Operates on 16 kHz mono f32 buffers using RMS energy measured over
//! 160-sample windows (10 ms at 16 kHz).

/// Trim leading and trailing silence from a 16 kHz mono audio buffer.
///
/// Silence is detected per 160-sample window (10 ms at 16 kHz) by comparing
/// the window's RMS energy to a linear threshold derived from `threshold_db`.
///
/// Returns the trimmed sub-slice. If the entire buffer is below the threshold,
/// the original slice is returned unchanged (no panic, no empty result).
pub fn trim_silence(audio: &[f32], threshold_db: f32) -> &[f32] {
    if audio.is_empty() {
        return audio;
    }

    let threshold_linear = f32::powf(10.0, threshold_db / 20.0);
    let window_size: usize = 160;

    let window_count = audio.len() / window_size;
    if window_count == 0 {
        return audio;
    }

    let is_voice = |window_idx: usize| -> bool {
        let start = window_idx * window_size;
        let end = start + window_size;
        let rms = rms_energy(&audio[start..end]);
        rms >= threshold_linear
    };

    let first_voice = match (0..window_count).find(|&i| is_voice(i)) {
        Some(i) => i,
        None => return audio,
    };

    let last_voice = (0..window_count)
        .rfind(|&i| is_voice(i))
        .unwrap_or(first_voice);

    let start_sample = first_voice * window_size;
    let end_sample = ((last_voice + 1) * window_size).min(audio.len());

    &audio[start_sample..end_sample]
}

/// Compute the RMS (root mean square) energy of a sample window.
fn rms_energy(window: &[f32]) -> f32 {
    if window.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = window.iter().map(|&s| s * s).sum();
    (sum_sq / window.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    mod test_helpers {
        /// Returns a buffer of `samples` zero-valued samples (pure silence).
        pub(super) fn generate_silence(samples: usize) -> Vec<f32> {
            vec![0.0f32; samples]
        }

        /// Returns a buffer filled with a constant amplitude (acts as voice for any threshold below it).
        pub(super) fn generate_voice(samples: usize, amplitude: f32) -> Vec<f32> {
            vec![amplitude; samples]
        }
    }

    use test_helpers::*;

    // GIVEN audio with silence at start and end, voice in the middle
    // WHEN trim_silence is called
    // THEN leading and trailing silence is removed
    #[test]
    fn trims_leading_and_trailing_silence() {
        let window = 160;
        let mut audio = vec![0.0f32; window * 10];
        // Windows 3..7 contain voice
        audio[(3 * window)..(7 * window)].fill(0.5);
        let trimmed = trim_silence(&audio, -40.0);
        assert_eq!(trimmed.len(), 4 * window);
        assert!(trimmed.iter().all(|&s| s == 0.5));
    }

    // GIVEN entirely silent audio
    // WHEN trim_silence is called
    // THEN the original slice is returned
    #[test]
    fn all_silent_returns_original() {
        let audio = vec![0.0f32; 1600];
        let trimmed = trim_silence(&audio, -40.0);
        assert_eq!(trimmed.len(), audio.len());
        assert!(std::ptr::eq(trimmed.as_ptr(), audio.as_ptr()));
    }

    // GIVEN an empty buffer
    // WHEN trim_silence is called
    // THEN it returns an empty slice without panic
    #[test]
    fn empty_audio_returns_empty() {
        let audio: &[f32] = &[];
        let trimmed = trim_silence(audio, -40.0);
        assert!(trimmed.is_empty());
    }

    // GIVEN audio shorter than one window
    // WHEN trim_silence is called
    // THEN the original slice is returned unchanged
    #[test]
    fn short_audio_returns_original() {
        let audio = vec![0.3f32; 100];
        let trimmed = trim_silence(&audio, -40.0);
        assert_eq!(trimmed.len(), 100);
    }

    // GIVEN audio where every window is above the threshold
    // WHEN trim_silence is called
    // THEN all complete windows are kept
    #[test]
    fn all_voice_keeps_all_windows() {
        let window = 160;
        let audio: Vec<f32> = (0..(window * 5))
            .map(|i| (i as f32 * 0.1).sin().abs() + 0.1)
            .collect();
        let trimmed = trim_silence(&audio, -40.0);
        assert_eq!(trimmed.len(), window * 5);
    }

    // GIVEN a buffer with silence at the start only (voice reaches the end)
    // WHEN trim_silence is called
    // THEN only the leading silence is removed; the voice region is fully preserved
    #[test]
    fn test_trim_silence_at_start_only() {
        let window = 160;
        let silence = generate_silence(3 * window);
        let voice = generate_voice(5 * window, 0.5);
        let audio: Vec<f32> = silence.into_iter().chain(voice).collect();

        // WHEN
        let trimmed = trim_silence(&audio, -40.0);

        // THEN voice region (5 windows) is returned
        assert_eq!(trimmed.len(), 5 * window);
        assert!(trimmed.iter().all(|&s| s == 0.5));
    }

    // GIVEN a buffer with silence at the end only (voice starts at the beginning)
    // WHEN trim_silence is called
    // THEN only the trailing silence is removed; the voice region is fully preserved
    #[test]
    fn test_trim_silence_at_end_only() {
        let window = 160;
        let voice = generate_voice(5 * window, 0.5);
        let silence = generate_silence(3 * window);
        let audio: Vec<f32> = voice.into_iter().chain(silence).collect();

        // WHEN
        let trimmed = trim_silence(&audio, -40.0);

        // THEN voice region (5 windows) is returned
        assert_eq!(trimmed.len(), 5 * window);
        assert!(trimmed.iter().all(|&s| s == 0.5));
    }

    // GIVEN a signal just above and just below the RMS threshold
    // WHEN trim_silence is called for each case
    // THEN a signal above threshold is detected as voice; one below is treated as silence
    #[test]
    fn test_rms_threshold_boundary() {
        let window = 160;
        let threshold_db = -40.0_f32;
        // threshold_linear = 10^(threshold_db/20) = 10^(-2) ≈ 0.01
        let threshold_linear = f32::powf(10.0, threshold_db / 20.0);

        // Signal clearly above threshold: RMS is unambiguously >= threshold_linear.
        // Using a 1% margin avoids f32 accumulation drift in the sum-of-squares.
        let just_above = threshold_linear * 1.01;
        let just_below = threshold_linear * 0.99;

        // --- above-threshold case: voice windows are preserved ---
        // GIVEN 6 windows: 2 silent, 2 just-above-threshold, 2 silent
        let mut audio_above = generate_silence(6 * window);
        for sample in audio_above.iter_mut().skip(2 * window).take(2 * window) {
            *sample = just_above;
        }
        // WHEN
        let trimmed_above = trim_silence(&audio_above, threshold_db);
        // THEN the 2 voice windows are kept
        assert_eq!(
            trimmed_above.len(),
            2 * window,
            "just-above-threshold signal should be classified as voice"
        );

        // --- below-threshold case: buffer is returned as-is (no voice found) ---
        // GIVEN 6 windows all just below threshold
        let audio_below = generate_voice(6 * window, just_below);
        // WHEN
        let trimmed_below = trim_silence(&audio_below, threshold_db);
        // THEN no voice is found → original slice returned unchanged
        assert_eq!(
            trimmed_below.len(),
            audio_below.len(),
            "just-below-threshold signal should not be detected as voice"
        );
    }

    // GIVEN a buffer with two voice regions separated by internal silence
    // WHEN trim_silence is called
    // THEN only leading and trailing silence are removed; the internal gap is preserved
    #[test]
    fn test_trim_multiple_voice_regions() {
        let window = 160;
        // Layout: [2 silent][3 voice][2 silent][3 voice][2 silent] = 12 windows
        let mut audio = generate_silence(12 * window);
        for sample in audio.iter_mut().skip(2 * window).take(3 * window) {
            *sample = 0.5;
        }
        for sample in audio.iter_mut().skip(7 * window).take(3 * window) {
            *sample = 0.5;
        }

        // WHEN
        let trimmed = trim_silence(&audio, -40.0);

        // THEN windows 2..10 are kept (first_voice=2, last_voice=9)
        assert_eq!(trimmed.len(), 8 * window);
    }

    // GIVEN a window of all-zero samples
    // WHEN rms_energy is called
    // THEN the result is exactly 0.0 (silence has zero energy)
    #[test]
    fn test_rms_energy_zero_is_silence() {
        // GIVEN
        let zeros = generate_silence(160);

        // WHEN
        let rms = rms_energy(&zeros);

        // THEN
        assert_eq!(rms, 0.0);
    }
}
