//! Deterministic "did you mean" suggestion by edit distance.
//!
//! Used when an unknown top-level command is typed: suggest the nearest valid
//! command name. No LLM, offline, instant (the LLM path is `apollia-os do`).

/// Levenshtein edit distance between `a` and `b`.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Nearest candidate to `input` within `max_distance`, or `None`.
///
/// Ties resolve to the first candidate in `candidates` order (deterministic).
pub fn nearest<'a>(input: &str, candidates: &[&'a str], max_distance: usize) -> Option<&'a str> {
    let mut best: Option<(&str, usize)> = None;
    for &c in candidates {
        let d = levenshtein(input, c);
        if d <= max_distance && best.map(|(_, bd)| d < bd).unwrap_or(true) {
            best = Some((c, d));
        }
    }
    best.map(|(c, _)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_basic() {
        // GIVEN a word compared to itself, a transposition, and a truncation
        // WHEN the edit distance is computed
        // THEN identical words are at zero and the two typos are within the suggestion threshold
        assert_eq!(levenshtein("agent", "agent"), 0);
        assert_eq!(levenshtein("agnet", "agent"), 2);
        assert_eq!(levenshtein("stat", "status"), 2);
    }

    // GIVEN a near typo WHEN searching THEN the right candidate.
    #[test]
    fn test_nearest_suggests_close_match() {
        let cands = ["agent", "audit", "auth", "task"];
        assert_eq!(nearest("agnet", &cands, 2), Some("agent"));
        assert_eq!(nearest("aduit", &cands, 2), Some("audit"));
    }

    // GIVEN an input too far away WHEN searching THEN None.
    #[test]
    fn test_nearest_none_when_far() {
        let cands = ["agent", "audit"];
        assert_eq!(nearest("xylophone", &cands, 2), None);
    }
}
