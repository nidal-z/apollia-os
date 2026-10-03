//! The generation statistics of one conversation turn.
//!
//! A turn makes one model call per step. Its answer keeps the total, under the
//! `generation` key of the message metadata, so a reloaded conversation shows
//! it without reading the calls back; the detail of each call is read from the
//! LLM call store, by turn.
//!
//! An unknown value stays absent: a turn whose calls measured no first token
//! has no time to first token, and one with no generation time has no speed.
//! Absent is "not measured", never zero.

use serde::Serialize;

use apollia_llm::{CallEnd, CallMeasure};

/// Why a turn ended, as its answer says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnStopReason {
    /// The model ended its answer.
    Completed,
    /// The last call hit the token limit.
    Length,
    /// The user stopped the turn.
    UserStop,
    /// The last call failed.
    Error,
}

/// The total of a turn, written under its answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TurnGeneration {
    /// Number of model calls the turn made.
    pub calls: u32,
    /// Tokens generated across the calls.
    pub output_tokens: u64,
    /// Generated tokens per second of generation time, over the calls whose
    /// generation time is known. Absent when none is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_per_second: Option<f64>,
    /// Time to first token of the first call. Absent when not measured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttft_ms: Option<u64>,
    /// From the user's message to the end of the turn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Why the turn ended, from its last call.
    pub stop_reason: TurnStopReason,
}

/// The total of the calls of a turn; `None` when the turn made no call.
///
/// `paused` says the user stopped the turn, whatever its last call reported:
/// a Stop between two steps leaves a last call that ended normally.
#[must_use]
pub fn aggregate(
    calls: &[CallMeasure],
    duration_ms: Option<u64>,
    paused: bool,
) -> Option<TurnGeneration> {
    let last = calls.last()?;
    let output_tokens = calls
        .iter()
        .map(|c| u64::from(c.usage.completion_tokens))
        .sum();
    let (timed_tokens, timed_ms) = calls
        .iter()
        .filter_map(|c| {
            c.generation_ms
                .map(|ms| (u64::from(c.usage.completion_tokens), ms))
        })
        .fold((0_u64, 0_u64), |(tokens, ms), (t, m)| {
            (tokens + t, ms.saturating_add(m))
        });
    let tokens_per_second = (timed_ms > 0).then(|| timed_tokens as f64 * 1000.0 / timed_ms as f64);
    let stop_reason = if paused {
        TurnStopReason::UserStop
    } else {
        match last.end {
            CallEnd::Stop | CallEnd::ToolCalls => TurnStopReason::Completed,
            CallEnd::Length => TurnStopReason::Length,
            CallEnd::Cancelled => TurnStopReason::UserStop,
            CallEnd::Error => TurnStopReason::Error,
        }
    };
    Some(TurnGeneration {
        calls: u32::try_from(calls.len()).unwrap_or(u32::MAX),
        output_tokens,
        tokens_per_second,
        ttft_ms: calls.first().and_then(|c| c.ttft_ms),
        duration_ms,
        stop_reason,
    })
}

/// Milliseconds from `started_at`, an RFC 3339 instant, to now; `None` when it
/// does not parse or lies in the future.
#[must_use]
pub fn elapsed_since(started_at: &str) -> Option<u64> {
    let started = chrono::DateTime::parse_from_rfc3339(started_at).ok()?;
    let elapsed = chrono::Utc::now().signed_duration_since(started);
    u64::try_from(elapsed.num_milliseconds()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    use apollia_llm::TokenUsage;

    fn call(tokens: u32, ttft: Option<u64>, generation: Option<u64>, end: CallEnd) -> CallMeasure {
        CallMeasure {
            usage: TokenUsage {
                prompt_tokens: 500,
                completion_tokens: tokens,
                ..Default::default()
            },
            latency_ms: 2_000,
            ttft_ms: ttft,
            generation_ms: generation,
            end,
        }
    }

    #[test]
    fn test_a_three_call_turn_adds_up_to_its_total() {
        // GIVEN a turn of three calls: two tool steps, then the answer
        let calls = vec![
            call(40, Some(320), Some(800), CallEnd::ToolCalls),
            call(20, Some(150), Some(400), CallEnd::ToolCalls),
            call(60, Some(180), Some(1_200), CallEnd::Stop),
        ];

        // WHEN the turn ends after 4.5 seconds
        let total = aggregate(&calls, Some(4_500), false).expect("three calls");

        // THEN the total sums the tokens, divides them by the generation time,
        // keeps the first call's TTFT and ends on a completed answer
        assert_eq!(
            total,
            TurnGeneration {
                calls: 3,
                output_tokens: 120,
                tokens_per_second: Some(50.0),
                ttft_ms: Some(320),
                duration_ms: Some(4_500),
                stop_reason: TurnStopReason::Completed,
            }
        );
    }

    #[test]
    fn test_an_unmeasured_first_token_and_speed_stay_absent() {
        // GIVEN a turn whose only call measured neither a first token nor a
        // generation time
        let calls = vec![call(30, None, None, CallEnd::Length)];

        // WHEN it is totalled and written
        let total = aggregate(&calls, None, false).expect("one call");
        let written = serde_json::to_value(&total).expect("serializes");

        // THEN neither is zero: both are absent, and the stop is the length limit
        assert_eq!(total.ttft_ms, None);
        assert_eq!(total.tokens_per_second, None);
        assert!(written.get("ttft_ms").is_none());
        assert!(written.get("tokens_per_second").is_none());
        assert_eq!(written["stop_reason"], "length");
    }

    #[test]
    fn test_a_paused_turn_ends_on_a_user_stop() {
        // GIVEN a turn the user stopped between two steps
        let calls = vec![call(10, Some(100), Some(200), CallEnd::ToolCalls)];

        // WHEN it is totalled as paused
        let total = aggregate(&calls, Some(900), true).expect("one call");

        // THEN its stop reason is the user's
        assert_eq!(total.stop_reason, TurnStopReason::UserStop);
    }

    #[test]
    fn test_a_turn_without_a_call_has_no_total() {
        // GIVEN a turn stopped before its first call
        // WHEN it is totalled
        // THEN there is nothing to write
        assert!(aggregate(&[], Some(10), true).is_none());
    }
}
