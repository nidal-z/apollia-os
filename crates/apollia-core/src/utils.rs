//! Utility functions shared across the Apollia runtime.

/// Retourne la plus grande frontière de caractère UTF-8 ≤ `index`.
///
/// Équivalent stable de la méthode nightly `str::floor_char_boundary`.
/// Garantit que la valeur retournée est une position valide pour découper `s`.
fn floor_char_boundary(s: &str, index: usize) -> usize {
    let mut i = index.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Tronque un texte en préservant le début et la fin, en supprimant le milieu.
///
/// Évite la perte d'information critique : le début contient le contexte et les
/// déclarations, la fin contient les résultats récents. Le milieu supprimé est
/// remplacé par un message indiquant le nombre de lignes perdues.
///
/// `max_chars` est exprimé en bytes UTF-8 — proxy raisonnable pour les outputs
/// d'outils qui sont du texte ASCII ou UTF-8 basique. Les découpages respectent
/// toujours les frontières de caractères UTF-8 valides.
///
/// # Retourne
///
/// - `(original.to_owned(), None)` si `s.len() <= max_chars`.
/// - `(contenu_tronqué, Some(lignes_supprimées))` si la troncation est appliquée.
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
        // Les 20 premiers et 20 derniers chars sont préservés
        assert!(result.starts_with(&"A".repeat(20)));
        assert!(result.ends_with(&"A".repeat(20)));
    }

    #[test]
    fn test_truncate_middle_line_count_exact() {
        // GIVEN : 10 lignes de 7 chars = 70 chars, max = 40 → middle supprimé
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
        // GIVEN un texte contenant des caractères multi-octets (é = 2 octets)
        let s = "ééééé".repeat(1000);
        // WHEN troncation à une limite qui tombe potentiellement au milieu d'un é
        let (result, truncated) = truncate_middle(&s, 101);
        // THEN pas de panic, le résultat est du UTF-8 valide
        assert!(truncated.is_some());
        assert!(std::str::from_utf8(result.as_bytes()).is_ok());
    }

    #[test]
    fn test_truncate_middle_zero_max_chars() {
        // GIVEN un texte quelconque avec max_chars = 0 (cas dégénéré)
        let s = "hello world";
        // WHEN
        let (result, truncated) = truncate_middle(s, 0);
        // THEN tronqué avec message (début et fin vides, tout est "milieu")
        assert!(truncated.is_some());
        assert!(result.contains("lignes tronquées"));
    }
}
