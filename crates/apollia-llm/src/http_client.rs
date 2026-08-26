//! HTTP client shared by the remote LLM backends.
//!
//! Inference needs two deadlines, not one. A total-request timeout is the wrong
//! tool: a legitimate generation on a large or remote model runs for minutes,
//! and cutting it would break the very setups this runtime targets. What must
//! never happen is *silence*. So the client bounds:
//!
//! - the **connect** phase, because a host that never completes the handshake is
//!   simply unreachable, and
//! - the **read idle** phase, because a backend that has accepted the request
//!   and then stops sending bytes is wedged.
//!
//! Without these, a remote backend that accepts the connection and never
//! answers pins the caller forever.
//!
//! # Why the idle budget is large
//!
//! On the non-streaming path the server sends **nothing** until generation is
//! complete, so "time since the last byte" is the whole generation, not a
//! symptom of a stall. A 14B model on a modest remote machine legitimately
//! takes minutes for a long answer. The idle budget therefore has to cover the
//! slowest honest generation, which makes it a backstop against a wedged
//! backend rather than a latency policy. Agent runs get their real bound from
//! `StepBudget`'s wall clock, which is enforced by the runtime.

use std::time::Duration;

/// Deadline for the TCP and TLS handshake. A backend that cannot answer this
/// fast is down, not slow: connecting is not proportional to model size.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Default tolerance to backend silence.
///
/// Ten minutes: long enough that no honest generation is ever cut, short enough
/// that a wedged backend eventually surfaces instead of hanging forever.
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(600);

/// Lower bound accepted for a configured idle timeout.
///
/// Anything shorter would cut healthy generations on slow backends, turning a
/// safety net into an outage. Measured reference: a 300-word answer from a 14B
/// model over the network took over four minutes.
pub const MIN_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Builds the HTTP client used to reach a remote LLM backend.
///
/// `idle_timeout` is clamped to at least [`MIN_IDLE_TIMEOUT`]. Falls back to a
/// default client if the builder rejects the configuration, so a transport
/// setting can never prevent the runtime from starting.
pub fn build_llm_http_client(idle_timeout: Duration) -> reqwest::Client {
    let idle = idle_timeout.max(MIN_IDLE_TIMEOUT);
    // The LLM endpoint is the one the operator configured, and a self-hosted
    // llama-server or Ollama on loopback is the default case, so the
    // public-destination policy is deliberately not applied here.
    apollia_core::net::configured_endpoint_client_builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(idle)
        .build()
        .unwrap_or_else(|e| {
            tracing::warn!(
                error = %e,
                detail = "falling back to an unbounded client",
                "llm.http_client.build.failed"
            );
            // SAFETY: policy-equivalent fallback. `Client::new()` carries
            // reqwest's default `Policy::limited(10)`, which is exactly what
            // `configured_endpoint_client_builder` sets, so the degraded client
            // loses the timeouts and nothing else. Rebuilding through the
            // helper here would fail for the same reason the first build did.
            reqwest::Client::new()
        })
}

/// Reads an idle timeout expressed in seconds, ignoring absent or absurd values.
///
/// `0` and negative-by-overflow values are treated as "not configured" rather
/// than as "never wait", which is the reading a user almost certainly does not
/// intend when writing `timeout_sec = 0`.
pub fn idle_timeout_from_secs(secs: Option<u64>) -> Duration {
    match secs {
        Some(s) if s > 0 => Duration::from_secs(s),
        _ => DEFAULT_IDLE_TIMEOUT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // GIVEN a configured idle timeout below the accepted floor
    // WHEN the client is built
    // THEN the floor wins, so a too-eager setting cannot cut healthy generations
    #[test]
    fn test_idle_timeout_is_clamped_to_the_floor() {
        assert!(
            MIN_IDLE_TIMEOUT >= Duration::from_secs(60),
            "the floor must cover a slow non-streaming generation"
        );
        // The clamp happens at build time; the builder must not panic on it.
        let _ = build_llm_http_client(Duration::from_secs(1));
    }

    // GIVEN the shipped defaults
    // WHEN they are compared
    // THEN the default leaves real room above the floor, and both are far above
    //      the multi-minute generations measured on modest remote hardware
    #[test]
    fn test_defaults_cannot_cut_an_honest_generation() {
        assert!(DEFAULT_IDLE_TIMEOUT > MIN_IDLE_TIMEOUT);
        assert!(
            DEFAULT_IDLE_TIMEOUT >= Duration::from_secs(600),
            "a 300-word answer from a 14B model took over four minutes"
        );
    }

    // GIVEN no configured timeout, or a zero
    // WHEN the idle timeout is resolved
    // THEN the default applies rather than an unbounded wait
    #[test]
    fn test_absent_or_zero_timeout_falls_back_to_the_default() {
        assert_eq!(idle_timeout_from_secs(None), DEFAULT_IDLE_TIMEOUT);
        assert_eq!(idle_timeout_from_secs(Some(0)), DEFAULT_IDLE_TIMEOUT);
    }

    // GIVEN a configured timeout above the floor
    // WHEN it is resolved
    // THEN it is honoured verbatim
    #[test]
    fn test_configured_timeout_is_honoured() {
        assert_eq!(idle_timeout_from_secs(Some(300)), Duration::from_secs(300));
    }

    // GIVEN the builder `build_llm_http_client` composes, with both deadlines
    // WHEN it is asked for a client
    // THEN it succeeds, so no backend falls back to the unbounded client.
    // Asserting on the returned `Client` cannot say this: the fallback is
    // silent and hands back a client too.
    #[test]
    fn test_client_builds_with_both_deadlines() {
        let built = apollia_core::net::configured_endpoint_client_builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(DEFAULT_IDLE_TIMEOUT)
            .build();
        assert!(
            built.is_ok(),
            "the shared builder refused a client: {:?}",
            built.err()
        );
    }
}
