//! Utility functions shared across the Apollia runtime.

/// Returns the largest UTF-8 character boundary that is `<= index`.
///
/// Stable equivalent of the nightly `str::floor_char_boundary` method.
/// Guarantees the returned value is a valid position to slice `s`, so callers
/// that cut untrusted text at a computed byte offset can clamp the offset with
/// this helper instead of risking a mid-code-point panic.
pub fn floor_char_boundary(s: &str, index: usize) -> usize {
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
    // Clamp the tail start to a real char boundary. Computing `len - floor(half)`
    // subtracts a floored value from the length, which lands at an arbitrary byte
    // that is not guaranteed to be a boundary (it panics mid-code-point on
    // multi-byte chars like '─'). Flooring `len - half` instead always yields a
    // valid slice position, and `len - half >= half >= start_end` here (the
    // function only runs when `len > max_chars = 2 * half`), so the middle slice
    // stays well-ordered.
    let end_start = floor_char_boundary(s, s.len() - half);

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

/// Canonicalize a timestamp string to RFC3339 UTC with a trailing `Z`.
///
/// SQLite `CURRENT_TIMESTAMP` columns store naive UTC as
/// `"YYYY-MM-DD HH:MM:SS"` (space separator, no timezone marker). A browser
/// parsing such a string reads it as local time, which skews every relative
/// time display by the local UTC offset. Normalizing at the read boundary
/// gives every view a single canonical wire format.
///
/// Behavior:
/// - Timezone-aware input (RFC3339 with `Z` or a numeric offset) is converted
///   to UTC and re-emitted with a `Z` suffix.
/// - SQLite naive input is treated as UTC and emitted with a `Z` suffix.
/// - Empty or unparseable input is returned unchanged.
pub fn sqlite_to_rfc3339(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return raw.to_string();
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return dt
            .with_timezone(&chrono::Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    }
    for fmt in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(trimmed, fmt) {
            return chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(naive, chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        }
    }
    raw.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sqlite_naive_treated_as_utc() {
        // GIVEN a SQLite CURRENT_TIMESTAMP value (naive UTC, no timezone marker)
        let raw = "2026-06-11 14:30:00";
        // WHEN canonicalizing it
        let out = sqlite_to_rfc3339(raw);
        // THEN it gains a Z suffix and keeps the same wall-clock UTC value
        assert_eq!(out, "2026-06-11T14:30:00Z");
    }

    #[test]
    fn test_rfc3339_with_offset_normalized_to_utc() {
        // GIVEN a timezone-aware RFC3339 timestamp
        let raw = "2026-06-11T16:30:00+02:00";
        // WHEN canonicalizing it
        let out = sqlite_to_rfc3339(raw);
        // THEN it is converted to UTC with a Z suffix
        assert_eq!(out, "2026-06-11T14:30:00Z");
    }

    #[test]
    fn test_empty_and_unparseable_pass_through() {
        // GIVEN empty and garbage inputs
        // WHEN canonicalizing them
        // THEN they are returned unchanged
        assert_eq!(sqlite_to_rfc3339(""), "");
        assert_eq!(sqlite_to_rfc3339("not-a-date"), "not-a-date");
    }

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
    fn test_truncate_middle_multibyte_at_tail_boundary() {
        // GIVEN an ASCII head then 3-byte box-drawing chars, sized so the tail
        // start (`len - half`) lands INSIDE a '─' (U+2500, 3 bytes). Regression:
        // the old `len - floor(half)` produced a non-boundary offset and panicked
        // mid-code-point. This is the shape that crashed on a long tool output.
        let s = format!("{}{}", "a".repeat(50), "\u{2500}".repeat(50));
        // WHEN truncating (half = 50; the tail would start at byte 150, inside a char)
        let (result, truncated) = truncate_middle(&s, 100);
        // THEN no panic and the result is valid UTF-8
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
