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

    // GIVEN audio with silence at start and end, voice in the middle
    // WHEN trim_silence is called
    // THEN leading and trailing silence is removed
    #[test]
    fn trims_leading_and_trailing_silence() {
        let window = 160;
        let mut audio = vec![0.0f32; window * 10];
        // Windows 3..7 contain voice
        for i in (3 * window)..(7 * window) {
            audio[i] = 0.5;
        }
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
}
