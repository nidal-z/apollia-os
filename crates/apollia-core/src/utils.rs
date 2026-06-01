//! Utility functions shared across the Apollia runtime.

/// Returns the largest UTF-8 character boundary that is `<= index`.
///
/// Stable equivalent of the nightly `str::floor_char_boundary` method.
/// Guarantees the returned value is a valid position to slice `s`.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    let mut i = index.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Truncates text while keeping the head and tail, dropping the middle.
///
/// Avoids losing critical information: the head holds the context and
/// declarations, the tail holds recent results. The removed middle is replaced
/// by a message stating how many lines were dropped.
///
/// `max_chars` is expressed in UTF-8 bytes, a reasonable proxy for tool outputs
/// that are ASCII or basic UTF-8 text. Slicing always respects valid UTF-8
/// character boundaries.
///
/// # Returns
///
/// - `(original.to_owned(), None)` when `s.len() <= max_chars`.
/// - `(truncated_content, Some(removed_lines))` when truncation is applied.
pub fn truncate_middle(s: &str, max_chars: usize) -> (String, Option<usize>) {
    if s.len() <= max_chars {
        return (s.to_owned(), None);
    }

    let half = max_chars / 2;
    let start_end = floor_char_boundary(s, half);
    let end_start = s.len() - floor_char_boundary(s, half);

    let start = &s[..start_end];
    let end = &s[end_start..];
    let middle = &s[start_end..end_start];
    let middle_lines = middle.lines().count();

    let truncated = format!(
        "{}\n\n... [{} lignes tronquées] ...\n\n{}",
        start, middle_lines, end
    );
    (truncated, Some(middle_lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_middle_under_limit_unchanged() {
        // GIVEN
        let s = "hello world";
        // WHEN
        let (result, truncated) = truncate_middle(s, 30000);
        // THEN
        assert_eq!(result, s);
        assert!(truncated.is_none());
    }

    #[test]
    fn test_truncate_middle_exact_limit_unchanged() {
        // GIVEN
        let s = "x".repeat(30000);
        // WHEN
        let (result, truncated) = truncate_middle(&s, 30000);
        // THEN
        assert_eq!(result.len(), 30000);
        assert!(truncated.is_none());
    }

    #[test]
    fn test_truncate_middle_over_limit_preserves_boundaries() {
        // GIVEN
        let s = "A".repeat(100);
        // WHEN
        let (result, truncated) = truncate_middle(&s, 40);
        // THEN
        assert!(truncated.is_some());
        assert!(result.contains("... ["));
        assert!(result.contains("lignes tronquées] ..."));
        // The first 20 and last 20 chars are preserved
        assert!(result.starts_with(&"A".repeat(20)));
        assert!(result.ends_with(&"A".repeat(20)));
    }

    #[test]
    fn test_truncate_middle_line_count_exact() {
        // GIVEN: 10 lines of 7 chars = 70 chars, max = 40, so the middle is dropped
        let lines: Vec<String> = (0..10).map(|i| format!("line{:02}", i)).collect();
        let s = lines.join("\n");
        // WHEN
        let (result, truncated) = truncate_middle(&s, 40);
        // THEN
        let n = truncated.unwrap();
        assert!(n > 0);
        assert!(result.contains(&format!("... [{} lignes tronquées] ...", n)));
    }

    #[test]
    fn test_truncate_middle_utf8_safe() {
        // GIVEN text containing multi-byte characters (é = 2 bytes)
        let s = "ééééé".repeat(1000);
        // WHEN truncating at a limit that may fall in the middle of an é
        let (result, truncated) = truncate_middle(&s, 101);
        // THEN no panic, the result is valid UTF-8
        assert!(truncated.is_some());
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn test_truncate_middle_zero_max_chars() {
        // GIVEN arbitrary text with max_chars = 0 (degenerate case)
        let s = "hello world";
        // WHEN
        let (result, truncated) = truncate_middle(s, 0);
        // THEN truncated with a message (empty head and tail, everything is "middle")
        assert!(truncated.is_some());
        assert!(result.contains("lignes tronquées"));
    }
}
