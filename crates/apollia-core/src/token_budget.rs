use serde::{Deserialize, Serialize};

/// Token budget accumulated over the lifetime of a session or task.
///
/// Accumulated on each LLM call via [`TokenBudget::merge`].
/// Serializable for persistence in `~/.apollia/session_costs.jsonl`
/// and transport over the REST API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenBudget {
    /// Input (prompt) tokens, summed across all calls in the session.
    pub input_tokens: u64,
    /// Generated (completion) tokens, summed across all calls.
    pub output_tokens: u64,
    /// Tokens read from the Anthropic cache (roughly 90% cheaper than normal input).
    pub cache_read_tokens: u64,
    /// Tokens written to the Anthropic cache (slightly more expensive than input).
    pub cache_write_tokens: u64,
    /// Total estimated cost in USD, summed across all calls.
    pub cost_usd: f64,
    /// Cumulative API call latency in milliseconds.
    pub api_duration_ms: u64,
    /// Total wall-clock duration of the session in milliseconds.
    pub wall_duration_ms: u64,
    /// Time to First Token: latency until the first token is received (ms).
    ///
    /// `None` if the session has no streaming call, or if it was not measured.
    /// Set on the first streaming call of the session.
    pub ttft_ms: Option<u64>,
    /// Streaming duration from the first byte to the last chunk (ms).
    pub streaming_duration_ms: u64,
}

/// Counters for a single LLM call, aggregated into a [`TokenBudget`] via
/// [`TokenBudget::merge`].
///
/// `prompt_tokens` / `completion_tokens` map to the equivalent fields of
/// `TokenUsage`. `cost_usd` is the cost computed for this call. `api_ms` is the
/// measured latency of the HTTP call.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenUsageDelta {
    /// Prompt tokens consumed by the call.
    pub prompt_tokens: u32,
    /// Completion tokens produced by the call.
    pub completion_tokens: u32,
    /// Tokens read from the prompt cache.
    pub cache_read: u32,
    /// Tokens written to the prompt cache.
    pub cache_write: u32,
    /// Cost computed for this call, in USD.
    pub cost_usd: f64,
    /// Measured latency of the HTTP call, in milliseconds.
    pub api_ms: u64,
}

impl TokenBudget {
    /// Merges the counters of an LLM call into this budget.
    pub fn merge(&mut self, delta: TokenUsageDelta) {
        self.input_tokens += u64::from(delta.prompt_tokens);
        self.output_tokens += u64::from(delta.completion_tokens);
        self.cache_read_tokens += u64::from(delta.cache_read);
        self.cache_write_tokens += u64::from(delta.cache_write);
        self.cost_usd += delta.cost_usd;
        self.api_duration_ms += delta.api_ms;
    }

    /// Formats the budget for a human-readable CLI display.
    ///
    /// Format: `"Tokens: X input / Y output / Z cache-read - $0.00XX USD (TTFT: Xms, wall: Xms)"`
    /// The TTFT segment is omitted when [`ttft_ms`](TokenBudget::ttft_ms) is `None`.
    pub fn format_summary(&self) -> String {
        let ttft_segment = self
            .ttft_ms
            .map(|t| format!("TTFT: {}ms, ", t))
            .unwrap_or_default();
        format!(
            "Tokens: {} input / {} output / {} cache-read - ${:.4} USD ({}wall: {}ms)",
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
        // WHEN: two identical successive calls
        let delta = TokenUsageDelta {
            prompt_tokens: 100,
            completion_tokens: 50,
            cache_read: 80,
            cache_write: 200,
            cost_usd: 0.0012,
            api_ms: 340,
        };
        budget.merge(delta);
        budget.merge(delta);
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
