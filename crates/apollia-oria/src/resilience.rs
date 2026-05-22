//! ResilienceLayer — per-tool circuit breaker and retry policy for production reliability.
//!
//! Each registered tool has its own [`CircuitBreaker`] with three states:
//! `Closed` (normal), `Open` (rejecting), `HalfOpen` (probing).
//!
//! [`RetryPolicy`] adds exponential backoff with jitter for transient errors,
//! retrying automatically before counting a failure on the circuit breaker.
//!
//! Only [`ErrorClass::Transient`] errors (timeout, rate limit) are retried
//! and increment the failure counter. Permanent errors pass through without
//! affecting the circuit state.

use std::collections::HashMap;
use std::future::Future;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use apollia_core::error_analysis::{ErrorAnalysis, ErrorCategory};
use apollia_core::events::{EventBusSender, RuntimeEvent};
use apollia_core::retry_attempt::{AttemptOutcome, RetryAttempt};
use rand::Rng;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Classification of errors to determine circuit breaker behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorClass {
    /// Timeout, rate limit — retryable, increments circuit breaker counter.
    Transient,
    /// Invalid input, file not found — never retry, does not affect circuit.
    Permanent,
    /// StepBudget exhausted — do not retry, does not affect circuit.
    BudgetExceeded,
    /// Path traversal attempt, unauthorized network access — does not affect circuit.
    SandboxViolation,
}

/// State of a circuit breaker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal — requests pass through.
    Closed,
    /// Circuit open — requests rejected immediately.
    Open,
    /// Probe — one request allowed to test recovery.
    HalfOpen,
}

/// Per-tool circuit breaker.
///
/// Tracks consecutive transient failures. When the failure count reaches
/// `failure_threshold`, the circuit opens and rejects all calls until
/// the cooldown elapses.
#[derive(Debug)]
pub struct CircuitBreaker {
    tool_name: String,
    state: CircuitState,
    failure_count: u32,
    failure_threshold: u32,
    cooldown: Duration,
    last_failure_at: Option<Instant>,
}

impl CircuitBreaker {
    /// Creates a new circuit breaker for the given tool.
    fn new(tool_name: String, failure_threshold: u32, cooldown: Duration) -> Self {
        Self {
            tool_name,
            state: CircuitState::Closed,
            failure_count: 0,
            failure_threshold,
            cooldown,
            last_failure_at: None,
        }
    }

    pub fn state(&self) -> &CircuitState {
        &self.state
    }

    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    pub fn failure_count(&self) -> u32 {
        self.failure_count
    }

    /// Manually resets the circuit breaker to Closed state.
    pub fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.last_failure_at = None;
    }
}

/// Errors from the resilience layer.
#[derive(Debug, thiserror::Error)]
pub enum ResilienceError {
    /// Circuit is open for the given tool — call rejected without execution.
    #[error("circuit open for tool '{tool_name}': {failure_count} consecutive failures, retry after cooldown")]
    CircuitOpen {
        /// Name of the tool whose circuit is open.
        tool_name: String,
        /// Number of consecutive failures that triggered the opening.
        failure_count: u32,
    },

    /// Tool execution failed (wraps the underlying error message).
    #[error("tool execution failed: {0}")]
    ExecutionFailed(String),

    /// The requested tool is not registered in the resilience layer.
    #[error("unknown tool: {0}")]
    UnknownTool(String),
}

/// Snapshot of a circuit breaker emitted by [`ResilienceLayer::snapshot`].
///
/// Serialisable so HTTP routes can hand it directly to JSON encoders without
/// leaking the internal `Instant`-based representation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CircuitBreakerSnapshot {
    /// Canonical tool name (matches the registry).
    pub tool_name: String,
    /// Current state: `"closed"`, `"half_open"`, or `"open"`.
    pub state: String,
    /// Consecutive transient failures observed since the last success.
    pub failure_count: u32,
    /// Threshold beyond which the breaker opens.
    pub failure_threshold: u32,
    /// Configured cooldown in seconds.
    pub cooldown_secs: u64,
    /// Seconds left before the breaker auto-transitions to half-open.
    /// `None` when the breaker is closed or half-open.
    pub cooldown_remaining_secs: Option<u64>,
}

/// Resilience layer with independent circuit breakers per tool.
///
/// Use [`register_tool`](Self::register_tool) to add tools, then
/// [`pre_check`](Self::pre_check) before each call and
/// [`record_success`](Self::record_success) / [`record_failure`](Self::record_failure)
/// after to update the circuit state.
pub struct ResilienceLayer {
    circuit_breakers: HashMap<String, CircuitBreaker>,
    default_failure_threshold: u32,
    default_cooldown: Duration,
}

impl Default for ResilienceLayer {
    /// Crée une `ResilienceLayer` avec les paramètres par défaut :
    /// seuil de 3 échecs consécutifs, cooldown de 30 secondes.
    fn default() -> Self {
        Self::new(3, Duration::from_secs(30))
    }
}

impl ResilienceLayer {
    /// Creates a new resilience layer with default threshold and cooldown.
    pub fn new(default_failure_threshold: u32, default_cooldown: Duration) -> Self {
        Self {
            circuit_breakers: HashMap::new(),
            default_failure_threshold,
            default_cooldown,
        }
    }

    /// Registers a tool with its own circuit breaker using the layer defaults.
    pub fn register_tool(&mut self, tool_name: &str) {
        self.circuit_breakers.insert(
            tool_name.to_string(),
            CircuitBreaker::new(
                tool_name.to_string(),
                self.default_failure_threshold,
                self.default_cooldown,
            ),
        );
    }

    /// Checks whether an outgoing call to the tool is allowed.
    ///
    /// - **Closed**: always allowed.
    /// - **Open**: if cooldown has elapsed, transitions to HalfOpen and allows one probe.
    ///   Otherwise returns [`ResilienceError::CircuitOpen`].
    /// - **HalfOpen**: allowed (single probe in progress).
    pub fn pre_check(&mut self, tool_name: &str) -> Result<(), ResilienceError> {
        let cb = self
            .circuit_breakers
            .get_mut(tool_name)
            .ok_or_else(|| ResilienceError::UnknownTool(tool_name.to_string()))?;

        match cb.state {
            CircuitState::Closed | CircuitState::HalfOpen => Ok(()),
            CircuitState::Open => {
                let cooldown_elapsed = cb
                    .last_failure_at
                    .map(|t| t.elapsed() >= cb.cooldown)
                    .unwrap_or(false);

                if cooldown_elapsed {
                    cb.state = CircuitState::HalfOpen;
                    tracing::info!(
                        tool = %cb.tool_name,
                        "circuit breaker transitioning Open -> HalfOpen (cooldown elapsed)"
                    );
                    Ok(())
                } else {
                    Err(ResilienceError::CircuitOpen {
                        tool_name: cb.tool_name.clone(),
                        failure_count: cb.failure_count,
                    })
                }
            }
        }
    }

    /// Records a successful call — resets failure count and closes the circuit.
    ///
    /// Returns `true` if the circuit was restored from HalfOpen to Closed
    /// (useful for emitting `ToolCircuitRestored` events).
    pub fn record_success(&mut self, tool_name: &str) -> Result<bool, ResilienceError> {
        let cb = self
            .circuit_breakers
            .get_mut(tool_name)
            .ok_or_else(|| ResilienceError::UnknownTool(tool_name.to_string()))?;

        let was_half_open = cb.state == CircuitState::HalfOpen;

        cb.failure_count = 0;
        cb.last_failure_at = None;

        if was_half_open {
            cb.state = CircuitState::Closed;
            tracing::info!(
                tool = %cb.tool_name,
                "circuit breaker restored: HalfOpen -> Closed"
            );
        }

        Ok(was_half_open)
    }

    /// Records a failed call. Only [`ErrorClass::Transient`] errors affect the circuit.
    ///
    /// Returns `true` if the circuit just transitioned to Open
    /// (useful for emitting `ToolCircuitBroken` events).
    pub fn record_failure(
        &mut self,
        tool_name: &str,
        error_class: &ErrorClass,
    ) -> Result<bool, ResilienceError> {
        let cb = self
            .circuit_breakers
            .get_mut(tool_name)
            .ok_or_else(|| ResilienceError::UnknownTool(tool_name.to_string()))?;

        if *error_class != ErrorClass::Transient {
            return Ok(false);
        }

        // HalfOpen probe failed — reopen immediately
        if cb.state == CircuitState::HalfOpen {
            cb.state = CircuitState::Open;
            cb.last_failure_at = Some(Instant::now());
            tracing::warn!(
                tool = %cb.tool_name,
                "circuit breaker probe failed: HalfOpen -> Open"
            );
            return Ok(true);
        }

        cb.failure_count += 1;
        cb.last_failure_at = Some(Instant::now());

        if cb.failure_count >= cb.failure_threshold {
            cb.state = CircuitState::Open;
            tracing::warn!(
                tool = %cb.tool_name,
                failure_count = cb.failure_count,
                threshold = cb.failure_threshold,
                "circuit breaker opened: Closed -> Open"
            );
            return Ok(true);
        }

        Ok(false)
    }

    /// Returns an immutable reference to a circuit breaker by tool name.
    pub fn get(&self, tool_name: &str) -> Option<&CircuitBreaker> {
        self.circuit_breakers.get(tool_name)
    }

    /// Returns a mutable reference to a circuit breaker by tool name.
    pub fn get_mut(&mut self, tool_name: &str) -> Option<&mut CircuitBreaker> {
        self.circuit_breakers.get_mut(tool_name)
    }

    /// Produce an operator-friendly snapshot of every registered breaker.
    ///
    /// Returned in alphabetical order by `tool_name` so HTTP responses are
    /// deterministic across calls (eases diffing in tests and dashboards).
    /// Internal `Instant`s are converted to remaining-cooldown seconds for
    /// safe JSON serialisation.
    pub fn snapshot(&self) -> Vec<CircuitBreakerSnapshot> {
        let mut names: Vec<&String> = self.circuit_breakers.keys().collect();
        names.sort();
        names
            .into_iter()
            .filter_map(|name| {
                let cb = self.circuit_breakers.get(name)?;
                let cooldown_remaining_secs = match cb.state {
                    CircuitState::Open => cb.last_failure_at.and_then(|t| {
                        let elapsed = t.elapsed();
                        if elapsed >= cb.cooldown {
                            Some(0)
                        } else {
                            Some((cb.cooldown - elapsed).as_secs())
                        }
                    }),
                    _ => None,
                };
                Some(CircuitBreakerSnapshot {
                    tool_name: cb.tool_name.clone(),
                    state: match cb.state {
                        CircuitState::Closed => "closed".to_string(),
                        CircuitState::HalfOpen => "half_open".to_string(),
                        CircuitState::Open => "open".to_string(),
                    },
                    failure_count: cb.failure_count,
                    failure_threshold: cb.failure_threshold,
                    cooldown_secs: cb.cooldown.as_secs(),
                    cooldown_remaining_secs,
                })
            })
            .collect()
    }

    /// Force-close a circuit breaker, regardless of its current state.
    ///
    /// Returns `Ok(true)` when the tool was registered and the breaker now
    /// sits in `Closed` state. Returns `Ok(false)` when the tool is unknown
    /// (caller can choose to register it lazily before retrying).
    pub fn reset_breaker(&mut self, tool_name: &str) -> bool {
        match self.circuit_breakers.get_mut(tool_name) {
            Some(cb) => {
                let was_open = !matches!(cb.state, CircuitState::Closed);
                cb.state = CircuitState::Closed;
                cb.failure_count = 0;
                cb.last_failure_at = None;
                if was_open {
                    tracing::info!(
                        tool = %cb.tool_name,
                        "circuit breaker manually reset to Closed"
                    );
                }
                true
            }
            None => false,
        }
    }

    /// Register the tool implicitly when an event arrives for an unknown tool.
    ///
    /// Internal helper used by the runtime event subscriber that hydrates the
    /// shared resilience layer from `ToolCallCompleted` / `ToolCallDenied`
    /// events: tools never seen before still get tracked from their first
    /// reported call instead of producing `UnknownTool` errors.
    pub fn ensure_tool(&mut self, tool_name: &str) {
        if !self.circuit_breakers.contains_key(tool_name) {
            self.register_tool(tool_name);
        }
    }

    /// Executes an operation with retry policy and circuit breaker integration.
    ///
    /// 1. Checks the circuit breaker (`pre_check`).
    /// 2. Runs the operation.
    /// 3. On success: calls `record_success` and returns `Ok`.
    /// 4. On failure: classifies the error.
    ///    - **Transient** and attempts remaining: waits with exponential backoff, retries.
    ///    - **Transient** and all retries exhausted: calls `record_failure`, returns `Err`.
    ///    - **Other** (`Permanent`, `BudgetExceeded`, `SandboxViolation`): returns `Err` immediately.
    pub async fn execute<F, Fut, T>(
        &mut self,
        tool_name: &str,
        retry_policy: &RetryPolicy,
        error_classifier: impl Fn(&str) -> ErrorClass,
        operation: F,
    ) -> Result<T, ResilienceError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, String>>,
    {
        self.pre_check(tool_name)?;

        for attempt in 1..=retry_policy.max_attempts {
            match operation().await {
                Ok(value) => {
                    let _ = self.record_success(tool_name);
                    return Ok(value);
                }
                Err(err_msg) => {
                    let error_class = error_classifier(&err_msg);

                    if error_class != ErrorClass::Transient {
                        return Err(ResilienceError::ExecutionFailed(err_msg));
                    }

                    if attempt < retry_policy.max_attempts {
                        let delay = retry_policy.calculate_delay(attempt);
                        tracing::warn!(
                            tool = %tool_name,
                            attempt = attempt,
                            max_attempts = retry_policy.max_attempts,
                            delay_ms = delay.as_millis() as u64,
                            "transient error, retrying after backoff"
                        );
                        tokio::time::sleep(delay).await;
                    } else {
                        let _ = self.record_failure(tool_name, &ErrorClass::Transient);
                        return Err(ResilienceError::ExecutionFailed(err_msg));
                    }
                }
            }
        }

        unreachable!("loop always returns")
    }

    /// Variant of [`execute`](Self::execute) that captures each attempt as a
    /// [`RetryAttempt`] and emits [`RuntimeEvent::ToolCallRetrying`] before
    /// every new attempt.
    ///
    /// Returns the final outcome together with the chain of attempts. The
    /// chain always ends with either [`AttemptOutcome::Success`] or
    /// [`AttemptOutcome::Failed`] / [`AttemptOutcome::TimedOut`], never with
    /// an implicit truncation.
    pub async fn execute_with_observability<F, Fut, T>(
        &mut self,
        tool_name: &str,
        tool_call_id: &str,
        retry_policy: &RetryPolicy,
        error_classifier: impl Fn(&str) -> ErrorClass,
        bus: Option<&EventBusSender>,
        operation: F,
    ) -> (Result<T, ResilienceError>, Vec<RetryAttempt>)
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, String>>,
    {
        let mut attempts: Vec<RetryAttempt> = Vec::new();

        if let Err(e) = self.pre_check(tool_name) {
            return (Err(e), attempts);
        }

        for attempt in 1..=retry_policy.max_attempts {
            let started_at = now_ms();
            let start_instant = Instant::now();

            if attempt > 1 {
                if let Some(b) = bus {
                    let last_reason = attempts.last().and_then(|a| a.reason.clone());
                    let _ = b.send(RuntimeEvent::ToolCallRetrying {
                        tool_call_id: tool_call_id.to_string(),
                        tool_name: tool_name.to_string(),
                        attempt,
                        reason: last_reason,
                    });
                }
            }

            match operation().await {
                Ok(value) => {
                    let _ = self.record_success(tool_name);
                    attempts.push(RetryAttempt::new(
                        attempt,
                        started_at,
                        now_ms(),
                        AttemptOutcome::Success,
                    ));
                    return (Ok(value), attempts);
                }
                Err(err_msg) => {
                    let error_class = error_classifier(&err_msg);
                    let outcome = if err_msg.to_lowercase().contains("timeout") {
                        AttemptOutcome::TimedOut
                    } else {
                        AttemptOutcome::Failed
                    };
                    let analysis = ErrorAnalysis::new(
                        map_error_class_to_category(&error_class, &outcome),
                        err_msg.clone(),
                        err_msg.clone(),
                    );
                    attempts.push(
                        RetryAttempt::new(attempt, started_at, now_ms(), outcome)
                            .with_reason(analysis),
                    );

                    if error_class != ErrorClass::Transient {
                        return (Err(ResilienceError::ExecutionFailed(err_msg)), attempts);
                    }

                    if attempt < retry_policy.max_attempts {
                        let delay = retry_policy.calculate_delay(attempt);
                        tracing::warn!(
                            tool = %tool_name,
                            tool_call_id = %tool_call_id,
                            attempt = attempt,
                            delay_ms = delay.as_millis() as u64,
                            elapsed_ms = start_instant.elapsed().as_millis() as u64,
                            "transient error, retrying after backoff"
                        );
                        tokio::time::sleep(delay).await;
                    } else {
                        let _ = self.record_failure(tool_name, &ErrorClass::Transient);
                        return (Err(ResilienceError::ExecutionFailed(err_msg)), attempts);
                    }
                }
            }
        }

        unreachable!("loop always returns")
    }
}

fn map_error_class_to_category(class: &ErrorClass, outcome: &AttemptOutcome) -> ErrorCategory {
    if matches!(outcome, AttemptOutcome::TimedOut) {
        return ErrorCategory::Timeout;
    }
    match class {
        ErrorClass::Transient => ErrorCategory::ToolFailure,
        ErrorClass::Permanent => ErrorCategory::ToolFailure,
        ErrorClass::BudgetExceeded => ErrorCategory::ToolFailure,
        ErrorClass::SandboxViolation => ErrorCategory::PermissionDenied,
    }
}

/// Retry policy with exponential backoff and optional jitter.
///
/// Computes delays as `min(base_delay * 2^(attempt-1), max_delay)` with
/// an optional random jitter of +/- 25% to desynchronize concurrent retries.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of attempts (including the initial call).
    pub max_attempts: u32,
    /// Base delay for the first retry (in milliseconds).
    pub base_delay_ms: u64,
    /// Maximum delay cap for exponential backoff (in milliseconds).
    pub max_delay_ms: u64,
    /// Whether to add random jitter (+/- 25%) to the computed delay.
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 500,
            max_delay_ms: 10_000,
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// Calculates the delay for a given attempt number (1-indexed).
    ///
    /// Formula: `min(base_delay_ms * 2^(attempt - 1), max_delay_ms)`,
    /// with optional jitter of +/- 25%.
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1);
        let multiplier = 1u64.checked_shl(exponent).unwrap_or(u64::MAX);
        let raw_delay = self.base_delay_ms.saturating_mul(multiplier);
        let capped = raw_delay.min(self.max_delay_ms);

        let final_delay = if self.jitter {
            let mut rng = rand::thread_rng();
            let jitter_factor: f64 = rng.gen_range(0.75..=1.25);
            let jittered = (capped as f64 * jitter_factor) as u64;
            jittered.min(self.max_delay_ms)
        } else {
            capped
        };

        Duration::from_millis(final_delay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_layer(threshold: u32) -> ResilienceLayer {
        ResilienceLayer::new(threshold, Duration::from_secs(30))
    }

    // Closed state allows calls
    #[test]
    fn test_closed_allows_call() {
        // GIVEN a CircuitBreaker in Closed state
        let mut layer = make_layer(5);
        layer.register_tool("file_io");

        // WHEN pre_check()
        let result = layer.pre_check("file_io");

        // THEN Ok(())
        assert!(result.is_ok());
        assert_eq!(layer.get("file_io").unwrap().state(), &CircuitState::Closed);
    }

    // Transient errors open the circuit after threshold
    #[test]
    fn test_transient_errors_open_circuit() {
        // GIVEN failure_threshold = 3
        let mut layer = make_layer(3);
        layer.register_tool("file_io");

        // WHEN 3 record_failure(Transient)
        for i in 0..3 {
            let opened = layer
                .record_failure("file_io", &ErrorClass::Transient)
                .unwrap();
            if i < 2 {
                assert!(!opened);
            } else {
                assert!(opened);
            }
        }

        // THEN state() == Open
        assert_eq!(layer.get("file_io").unwrap().state(), &CircuitState::Open);
    }

    // continued — Open rejects immediately
    #[test]
    fn test_open_rejects_immediately() {
        // GIVEN circuit in Open (cooldown not elapsed)
        let mut layer = make_layer(1);
        layer.register_tool("file_io");
        layer
            .record_failure("file_io", &ErrorClass::Transient)
            .unwrap();

        // WHEN pre_check()
        let result = layer.pre_check("file_io");

        // THEN Err(CircuitOpen)
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ResilienceError::CircuitOpen { .. }
        ));
    }

    // Cooldown transitions to HalfOpen
    #[test]
    fn test_cooldown_transitions_to_half_open() {
        // GIVEN circuit in Open with cooldown = 0ms (immediately elapsed)
        let mut layer = ResilienceLayer::new(1, Duration::from_millis(0));
        layer.register_tool("file_io");
        layer
            .record_failure("file_io", &ErrorClass::Transient)
            .unwrap();
        assert_eq!(layer.get("file_io").unwrap().state(), &CircuitState::Open);

        // Sleep briefly to ensure Instant::elapsed() > 0
        std::thread::sleep(Duration::from_millis(1));

        // WHEN pre_check()
        let result = layer.pre_check("file_io");

        // THEN Ok(()) and state == HalfOpen
        assert!(result.is_ok());
        assert_eq!(
            layer.get("file_io").unwrap().state(),
            &CircuitState::HalfOpen
        );
    }

    // HalfOpen success closes circuit
    #[test]
    fn test_half_open_success_closes_circuit() {
        // GIVEN circuit in HalfOpen
        let mut layer = ResilienceLayer::new(1, Duration::from_millis(0));
        layer.register_tool("file_io");
        layer
            .record_failure("file_io", &ErrorClass::Transient)
            .unwrap();
        std::thread::sleep(Duration::from_millis(1));
        layer.pre_check("file_io").unwrap(); // transitions to HalfOpen

        // WHEN record_success()
        let restored = layer.record_success("file_io").unwrap();

        // THEN state == Closed and failure_count == 0
        assert!(restored);
        let cb = layer.get("file_io").unwrap();
        assert_eq!(cb.state(), &CircuitState::Closed);
        assert_eq!(cb.failure_count(), 0);
    }

    // HalfOpen failure reopens circuit
    #[test]
    fn test_half_open_failure_reopens_circuit() {
        // GIVEN circuit in HalfOpen
        let mut layer = ResilienceLayer::new(1, Duration::from_millis(0));
        layer.register_tool("file_io");
        layer
            .record_failure("file_io", &ErrorClass::Transient)
            .unwrap();
        std::thread::sleep(Duration::from_millis(1));
        layer.pre_check("file_io").unwrap(); // transitions to HalfOpen

        // WHEN record_failure(Transient)
        let opened = layer
            .record_failure("file_io", &ErrorClass::Transient)
            .unwrap();

        // THEN state == Open
        assert!(opened);
        assert_eq!(layer.get("file_io").unwrap().state(), &CircuitState::Open);
    }

    // Permanent errors do not increment
    #[test]
    fn test_permanent_error_does_not_increment() {
        // GIVEN circuit in Closed with failure_count == 0
        let mut layer = make_layer(5);
        layer.register_tool("file_io");

        // WHEN record_failure(Permanent)
        let opened = layer
            .record_failure("file_io", &ErrorClass::Permanent)
            .unwrap();

        // THEN failure_count == 0 and state == Closed
        assert!(!opened);
        let cb = layer.get("file_io").unwrap();
        assert_eq!(cb.failure_count(), 0);
        assert_eq!(cb.state(), &CircuitState::Closed);
    }

    // BudgetExceeded does not increment
    #[test]
    fn test_budget_exceeded_does_not_increment() {
        // GIVEN circuit in Closed
        let mut layer = make_layer(5);
        layer.register_tool("file_io");

        // WHEN record_failure(BudgetExceeded)
        let opened = layer
            .record_failure("file_io", &ErrorClass::BudgetExceeded)
            .unwrap();

        // THEN failure_count == 0 and state == Closed
        assert!(!opened);
        assert_eq!(layer.get("file_io").unwrap().failure_count(), 0);
    }

    // SandboxViolation does not increment
    #[test]
    fn test_sandbox_violation_does_not_increment() {
        // GIVEN circuit in Closed
        let mut layer = make_layer(5);
        layer.register_tool("file_io");

        // WHEN record_failure(SandboxViolation)
        let opened = layer
            .record_failure("file_io", &ErrorClass::SandboxViolation)
            .unwrap();

        // THEN failure_count == 0 and state == Closed
        assert!(!opened);
        assert_eq!(layer.get("file_io").unwrap().failure_count(), 0);
    }

    // Success resets failure count
    #[test]
    fn test_success_resets_failure_count() {
        // GIVEN failure_count == 3 (under threshold of 5)
        let mut layer = make_layer(5);
        layer.register_tool("file_io");
        for _ in 0..3 {
            layer
                .record_failure("file_io", &ErrorClass::Transient)
                .unwrap();
        }
        assert_eq!(layer.get("file_io").unwrap().failure_count(), 3);

        // WHEN record_success()
        layer.record_success("file_io").unwrap();

        // THEN failure_count == 0
        assert_eq!(layer.get("file_io").unwrap().failure_count(), 0);
    }

    // Independent circuit breakers
    #[test]
    fn test_independent_circuit_breakers() {
        // GIVEN ResilienceLayer with 3 tools
        let mut layer = make_layer(2);
        layer.register_tool("file_io");
        layer.register_tool("bash_executor");
        layer.register_tool("python_executor");

        // WHEN "file_io" goes Open
        layer
            .record_failure("file_io", &ErrorClass::Transient)
            .unwrap();
        layer
            .record_failure("file_io", &ErrorClass::Transient)
            .unwrap();

        // THEN "file_io" is Open, others are Closed
        assert_eq!(layer.get("file_io").unwrap().state(), &CircuitState::Open);
        assert_eq!(
            layer.get("bash_executor").unwrap().state(),
            &CircuitState::Closed
        );
        assert_eq!(
            layer.get("python_executor").unwrap().state(),
            &CircuitState::Closed
        );

        // AND calls to other tools work normally
        assert!(layer.pre_check("bash_executor").is_ok());
        assert!(layer.pre_check("python_executor").is_ok());
        assert!(layer.pre_check("file_io").is_err());
    }

    // Manual reset
    #[test]
    fn test_manual_reset() {
        // GIVEN circuit in Open
        let mut layer = make_layer(1);
        layer.register_tool("file_io");
        layer
            .record_failure("file_io", &ErrorClass::Transient)
            .unwrap();
        assert_eq!(layer.get("file_io").unwrap().state(), &CircuitState::Open);

        // WHEN reset()
        layer.get_mut("file_io").unwrap().reset();

        // THEN state == Closed and failure_count == 0
        let cb = layer.get("file_io").unwrap();
        assert_eq!(cb.state(), &CircuitState::Closed);
        assert_eq!(cb.failure_count(), 0);
    }

    // Unknown tool returns error
    #[test]
    fn test_unknown_tool_returns_error() {
        // GIVEN an empty layer
        let mut layer = make_layer(5);

        // WHEN pre_check for unregistered tool
        let result = layer.pre_check("nonexistent");

        // THEN UnknownTool error
        assert!(matches!(
            result.unwrap_err(),
            ResilienceError::UnknownTool(_)
        ));
    }

    // --- RetryPolicy tests ---

    // Exponential backoff delays
    #[test]
    fn test_calculate_delay_exponential() {
        // GIVEN RetryPolicy with base_delay=500, jitter=false
        let policy = RetryPolicy {
            max_attempts: 5,
            base_delay_ms: 500,
            max_delay_ms: 100_000,
            jitter: false,
        };

        // WHEN calculate_delay for attempts 1, 2, 3
        // THEN 500ms, 1000ms, 2000ms
        assert_eq!(policy.calculate_delay(1), Duration::from_millis(500));
        assert_eq!(policy.calculate_delay(2), Duration::from_millis(1000));
        assert_eq!(policy.calculate_delay(3), Duration::from_millis(2000));
    }

    // Max delay caps the backoff
    #[test]
    fn test_calculate_delay_capped_at_max() {
        // GIVEN RetryPolicy with base_delay=500, max_delay=2000
        let policy = RetryPolicy {
            max_attempts: 10,
            base_delay_ms: 500,
            max_delay_ms: 2000,
            jitter: false,
        };

        // WHEN calculate_delay for attempt 10
        let delay = policy.calculate_delay(10);

        // THEN <= 2000ms
        assert!(delay <= Duration::from_millis(2000));
        assert_eq!(delay, Duration::from_millis(2000));
    }

    // Jitter adds randomness
    #[test]
    fn test_calculate_delay_with_jitter() {
        // GIVEN RetryPolicy with jitter=true
        let policy = RetryPolicy {
            max_attempts: 3,
            base_delay_ms: 1000,
            max_delay_ms: 100_000,
            jitter: true,
        };

        // WHEN calculate_delay called 20 times for attempt 1
        let delays: Vec<Duration> = (0..20).map(|_| policy.calculate_delay(1)).collect();

        // THEN not all values are identical (jitter introduces variation)
        let first = delays[0];
        let has_variation = delays.iter().any(|d| *d != first);
        assert!(has_variation, "jitter should produce varying delays");

        // AND all values are within +/- 25% of base (750..=1250)
        for d in &delays {
            let ms = d.as_millis() as u64;
            assert!(ms >= 750, "delay {ms}ms below jitter floor 750ms");
            assert!(ms <= 1250, "delay {ms}ms above jitter ceiling 1250ms");
        }
    }

    // Default values
    #[test]
    fn test_default_retry_policy() {
        // GIVEN RetryPolicy::default()
        let policy = RetryPolicy::default();

        // THEN expected defaults
        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.base_delay_ms, 500);
        assert_eq!(policy.max_delay_ms, 10_000);
        assert!(policy.jitter);
    }

    // --- execute() tests ---

    fn transient_classifier(err: &str) -> ErrorClass {
        if err.contains("permanent") {
            ErrorClass::Permanent
        } else {
            ErrorClass::Transient
        }
    }

    fn fast_retry_policy(max_attempts: u32) -> RetryPolicy {
        RetryPolicy {
            max_attempts,
            base_delay_ms: 1,
            max_delay_ms: 5,
            jitter: false,
        }
    }

    // (partial) — Success on first try, no retry needed
    #[tokio::test]
    async fn test_execute_success_no_retry() {
        // GIVEN an operation that succeeds
        let mut layer = make_layer(5);
        layer.register_tool("t");
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();

        // WHEN execute()
        let result = layer
            .execute("t", &fast_retry_policy(3), transient_classifier, || {
                let cc = cc.clone();
                async move {
                    cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok::<_, String>(42)
                }
            })
            .await;

        // THEN Ok returned, operation called once
        assert_eq!(result.unwrap(), 42);
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    // Transient failures then success
    #[tokio::test]
    async fn test_execute_transient_then_success() {
        // GIVEN an operation that fails 2 times (Transient) then succeeds
        let mut layer = make_layer(5);
        layer.register_tool("t");
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();

        // WHEN execute() with max_attempts=3
        let result = layer
            .execute("t", &fast_retry_policy(3), transient_classifier, || {
                let cc = cc.clone();
                async move {
                    let n = cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if n < 2 {
                        Err("transient timeout".to_string())
                    } else {
                        Ok(99)
                    }
                }
            })
            .await;

        // THEN Ok returned after 3 calls
        assert_eq!(result.unwrap(), 99);
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 3);
        // AND circuit breaker is healthy (success recorded)
        assert_eq!(layer.get("t").unwrap().failure_count(), 0);
    }

    // Permanent error returns immediately, no retry
    #[tokio::test]
    async fn test_execute_permanent_no_retry() {
        // GIVEN an operation that fails with Permanent
        let mut layer = make_layer(5);
        layer.register_tool("t");
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();

        // WHEN execute()
        let result: Result<i32, _> = layer
            .execute("t", &fast_retry_policy(3), transient_classifier, || {
                let cc = cc.clone();
                async move {
                    cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Err("permanent error".to_string())
                }
            })
            .await;

        // THEN Err returned after 1 call (no retry)
        assert!(result.is_err());
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);
        // AND circuit breaker not affected
        assert_eq!(layer.get("t").unwrap().failure_count(), 0);
    }

    // fail twice then succeed, attempts captured with outcomes + event emitted
    #[tokio::test]
    async fn test_execute_with_observability_fail_twice_then_success() {
        use apollia_core::events::RuntimeEvent;
        use apollia_core::retry_attempt::AttemptOutcome;
        use tokio::sync::broadcast;

        // GIVEN a resilience layer, a bus, and an op that fails 2x then succeeds
        let mut layer = make_layer(5);
        layer.register_tool("t");
        let (tx, mut rx) = broadcast::channel::<RuntimeEvent>(16);
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();

        // WHEN execute_with_observability with max_attempts=3
        let (result, attempts) = layer
            .execute_with_observability(
                "t",
                "call-42",
                &fast_retry_policy(3),
                transient_classifier,
                Some(&tx),
                || {
                    let cc = cc.clone();
                    async move {
                        let n = cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        if n < 2 {
                            Err("transient boom".to_string())
                        } else {
                            Ok(7u32)
                        }
                    }
                },
            )
            .await;

        // THEN success, 3 attempts captured, last one is Success
        assert_eq!(result.unwrap(), 7);
        assert_eq!(attempts.len(), 3);
        assert!(matches!(attempts[0].outcome, AttemptOutcome::Failed));
        assert!(matches!(attempts[1].outcome, AttemptOutcome::Failed));
        assert!(matches!(attempts[2].outcome, AttemptOutcome::Success));
        assert!(attempts[0].reason.is_some());

        // AND 2 ToolCallRetrying events were emitted (one before attempt 2 and 3)
        let mut retries = 0;
        while let Ok(evt) = rx.try_recv() {
            if let RuntimeEvent::ToolCallRetrying {
                tool_call_id,
                attempt,
                ..
            } = evt
            {
                assert_eq!(tool_call_id, "call-42");
                assert!(attempt >= 2);
                retries += 1;
            }
        }
        assert_eq!(retries, 2);
    }

    // All retries exhausted, failure recorded on circuit breaker
    #[tokio::test]
    async fn test_execute_all_retries_exhausted() {
        // GIVEN an operation that always fails (Transient), max_attempts=3
        let mut layer = make_layer(5);
        layer.register_tool("t");
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();

        // WHEN execute()
        let result: Result<i32, _> = layer
            .execute("t", &fast_retry_policy(3), transient_classifier, || {
                let cc = cc.clone();
                async move {
                    cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Err("transient timeout".to_string())
                }
            })
            .await;

        // THEN Err returned after 3 calls
        assert!(result.is_err());
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 3);
        // AND record_failure was called on circuit breaker
        assert_eq!(layer.get("t").unwrap().failure_count(), 1);
    }
}
