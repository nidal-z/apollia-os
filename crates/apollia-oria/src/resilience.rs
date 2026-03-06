//! ResilienceLayer — per-tool circuit breaker for production reliability.
//!
//! Each registered tool has its own [`CircuitBreaker`] with three states:
//! `Closed` (normal), `Open` (rejecting), `HalfOpen` (probing).
//!
//! Only [`ErrorClass::Transient`] errors (timeout, rate limit) increment
//! the failure counter. Permanent errors pass through without affecting
//! the circuit state.

use std::collections::HashMap;
use std::time::{Duration, Instant};

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
}
