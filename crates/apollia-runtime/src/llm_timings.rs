//! Engine-reported per-completion timings from the embedded `llama-server`.
//!
//! `llama-server` attaches a `timings` object to every chat completion response,
//! carrying what it measured itself: how many prompt tokens it evaluated, how
//! many it served from the KV cache, and how long prefill and generation each
//! took. Capturing it makes prefill and decode separable without estimating
//! anything client-side, which no wall-clock measurement can do.
//!
//! Field names here follow the measurement dictionary in
//! `docs/agents/OBSERVABILITY.md`, which is the naming authority for the
//! tracing fields emitted by [`emit_completion_timings`]. The engine's own key
//! names are deliberately not reused: `prompt_n` counts only the tokens the
//! engine actually evaluated, excluding cached ones, and a field called
//! "prompt tokens" that silently excludes most of the prompt is the single
//! ambiguity most likely to corrupt a measurement.
//!
//! Degradation is total: a response without timings, or with a malformed
//! timings object, yields no event and never disturbs the completion. A build
//! that stops reporting them costs observability, never availability.

use serde::Deserialize;

/// Failure to interpret a `timings` object that was present in a response.
///
/// Absence is not an error: it is `Ok(None)` from [`parse_timings`].
#[derive(Debug, thiserror::Error)]
pub enum TimingsError {
    /// The `timings` value was not a JSON object.
    #[error("timings is not an object")]
    NotAnObject,

    /// A required field was absent or not of the expected type.
    #[error("timings field {field} is missing or not a number")]
    BadField {
        /// Engine-side key that could not be read.
        field: &'static str,
    },
}

/// One completion's engine-reported timings.
///
/// Durations are milliseconds as the engine reports them. Counts are tokens.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(try_from = "RawTimings")]
pub struct EngineTimings {
    /// Prompt tokens the engine actually evaluated (`prompt_n`).
    ///
    /// Excludes anything served from cache, so on a warm request this can be 1
    /// while the submitted prompt is thousands of tokens.
    pub prompt_tok_computed: u32,
    /// Prompt tokens served from the KV cache without recomputation (`cache_n`).
    pub prompt_tok_cached: u32,
    /// Prefill duration for [`prompt_tok_computed`](Self::prompt_tok_computed)
    /// only (`prompt_ms`).
    pub prefill_ms: f64,
    /// Tokens generated, reasoning and content together (`predicted_n`).
    pub decode_tok: u32,
    /// Generation duration (`predicted_ms`).
    pub decode_ms: f64,
    /// The engine's own prefill rate, kept as a cross-check against
    /// [`prefill_tps`](Self::prefill_tps) rather than replacing it.
    pub engine_prefill_tps: Option<f64>,
    /// The engine's own decode rate, kept as a cross-check against
    /// [`decode_tps`](Self::decode_tps).
    pub engine_decode_tps: Option<f64>,
}

impl EngineTimings {
    /// Every prompt token submitted, cached or recomputed.
    ///
    /// The prompt size a user would mean, which is neither of the two counts the
    /// engine reports on its own.
    pub fn prompt_tok_total(&self) -> u32 {
        self.prompt_tok_computed
            .saturating_add(self.prompt_tok_cached)
    }

    /// Share of the submitted prompt that avoided recomputation.
    ///
    /// `None` for an empty prompt, where the ratio is undefined rather than zero.
    pub fn prompt_cache_hit_ratio(&self) -> Option<f64> {
        let total = self.prompt_tok_total();
        (total > 0).then(|| f64::from(self.prompt_tok_cached) / f64::from(total))
    }

    /// Rate at which the engine consumed new prompt tokens.
    ///
    /// `None` when nothing was evaluated, which is a fully cached prompt: there
    /// is no prefill to rate, and reporting zero would read as a stall. Note
    /// that on a nearly-cached request this figure is dominated by fixed
    /// per-request cost and is not comparable to a cold one.
    pub fn prefill_tps(&self) -> Option<f64> {
        (self.prompt_tok_computed > 0 && self.prefill_ms > 0.0)
            .then(|| f64::from(self.prompt_tok_computed) / (self.prefill_ms / 1000.0))
    }

    /// Generation rate, excluding prefill.
    ///
    /// Computed over the engine's generation duration, never over wall-clock.
    pub fn decode_tps(&self) -> Option<f64> {
        (self.decode_tok > 0 && self.decode_ms > 0.0)
            .then(|| f64::from(self.decode_tok) / (self.decode_ms / 1000.0))
    }
}

/// Wire shape of the engine's `timings` object.
///
/// Only the fields Apollia consumes are named. The engine also reports
/// per-token millisecond figures and, under speculative decoding, draft
/// counters; ignoring them here keeps the typed surface to what is measured.
#[derive(Deserialize)]
struct RawTimings {
    prompt_n: Option<i64>,
    cache_n: Option<i64>,
    prompt_ms: Option<f64>,
    predicted_n: Option<i64>,
    predicted_ms: Option<f64>,
    prompt_per_second: Option<f64>,
    predicted_per_second: Option<f64>,
}

impl TryFrom<RawTimings> for EngineTimings {
    type Error = TimingsError;

    fn try_from(raw: RawTimings) -> Result<Self, Self::Error> {
        fn count(v: Option<i64>, field: &'static str) -> Result<u32, TimingsError> {
            // A negative count is the engine signalling "not applicable" rather
            // than a measurement, so it is rejected rather than clamped.
            v.filter(|n| *n >= 0)
                .and_then(|n| u32::try_from(n).ok())
                .ok_or(TimingsError::BadField { field })
        }
        fn ms(v: Option<f64>, field: &'static str) -> Result<f64, TimingsError> {
            v.filter(|d| d.is_finite() && *d >= 0.0)
                .ok_or(TimingsError::BadField { field })
        }

        Ok(Self {
            prompt_tok_computed: count(raw.prompt_n, "prompt_n")?,
            // Absent on a build that predates prompt-cache reporting. Treated as
            // zero cached rather than as a parse failure, so an older engine
            // still yields usable prefill and decode figures.
            prompt_tok_cached: count(raw.cache_n.or(Some(0)), "cache_n")?,
            prefill_ms: ms(raw.prompt_ms, "prompt_ms")?,
            decode_tok: count(raw.predicted_n, "predicted_n")?,
            decode_ms: ms(raw.predicted_ms, "predicted_ms")?,
            engine_prefill_tps: raw.prompt_per_second.filter(|v| v.is_finite()),
            engine_decode_tps: raw.predicted_per_second.filter(|v| v.is_finite()),
        })
    }
}

/// Extract the engine timings from a chat completion response body.
///
/// Returns `Ok(None)` when the response carries no `timings` key, which is the
/// normal case for any backend that is not `llama-server`. Returns an error only
/// when a `timings` object is present and cannot be interpreted.
pub fn parse_timings(body: &serde_json::Value) -> Result<Option<EngineTimings>, TimingsError> {
    let Some(raw) = body.get("timings") else {
        return Ok(None);
    };
    if raw.is_null() {
        return Ok(None);
    }
    parse_timings_object(raw).map(Some)
}

/// Interpret a `timings` object that has already been lifted out of a response.
///
/// The transport layer hands the object over on its own, so this is the entry
/// point the completion paths use.
pub fn parse_timings_object(raw: &serde_json::Value) -> Result<EngineTimings, TimingsError> {
    if !raw.is_object() {
        return Err(TimingsError::NotAnObject);
    }
    let parsed: RawTimings =
        serde_json::from_value(raw.clone()).map_err(|_| TimingsError::NotAnObject)?;
    EngineTimings::try_from(parsed)
}

/// Parse a lifted `timings` object and emit, absorbing every failure.
///
/// Never returns anything to act on: no timing problem is worth failing a
/// completion over.
pub fn observe_timings(backend: &str, model: &str, raw: &serde_json::Value) {
    match parse_timings_object(raw) {
        Ok(timings) => emit_completion_timings(backend, model, &timings),
        Err(e) => {
            tracing::debug!(
                backend = %backend,
                model = %model,
                error = %e,
                "llm.completion.timings.unreadable"
            );
        }
    }
}

/// Parse and emit in one step, absorbing every failure.
///
/// This is the call site a completion path uses: it never returns anything to
/// act on, because no timing problem is worth failing a completion over. A
/// missing or unreadable object is logged at DEBUG and forgotten.
pub fn observe_completion(backend: &str, model: &str, body: &serde_json::Value) {
    match parse_timings(body) {
        Ok(Some(timings)) => emit_completion_timings(backend, model, &timings),
        Ok(None) => {
            tracing::debug!(
                backend = %backend,
                model = %model,
                "llm.completion.timings.absent"
            );
        }
        Err(e) => {
            tracing::debug!(
                backend = %backend,
                model = %model,
                error = %e,
                "llm.completion.timings.unreadable"
            );
        }
    }
}

/// Emit one INFO event carrying the completion's separated timings.
///
/// Field names are the canonical ones from the measurement dictionary, so a log
/// line and a measurement record denote the same quantities under the same
/// names with no translation.
pub fn emit_completion_timings(backend: &str, model: &str, t: &EngineTimings) {
    tracing::info!(
        backend = %backend,
        model = %model,
        prompt_tok_total = t.prompt_tok_total(),
        prompt_tok_computed = t.prompt_tok_computed,
        prompt_tok_cached = t.prompt_tok_cached,
        prompt_cache_hit_ratio = %opt_f64(t.prompt_cache_hit_ratio()),
        prefill_ms = t.prefill_ms,
        decode_tok = t.decode_tok,
        decode_ms = t.decode_ms,
        prefill_tps = %opt_f64(t.prefill_tps()),
        decode_tps = %opt_f64(t.decode_tps()),
        engine_prefill_tps = %opt_f64(t.engine_prefill_tps),
        engine_decode_tps = %opt_f64(t.engine_decode_tps),
        "llm.completion.timings"
    );
}

/// Render an optional rate for a tracing field.
///
/// An undefined rate reads `unset`, never `0`: a fully cached prompt has no
/// prefill rate, and a zero there would be indistinguishable from a stall.
fn opt_f64(v: Option<f64>) -> String {
    match v {
        Some(v) => format!("{v}"),
        None => "unset".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A verbatim `llama-server` b9870 response body, recorded from a cold
    /// request against Ministral-3-8B on an M3 Ultra. Trimmed to the fields the
    /// parser reads plus enough envelope to be a realistic body.
    fn recorded_cold_response() -> serde_json::Value {
        serde_json::json!({
            "choices": [{ "finish_reason": "length", "index": 0,
                          "message": { "role": "assistant", "content": "item-000 quantity 0" } }],
            "created": 1_785_000_000,
            "model": "local",
            "object": "chat.completion",
            "usage": { "completion_tokens": 60, "prompt_tokens": 3456, "total_tokens": 3516,
                       "prompt_tokens_details": { "cached_tokens": 0 } },
            "timings": {
                "cache_n": 0,
                "prompt_n": 3456,
                "prompt_ms": 3815.254,
                "prompt_per_token_ms": 1.1039508101851851,
                "prompt_per_second": 905.8374619356929,
                "predicted_n": 60,
                "predicted_ms": 897.89,
                "predicted_per_token_ms": 14.964833333333333,
                "predicted_per_second": 66.82333025203533
            }
        })
    }

    /// The warm counterpart of the same recorded run: nearly the whole prompt
    /// was served from cache.
    fn recorded_warm_response() -> serde_json::Value {
        serde_json::json!({
            "timings": {
                "cache_n": 3455,
                "prompt_n": 17,
                "prompt_ms": 71.235,
                "prompt_per_second": 238.64673264546923,
                "predicted_n": 60,
                "predicted_ms": 906.848,
                "predicted_per_second": 66.16323794064716
            }
        })
    }

    #[test]
    fn test_parse_timings_recorded_response_carries_expected_values() {
        // GIVEN a recorded llama-server response containing timings
        let body = recorded_cold_response();

        // WHEN it is parsed
        let parsed = parse_timings(&body);

        // THEN the struct carries the engine's figures under the canonical names
        let t = parsed.expect("parse succeeds").expect("timings present");
        assert_eq!(
            t,
            EngineTimings {
                prompt_tok_computed: 3456,
                prompt_tok_cached: 0,
                prefill_ms: 3815.254,
                decode_tok: 60,
                decode_ms: 897.89,
                engine_prefill_tps: Some(905.8374619356929),
                engine_decode_tps: Some(66.82333025203533),
            }
        );
    }

    #[test]
    fn test_parse_timings_response_without_timings_is_none() {
        // GIVEN a response body carrying no timings key, as any cloud backend returns
        let body = serde_json::json!({
            "choices": [], "model": "gpt-4o", "object": "chat.completion"
        });

        // WHEN it is parsed
        let parsed = parse_timings(&body);

        // THEN the result is None and no error surfaces to the caller
        assert!(matches!(parsed, Ok(None)));
    }

    #[test]
    fn test_parse_timings_null_timings_is_none() {
        // GIVEN a response whose timings key is explicitly null
        let body = serde_json::json!({ "timings": serde_json::Value::Null });

        // WHEN it is parsed
        let parsed = parse_timings(&body);

        // THEN it is treated as absent, not as malformed
        assert!(matches!(parsed, Ok(None)));
    }

    #[test]
    fn test_parse_timings_non_object_timings_is_an_error() {
        // GIVEN a response whose timings key is not an object
        let body = serde_json::json!({ "timings": "3815ms" });

        // WHEN it is parsed
        let parsed = parse_timings(&body);

        // THEN the malformed object is reported rather than silently skipped
        assert!(matches!(parsed, Err(TimingsError::NotAnObject)));
    }

    #[test]
    fn test_parse_timings_missing_required_field_is_an_error() {
        // GIVEN a timings object with no prompt_ms
        let body = serde_json::json!({
            "timings": { "prompt_n": 10, "cache_n": 0, "predicted_n": 5, "predicted_ms": 50.0 }
        });

        // WHEN it is parsed
        let parsed = parse_timings(&body);

        // THEN the offending field is named
        assert!(matches!(
            parsed,
            Err(TimingsError::BadField { field: "prompt_ms" })
        ));
    }

    #[test]
    fn test_parse_timings_absent_cache_n_defaults_to_zero() {
        // GIVEN a build that predates prompt-cache reporting, so cache_n is absent
        let body = serde_json::json!({
            "timings": { "prompt_n": 10, "prompt_ms": 5.0, "predicted_n": 5, "predicted_ms": 50.0 }
        });

        // WHEN it is parsed
        let t = parse_timings(&body)
            .expect("parse succeeds")
            .expect("timings present");

        // THEN prefill and decode stay usable and nothing is reported as cached
        assert_eq!(t.prompt_tok_cached, 0);
        assert_eq!(t.prompt_tok_total(), 10);
    }

    #[test]
    fn test_prompt_tok_total_sums_computed_and_cached() {
        // GIVEN the recorded warm response, whose prompt was nearly all cached
        let t = parse_timings(&recorded_warm_response())
            .expect("parse succeeds")
            .expect("timings present");

        // WHEN the submitted prompt size is derived
        let total = t.prompt_tok_total();

        // THEN it matches the engine's own usage.prompt_tokens for that request
        assert_eq!(total, 3472);
        assert_eq!(t.prompt_tok_computed, 17);
        assert_eq!(t.prompt_tok_cached, 3455);
    }

    #[test]
    fn test_derived_rates_match_the_engine_on_a_recorded_response() {
        // GIVEN the recorded cold response
        let t = parse_timings(&recorded_cold_response())
            .expect("parse succeeds")
            .expect("timings present");

        // WHEN the rates are derived from counts and durations
        let prefill = t.prefill_tps().expect("prefill rate defined");
        let decode = t.decode_tps().expect("decode rate defined");

        // THEN they agree with the engine's independently reported figures
        assert!((prefill - 905.8374619356929).abs() < 1e-9, "{prefill}");
        assert!((decode - 66.82333025203533).abs() < 1e-9, "{decode}");
    }

    #[test]
    fn test_derived_rates_separate_prefill_from_decode_by_an_order_of_magnitude() {
        // GIVEN the recorded cold response from a GPU-resident model
        let t = parse_timings(&recorded_cold_response())
            .expect("parse succeeds")
            .expect("timings present");

        // WHEN prefill and decode rates are compared
        let ratio = t.prefill_tps().unwrap_or_default() / t.decode_tps().unwrap_or_default();

        // THEN they differ by at least 10x, the signature of a separated measurement
        assert!(ratio >= 10.0, "prefill/decode ratio was {ratio}");
    }

    #[test]
    fn test_prefill_tps_is_none_when_nothing_was_computed() {
        // GIVEN a fully cached prompt, where the engine evaluated no new tokens
        let body = serde_json::json!({
            "timings": { "prompt_n": 0, "cache_n": 4031, "prompt_ms": 0.0,
                         "predicted_n": 34, "predicted_ms": 512.659 }
        });

        // WHEN the prefill rate is derived
        let t = parse_timings(&body)
            .expect("parse succeeds")
            .expect("timings present");

        // THEN it is undefined rather than zero, which would read as a stall
        assert_eq!(t.prefill_tps(), None);
        assert!(t.decode_tps().is_some());
    }

    #[test]
    fn test_prompt_cache_hit_ratio_on_a_recorded_warm_response() {
        // GIVEN the recorded warm response
        let t = parse_timings(&recorded_warm_response())
            .expect("parse succeeds")
            .expect("timings present");

        // WHEN the hit ratio is derived
        let ratio = t.prompt_cache_hit_ratio().expect("ratio defined");

        // THEN it is the cached share of the submitted prompt
        assert!((ratio - 3455.0 / 3472.0).abs() < 1e-12, "{ratio}");
    }

    #[test]
    fn test_prompt_cache_hit_ratio_is_none_for_an_empty_prompt() {
        // GIVEN a completion with no prompt tokens at all
        let body = serde_json::json!({
            "timings": { "prompt_n": 0, "cache_n": 0, "prompt_ms": 0.0,
                         "predicted_n": 1, "predicted_ms": 10.0 }
        });

        // WHEN the hit ratio is derived
        let t = parse_timings(&body)
            .expect("parse succeeds")
            .expect("timings present");

        // THEN it is undefined rather than zero
        assert_eq!(t.prompt_cache_hit_ratio(), None);
    }

    #[test]
    fn test_observe_completion_absorbs_a_malformed_object() {
        // GIVEN a response whose timings cannot be interpreted
        let body = serde_json::json!({ "timings": 42 });

        // WHEN it is observed on the completion path
        observe_completion("llama-local", "ministral-3-8b", &body);

        // THEN nothing propagates: reaching this line is the assertion
    }
}
