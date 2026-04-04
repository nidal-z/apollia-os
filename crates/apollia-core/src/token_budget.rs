use serde::{Deserialize, Serialize};

/// Budget de tokens accumulé sur la durée d'une session ou d'une tâche.
///
/// Accumulé à chaque appel LLM via [`TokenBudget::merge`].
/// Sérialisable pour la persistance dans `~/.apollia/session_costs.jsonl`
/// et le transport via l'API REST.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenBudget {
    /// Tokens en entrée (prompt), cumulés sur tous les appels de la session.
    pub input_tokens: u64,
    /// Tokens générés (completion), cumulés sur tous les appels.
    pub output_tokens: u64,
    /// Tokens lus depuis le cache Anthropic (coût ~90% inférieur à l'input normal).
    pub cache_read_tokens: u64,
    /// Tokens écrits dans le cache Anthropic (coût légèrement supérieur à l'input).
    pub cache_write_tokens: u64,
    /// Coût total estimé en USD, cumulé sur tous les appels.
    pub cost_usd: f64,
    /// Latence cumulée des appels API en millisecondes.
    pub api_duration_ms: u64,
    /// Durée wall-clock totale de la session en millisecondes.
    pub wall_duration_ms: u64,
    /// Time to First Token — latence jusqu'au premier token reçu (ms).
    ///
    /// `None` si la session ne contient aucun appel streaming, ou si non mesuré.
    /// Défini sur le premier appel streaming de la session.
    pub ttft_ms: Option<u64>,
    /// Durée du streaming depuis le premier byte jusqu'au dernier chunk (ms).
    pub streaming_duration_ms: u64,
}

impl TokenBudget {
    /// Fusionne les compteurs d'un appel LLM dans ce budget.
    ///
    /// `prompt_tokens` / `completion_tokens` correspondent aux champs équivalents
    /// de `TokenUsage`. `cost_usd` est le coût calculé pour cet appel. `api_ms`
    /// est la latence mesurée de l'appel HTTP.
    pub fn merge(
        &mut self,
        prompt_tokens: u32,
        completion_tokens: u32,
        cache_read: u32,
        cache_write: u32,
        cost_usd: f64,
        api_ms: u64,
    ) {
        self.input_tokens += u64::from(prompt_tokens);
        self.output_tokens += u64::from(completion_tokens);
        self.cache_read_tokens += u64::from(cache_read);
        self.cache_write_tokens += u64::from(cache_write);
        self.cost_usd += cost_usd;
        self.api_duration_ms += api_ms;
    }

    /// Formate le budget pour un affichage CLI lisible par un humain.
    ///
    /// Format : `"Tokens: X input / Y output / Z cache-read — $0.00XX USD (TTFT: Xms, wall: Xms)"`
    /// Le segment TTFT est omis si [`ttft_ms`](TokenBudget::ttft_ms) est `None`.
    pub fn format_summary(&self) -> String {
        let ttft_segment = self
            .ttft_ms
            .map(|t| format!("TTFT: {}ms, ", t))
            .unwrap_or_default();
        format!(
            "Tokens: {} input / {} output / {} cache-read — ${:.4} USD ({}wall: {}ms)",
            self.input_tokens,
            self.output_tokens,
            self.cache_read_tokens,
            self.cost_usd,
            ttft_segment,
            self.wall_duration_ms
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_budget_merge_accumulates() {
        // GIVEN
        let mut budget = TokenBudget::default();
        // WHEN — deux appels successifs identiques
        budget.merge(100, 50, 80, 200, 0.0012, 340);
        budget.merge(100, 50, 80, 200, 0.0012, 340);
        // THEN
        assert_eq!(budget.input_tokens, 200);
        assert_eq!(budget.output_tokens, 100);
        assert_eq!(budget.cache_read_tokens, 160);
        assert_eq!(budget.cache_write_tokens, 400);
        assert_eq!(budget.api_duration_ms, 680);
        assert!((budget.cost_usd - 0.0024).abs() < 1e-6);
    }

    #[test]
    fn test_format_summary_with_ttft() {
        // GIVEN
        let budget = TokenBudget {
            input_tokens: 1234,
            output_tokens: 567,
            cache_read_tokens: 890,
            cost_usd: 0.0089,
            ttft_ms: Some(234),
            wall_duration_ms: 4200,
            ..Default::default()
        };
        // WHEN
        let s = budget.format_summary();
        // THEN
        assert!(s.contains("1234 input"), "missing input count: {s}");
        assert!(s.contains("567 output"), "missing output count: {s}");
        assert!(s.contains("890 cache-read"), "missing cache-read: {s}");
        assert!(s.contains("TTFT: 234ms"), "missing TTFT: {s}");
        assert!(s.contains("$0.0089"), "missing cost: {s}");
        assert!(s.contains("wall: 4200ms"), "missing wall time: {s}");
    }

    #[test]
    fn test_format_summary_without_ttft() {
        // GIVEN
        let budget = TokenBudget {
            wall_duration_ms: 1000,
            ..Default::default()
        };
        // WHEN
        let s = budget.format_summary();
        // THEN
        assert!(!s.contains("TTFT"), "TTFT should be absent: {s}");
        assert!(s.contains("wall: 1000ms"), "missing wall time: {s}");
    }

    #[test]
    fn test_token_budget_serde_roundtrip() {
        // GIVEN
        let budget = TokenBudget {
            input_tokens: 500,
            output_tokens: 200,
            cache_read_tokens: 100,
            cache_write_tokens: 50,
            cost_usd: 0.005,
            api_duration_ms: 800,
            wall_duration_ms: 900,
            ttft_ms: Some(150),
            streaming_duration_ms: 750,
        };
        // WHEN
        let json = serde_json::to_string(&budget).expect("serialize");
        let restored: TokenBudget = serde_json::from_str(&json).expect("deserialize");
        // THEN
        assert_eq!(restored.input_tokens, 500);
        assert_eq!(restored.ttft_ms, Some(150));
        assert!((restored.cost_usd - 0.005).abs() < 1e-9);
    }
}
