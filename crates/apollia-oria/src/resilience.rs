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
use std::time::{Duration, Instant};

use rand::Rng;

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

    /// Returns the current state of the circuit breaker.
    pub fn state(&self) -> &CircuitState {
        &self.state
    }

    /// Returns the tool name this circuit breaker is associated with.
    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    /// Returns the current failure count.
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

    // AC-1 — Closed state allows calls
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

    // AC-2 — Transient errors open the circuit after threshold
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

    // AC-2 continued — Open rejects immediately
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

    // AC-3 — Cooldown transitions to HalfOpen
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

    // AC-3 — HalfOpen success closes circuit
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

    // AC-3 — HalfOpen failure reopens circuit
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

    // AC-4 — Permanent errors do not increment
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

    // AC-4 — BudgetExceeded does not increment
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

    // AC-4 — SandboxViolation does not increment
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

    // AC-5 — Success resets failure count
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

    // AC-6 — Independent circuit breakers
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

    // AC-1 — Exponential backoff delays
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

    // AC-6 — Max delay caps the backoff
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

    // AC-2 — Jitter adds randomness
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

    // AC-4 (partial) — Success on first try, no retry needed
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

    // AC-4 — Transient failures then success
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

    // AC-3 — Permanent error returns immediately, no retry
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

    // AC-5 — All retries exhausted, failure recorded on circuit breaker
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
