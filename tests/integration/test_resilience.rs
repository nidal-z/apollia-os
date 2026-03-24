//! Integration tests — ResilienceLayer circuit breaker.
//!
//! Tests the circuit breaker lifecycle at the workspace integration level:
//! Closed → Open (on threshold) → HalfOpen (after cooldown) → Closed (on probe success).
//!
//! These tests do NOT require Python. They run in all CI environments.

use std::time::Duration;

use apollia_oria::resilience::{CircuitState, ErrorClass, ResilienceError, ResilienceLayer};

// Circuit opens after failure_threshold consecutive Transient errors
#[tokio::test]
async fn test_circuit_breaker_opens_on_threshold() {
    // GIVEN a ResilienceLayer with failure_threshold=3 and a registered tool
    let mut layer = ResilienceLayer::new(3, Duration::from_secs(60));
    layer.register_tool("test_tool");

    // Verify initial state is Closed
    assert_eq!(
        layer.get("test_tool").unwrap().state(),
        &CircuitState::Closed
    );
    assert!(layer.pre_check("test_tool").is_ok());

    // WHEN 3 consecutive Transient failures are recorded
    for i in 0..3 {
        let opened = layer
            .record_failure("test_tool", &ErrorClass::Transient)
            .expect("record_failure should not fail for known tool");
        // The circuit opens exactly on the 3rd failure
        assert_eq!(opened, i == 2, "circuit should open on 3rd failure only");
    }

    // THEN the circuit is Open
    assert_eq!(layer.get("test_tool").unwrap().state(), &CircuitState::Open);
    assert_eq!(layer.get("test_tool").unwrap().failure_count(), 3);

    // AND subsequent pre_check calls return CircuitOpen without executing
    let result = layer.pre_check("test_tool");
    assert!(
        matches!(
            result,
            Err(ResilienceError::CircuitOpen {
                failure_count: 3,
                ..
            })
        ),
        "expected CircuitOpen with failure_count=3, got: {result:?}"
    );

    // AND other tools remain unaffected (Closed)
    layer.register_tool("other_tool");
    assert!(layer.pre_check("other_tool").is_ok());
    assert_eq!(
        layer.get("other_tool").unwrap().state(),
        &CircuitState::Closed
    );
}

// Circuit transitions to HalfOpen after cooldown, then Closed on probe success
#[tokio::test]
async fn test_circuit_breaker_half_open_after_cooldown() {
    // GIVEN a circuit breaker with a very short cooldown (1ms) that is Open
    let mut layer = ResilienceLayer::new(1, Duration::from_millis(1));
    layer.register_tool("test_tool");

    // Trip the circuit
    let opened = layer
        .record_failure("test_tool", &ErrorClass::Transient)
        .unwrap();
    assert!(opened, "circuit should open after 1 failure (threshold=1)");
    assert_eq!(layer.get("test_tool").unwrap().state(), &CircuitState::Open);

    // Verify it rejects calls while Open (cooldown not yet elapsed)
    // Note: may or may not be elapsed within 1ms — just verify it's either Open or HalfOpen
    // (timing-sensitive: sleep 5ms to reliably elapse the 1ms cooldown)
    tokio::time::sleep(Duration::from_millis(5)).await;

    // WHEN pre_check is called after the cooldown has elapsed
    let result = layer.pre_check("test_tool");

    // THEN the call is allowed (transitions Open -> HalfOpen)
    assert!(
        result.is_ok(),
        "pre_check should succeed after cooldown elapsed"
    );
    assert_eq!(
        layer.get("test_tool").unwrap().state(),
        &CircuitState::HalfOpen,
        "circuit should be HalfOpen after cooldown"
    );

    // AND a successful probe closes the circuit
    let restored = layer.record_success("test_tool").unwrap();
    assert!(
        restored,
        "record_success on HalfOpen should return true (restored)"
    );
    assert_eq!(
        layer.get("test_tool").unwrap().state(),
        &CircuitState::Closed,
        "circuit should be Closed after successful probe"
    );
    assert_eq!(layer.get("test_tool").unwrap().failure_count(), 0);
}

// HalfOpen probe failure reopens the circuit
#[tokio::test]
async fn test_circuit_half_open_probe_failure_reopens() {
    // GIVEN a circuit in HalfOpen state
    let mut layer = ResilienceLayer::new(1, Duration::from_millis(1));
    layer.register_tool("test_tool");
    layer
        .record_failure("test_tool", &ErrorClass::Transient)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;
    layer.pre_check("test_tool").unwrap(); // transitions to HalfOpen

    assert_eq!(
        layer.get("test_tool").unwrap().state(),
        &CircuitState::HalfOpen
    );

    // WHEN the probe fails with a Transient error
    let reopened = layer
        .record_failure("test_tool", &ErrorClass::Transient)
        .unwrap();

    // THEN the circuit reopens immediately (HalfOpen -> Open)
    assert!(reopened, "probe failure should reopen the circuit");
    assert_eq!(layer.get("test_tool").unwrap().state(), &CircuitState::Open);
}

// execute() with retry retries on Transient, not on Permanent
#[tokio::test]
async fn test_execute_with_retry_transient_then_success() {
    use apollia_oria::resilience::RetryPolicy;
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

    // GIVEN a layer and an operation that fails once then succeeds
    let mut layer = ResilienceLayer::new(5, Duration::from_secs(60));
    layer.register_tool("tool");
    let call_count = Arc::new(AtomicU32::new(0));
    let cc = call_count.clone();

    let policy = RetryPolicy {
        max_attempts: 3,
        base_delay_ms: 1,
        max_delay_ms: 5,
        jitter: false,
    };

    // WHEN execute() is called with an op that fails once (Transient) then succeeds
    let result = layer
        .execute(
            "tool",
            &policy,
            |e| {
                if e.contains("transient") {
                    ErrorClass::Transient
                } else {
                    ErrorClass::Permanent
                }
            },
            || {
                let cc = cc.clone();
                async move {
                    let n = cc.fetch_add(1, Ordering::SeqCst);
                    if n == 0 {
                        Err("transient timeout".to_string())
                    } else {
                        Ok(42u32)
                    }
                }
            },
        )
        .await;

    // THEN Ok returned after 2 calls
    assert_eq!(result.unwrap(), 42);
    assert_eq!(call_count.load(Ordering::SeqCst), 2);
    // AND circuit is still Closed (failure then success = reset)
    assert_eq!(layer.get("tool").unwrap().failure_count(), 0);
}
