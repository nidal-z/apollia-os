//! Dependency-free subsequence fuzzy matching for the REPL command palette.

/// Score `query` against `candidate` by greedy left-to-right subsequence match.
///
/// Returns `None` when `query` is not a subsequence of `candidate`. Otherwise a
/// penalty (sum of gaps between matched characters); lower is a tighter match.
pub fn score(query: &str, candidate: &str) -> Option<i32> {
    let q: Vec<char> = query.to_ascii_lowercase().chars().collect();
    if q.is_empty() {
        return Some(0);
    }
    let c: Vec<char> = candidate.to_ascii_lowercase().chars().collect();
    let mut qi = 0usize;
    let mut penalty = 0i32;
    let mut last: i32 = -1;
    for (i, &cc) in c.iter().enumerate() {
        if qi < q.len() && cc == q[qi] {
            if last >= 0 {
                penalty += i as i32 - last - 1;
            }
            last = i as i32;
            qi += 1;
        }
    }
    if qi == q.len() {
        Some(penalty)
    } else {
        None
    }
}

/// Rank `candidates` matching `query`, best first. An empty query keeps order.
pub fn rank<'a>(query: &str, candidates: &[&'a str]) -> Vec<&'a str> {
    if query.trim().is_empty() {
        return candidates.to_vec();
    }
    let mut scored: Vec<(&str, i32)> = candidates
        .iter()
        .filter_map(|c| score(query, c).map(|s| (*c, s)))
        .collect();
    scored.sort_by_key(|(c, s)| (*s, c.len()));
    scored.into_iter().map(|(c, _)| c).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_subsequence_and_miss() {
        // GIVEN a subsequence of a command, a string that is not one, and an empty input
        // WHEN each is scored against that command
        // THEN the subsequence scores, the miss does not, and the empty input scores zero rather than nothing
        assert!(score("al", "agent list").is_some());
        assert!(score("xyz", "agent list").is_none());
        assert_eq!(score("", "anything"), Some(0));
    }

    // GIVEN a query WHEN ranked THEN the best matches come first.
    #[test]
    fn test_rank_orders_best_first() {
        let cands = ["audit list", "audit verify", "agent list", "task list"];
        let ranked = rank("aud", &cands);
        assert_eq!(ranked[0], "audit list");
        assert!(ranked.contains(&"audit verify"));
        assert!(!ranked.contains(&"task list")); // 'aud' is not a subsequence
    }
}
