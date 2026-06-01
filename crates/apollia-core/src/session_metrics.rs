//! Aggregated session metrics.
//!
//! [`SessionMetrics`] aggregates tokens, context, tool timings, and
//! summarization events over the lifetime of a session. Computed by
//! `apollia_runtime::session_metrics::SessionMetricsActor` and broadcast via
//! [`crate::events::RuntimeEvent::SessionMetricsUpdated`].

use serde::{Deserialize, Serialize};

/// Timing of a tool call, with the delta against the static hint.
///
/// `expected_ms` comes from `tool_performance_hints.toml` and `actual_ms` is
/// measured by `ChatToolCallCompleted`. `delta_pct` = 100 * (actual - expected) / expected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolTiming {
    /// Logical name of the invoked tool.
    pub tool_name: String,
    /// Expected duration from `tool_performance_hints.toml` (ms). `None` if absent from the table.
    pub expected_ms: Option<u64>,
    /// Observed actual duration (ms).
    pub actual_ms: u64,
    /// Percentage deviation; positive means slower than expected. `None` when `expected_ms` is `None`.
    pub delta_pct: Option<f32>,
}

impl ToolTiming {
    /// Builds a `ToolTiming`, computing the delta when `expected_ms` is provided.
    pub fn new(tool_name: impl Into<String>, expected_ms: Option<u64>, actual_ms: u64) -> Self {
        let delta_pct = expected_ms.and_then(|exp| {
            if exp == 0 {
                None
            } else {
                Some((actual_ms as f32 - exp as f32) / exp as f32 * 100.0)
            }
        });
        Self {
            tool_name: tool_name.into(),
            expected_ms,
            actual_ms,
            delta_pct,
        }
    }
}

/// Context summarization / compaction event.
///
/// Emitted by the ContextManager once history has been compacted. Lets the
/// frontend show a "N messages summarized" banner plus a preview of the summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SummarizationEvent {
    /// Number of original messages replaced by the summary.
    pub messages_summarized_count: usize,
    /// Tokens saved (estimate: tokens_before - tokens_after).
    pub tokens_saved: u64,
    /// Truncated summary excerpt for display (<= 280 chars).
    pub summary_excerpt: String,
}

/// Budget alert level based on the configured thresholds.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BudgetAlertLevel {
    /// Below the warning threshold, nothing to report.
    #[default]
    Ok,
    /// Above `token_warn_pct`, show a warning toast.
    Warning,
    /// Above `token_block_pct`, blocking behavior.
    Block,
}

/// Thresholds configurable from `apollia.toml [session]`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SessionThresholds {
    /// Warning threshold as a percentage of the budget (default 80).
    pub token_warn_pct: u8,
    /// Blocking threshold as a percentage of the budget (default 100).
    pub token_block_pct: u8,
}

impl Default for SessionThresholds {
    fn default() -> Self {
        Self {
            token_warn_pct: 80,
            token_block_pct: 100,
        }
    }
}

impl SessionThresholds {
    /// Evaluates the alert level for a given usage against the total budget.
    ///
    /// `budget` is the total number of allowed tokens. Returns
    /// [`BudgetAlertLevel::Ok`] when `budget == 0` (budget not configured).
    pub fn evaluate(&self, used: u64, budget: u64) -> BudgetAlertLevel {
        if budget == 0 {
            return BudgetAlertLevel::Ok;
        }
        let pct = (used as f64) * 100.0 / (budget as f64);
        if pct >= self.token_block_pct as f64 {
            BudgetAlertLevel::Block
        } else if pct >= self.token_warn_pct as f64 {
            BudgetAlertLevel::Warning
        } else {
            BudgetAlertLevel::Ok
        }
    }
}

/// Snapshot of a session's metrics: tokens, context, timings, summarization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionMetrics {
    /// Cumulative input (prompt) tokens.
    pub tokens_in: u64,
    /// Cumulative output (generated) tokens.
    pub tokens_out: u64,
    /// Tokens read from cache (cache hits).
    pub tokens_cached: u64,
    /// Tokens consumed by meta-LLM calls (narration, summarization).
    pub tokens_meta: u64,
    /// Tokens currently occupying the context window (live estimate).
    pub context_window_used: u64,
    /// Maximum context window size of the current backend.
    pub context_window_max: u64,
    /// Total budget configured for the session (0 if not configured).
    pub token_budget: u64,
    /// Tool timing history, capped at the last 100 entries.
    pub tool_timings: Vec<ToolTiming>,
    /// Summarization event history.
    pub summarization_events: Vec<SummarizationEvent>,
}

/// Maximum number of timings kept in [`SessionMetrics::tool_timings`].
///
/// Beyond this, the oldest are evicted FIFO to bound memory usage.
pub const TOOL_TIMINGS_MAX: usize = 100;

impl SessionMetrics {
    /// Merges the counters of an LLM call into `tokens_in`, `tokens_out`, `tokens_cached`.
    ///
    /// `is_meta = true` routes tokens to `tokens_meta` instead of the main counters.
    pub fn record_llm_call(
        &mut self,
        prompt_tokens: u32,
        completion_tokens: u32,
        cached_tokens: u32,
        is_meta: bool,
    ) {
        let total = u64::from(prompt_tokens) + u64::from(completion_tokens);
        if is_meta {
            self.tokens_meta = self.tokens_meta.saturating_add(total);
        } else {
            self.tokens_in = self.tokens_in.saturating_add(u64::from(prompt_tokens));
            self.tokens_out = self.tokens_out.saturating_add(u64::from(completion_tokens));
            self.tokens_cached = self.tokens_cached.saturating_add(u64::from(cached_tokens));
        }
        // Quick estimate: context_window_used is approximately non-meta
        // tokens_in + tokens_out. The ContextManager can correct it via
        // `set_context_window_used`.
        self.context_window_used = self.tokens_in.saturating_add(self.tokens_out);
    }

    /// Updates `context_window_used` from an authoritative measurement.
    pub fn set_context_window_used(&mut self, used: u64) {
        self.context_window_used = used;
    }

    /// Adds a tool timing while respecting the [`TOOL_TIMINGS_MAX`] bound.
    pub fn push_tool_timing(&mut self, timing: ToolTiming) {
        if self.tool_timings.len() >= TOOL_TIMINGS_MAX {
            self.tool_timings.remove(0);
        }
        self.tool_timings.push(timing);
    }

    /// Records a summarization event.
    pub fn push_summarization(&mut self, event: SummarizationEvent) {
        self.summarization_events.push(event);
    }

    /// Sum of tokens counted toward the budget (in + out + meta, cache excluded).
    pub fn tokens_used_for_budget(&self) -> u64 {
        self.tokens_in
            .saturating_add(self.tokens_out)
            .saturating_add(self.tokens_meta)
    }

    /// Context window occupancy percentage (0.0 to 100.0).
    ///
    /// Returns `0.0` when `context_window_max == 0` to avoid dividing by zero.
    pub fn context_window_pct(&self) -> f32 {
        if self.context_window_max == 0 {
            return 0.0;
        }
        (self.context_window_used as f32 / self.context_window_max as f32) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_timing_delta_computation() {
        // GIVEN a tool slower than expected
        let t = ToolTiming::new("file_read", Some(100), 150);
        // THEN delta = +50%
        assert_eq!(t.expected_ms, Some(100));
        assert_eq!(t.actual_ms, 150);
        assert!((t.delta_pct.unwrap() - 50.0).abs() < 0.01);
    }

    #[test]
    fn tool_timing_delta_absent_when_no_expected() {
        // GIVEN no hint
        let t = ToolTiming::new("custom_tool", None, 42);
        // THEN delta_pct is None
        assert!(t.delta_pct.is_none());
    }

    #[test]
    fn tool_timing_delta_ignores_zero_expected() {
        // GIVEN a hint of 0 ms (invalid but possible)
        let t = ToolTiming::new("tool", Some(0), 50);
        // THEN delta_pct is None to avoid dividing by zero
        assert!(t.delta_pct.is_none());
    }

    #[test]
    fn thresholds_default_is_80_100() {
        let th = SessionThresholds::default();
        assert_eq!(th.token_warn_pct, 80);
        assert_eq!(th.token_block_pct, 100);
    }

    #[test]
    fn thresholds_evaluate_returns_ok_without_budget() {
        // GIVEN no budget configured
        let th = SessionThresholds::default();
        // THEN evaluation returns Ok even with high usage
        assert_eq!(th.evaluate(10_000, 0), BudgetAlertLevel::Ok);
    }

    #[test]
    fn thresholds_evaluate_crosses_warning_and_block() {
        let th = SessionThresholds::default();
        // Below warning: Ok
        assert_eq!(th.evaluate(500, 1000), BudgetAlertLevel::Ok);
        // At 80%: Warning
        assert_eq!(th.evaluate(800, 1000), BudgetAlertLevel::Warning);
        // At 95%: Warning
        assert_eq!(th.evaluate(950, 1000), BudgetAlertLevel::Warning);
        // At 100%: Block
        assert_eq!(th.evaluate(1000, 1000), BudgetAlertLevel::Block);
        // Beyond: Block
        assert_eq!(th.evaluate(1200, 1000), BudgetAlertLevel::Block);
    }

    #[test]
    fn session_metrics_aggregates_llm_calls() {
        // GIVEN two successive non-meta LLM calls
        let mut m = SessionMetrics::default();
        m.record_llm_call(100, 50, 30, false);
        m.record_llm_call(200, 80, 60, false);
        // THEN main counters are accumulated
        assert_eq!(m.tokens_in, 300);
        assert_eq!(m.tokens_out, 130);
        assert_eq!(m.tokens_cached, 90);
        assert_eq!(m.tokens_meta, 0);
        // AND context_window_used = tokens_in + tokens_out
        assert_eq!(m.context_window_used, 430);
    }

    #[test]
    fn session_metrics_routes_meta_tokens_separately() {
        // GIVEN a meta LLM call (narration)
        let mut m = SessionMetrics::default();
        m.record_llm_call(50, 30, 0, true);
        // THEN only the meta counters are incremented
        assert_eq!(m.tokens_in, 0);
        assert_eq!(m.tokens_out, 0);
        assert_eq!(m.tokens_meta, 80);
    }

    #[test]
    fn session_metrics_tool_timings_capped() {
        // GIVEN more than TOOL_TIMINGS_MAX timings pushed
        let mut m = SessionMetrics::default();
        for i in 0..(TOOL_TIMINGS_MAX + 5) {
            m.push_tool_timing(ToolTiming::new(format!("tool_{i}"), Some(10), 12));
        }
        // THEN the list is bounded and FIFO
        assert_eq!(m.tool_timings.len(), TOOL_TIMINGS_MAX);
        assert_eq!(m.tool_timings[0].tool_name, "tool_5");
        assert_eq!(
            m.tool_timings.last().unwrap().tool_name,
            format!("tool_{}", TOOL_TIMINGS_MAX + 4)
        );
    }

    #[test]
    fn session_metrics_context_window_pct() {
        // GIVEN a context at 70%
        let m = SessionMetrics {
            context_window_used: 7000,
            context_window_max: 10_000,
            ..Default::default()
        };
        // THEN pct is approximately 70.0
        assert!((m.context_window_pct() - 70.0).abs() < 0.01);
    }

    #[test]
    fn session_metrics_context_window_pct_safe_when_max_zero() {
        // GIVEN a context with no max configured
        let m = SessionMetrics::default();
        // THEN pct = 0 (no panic on division by zero)
        assert_eq!(m.context_window_pct(), 0.0);
    }

    #[test]
    fn session_metrics_tokens_used_for_budget_sums_all_non_cached() {
        // GIVEN a mix of in/out/meta/cached
        let m = SessionMetrics {
            tokens_in: 100,
            tokens_out: 50,
            tokens_cached: 200,
            tokens_meta: 30,
            ..Default::default()
        };
        // THEN the budget counts in + out + meta, never cached
        assert_eq!(m.tokens_used_for_budget(), 180);
    }

    #[test]
    fn session_metrics_serde_roundtrip() {
        // GIVEN metrics with events
        let mut m = SessionMetrics {
            tokens_in: 500,
            tokens_out: 200,
            tokens_cached: 100,
            tokens_meta: 50,
            context_window_used: 700,
            context_window_max: 200_000,
            token_budget: 10_000,
            ..Default::default()
        };
        m.push_tool_timing(ToolTiming::new("file_read", Some(50), 75));
        m.push_summarization(SummarizationEvent {
            messages_summarized_count: 10,
            tokens_saved: 2500,
            summary_excerpt: "Résumé de 10 messages…".into(),
        });
        // WHEN serialize / deserialize
        let json = serde_json::to_string(&m).expect("serialize");
        let back: SessionMetrics = serde_json::from_str(&json).expect("deserialize");
        // THEN fields are preserved
        assert_eq!(back.tokens_in, 500);
        assert_eq!(back.tool_timings.len(), 1);
        assert_eq!(back.summarization_events.len(), 1);
        assert_eq!(back.summarization_events[0].tokens_saved, 2500);
    }

    #[test]
    fn alert_level_default_is_ok() {
        assert_eq!(BudgetAlertLevel::default(), BudgetAlertLevel::Ok);
    }
}
