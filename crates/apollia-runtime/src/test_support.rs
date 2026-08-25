//! Deterministic primitives for the crate's async tests: waiting for a
//! condition, and reserving a loopback TCP port.
//!
//! Timing-dependent tests must never sleep a fixed duration to "wait for
//! propagation": under load the delay loses its race and the suite fails
//! spuriously. These helpers wait for the actual condition, bounded by a
//! generous ceiling, so they succeed the instant the condition holds and stay
//! robust when the machine is saturated.

use std::future::Future;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Interval between two condition checks. Small enough that a satisfied
/// condition is observed almost immediately, large enough not to spin the CPU.
const POLL_INTERVAL: Duration = Duration::from_millis(2);

/// Poll `cond` until it returns `true` or `timeout` elapses.
///
/// Returns the last value of `cond` (`true` on success, `false` on timeout), so
/// a caller can assert the outcome. The condition is checked once before the
/// first wait, so an already-satisfied condition returns without sleeping.
pub(crate) async fn poll_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if cond() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Async variant of [`poll_until`] for conditions that must be awaited, e.g. an
/// HTTP request or an actor query. `cond` is re-evaluated (its future rebuilt)
/// on each iteration.
pub(crate) async fn poll_until_async<F, Fut>(timeout: Duration, mut cond: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if cond().await {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// First port of the private range these tests draw from.
///
/// Chosen below every ephemeral pool in use: macOS allocates 49152-65535
/// (`sysctl net.inet.ip.portrange.first`), Linux 32768-60999
/// (`/proc/sys/net/ipv4/ip_local_port_range`). A process that asks the
/// operating system for a port therefore cannot be handed one of these.
const PORT_POOL_START: u16 = 20_000;

/// Width of that range.
const PORT_POOL_LEN: u16 = 10_000;

/// Per-process cursor into the private range, seeded so that two test processes
/// started at the same moment do not walk the same ports in the same order.
fn port_cursor() -> &'static AtomicU16 {
    static CURSOR: OnceLock<AtomicU16> = OnceLock::new();
    CURSOR.get_or_init(|| {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let seed = (std::process::id() ^ nanos) % u32::from(PORT_POOL_LEN);
        AtomicU16::new(seed as u16)
    })
}

/// A reserved loopback port, held by its probe listener until released.
///
/// The bound listener is the reservation itself: while it is held, a
/// concurrent `reserve_port()` in this process or another probe-binds the
/// same number, fails, and skips it. Call [`ReservedPort::release`] at the
/// last moment before the real bind, so the unguarded window shrinks from
/// "between reservation and server start" to the instants between release
/// and bind.
pub(crate) struct ReservedPort {
    port: u16,
    /// Held, never read: keeping it bound is what keeps the number taken.
    _listener: std::net::TcpListener,
}

impl ReservedPort {
    /// The reserved number, without giving up the reservation.
    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// Drop the probe listener and hand the number to the caller.
    pub(crate) fn release(self) -> u16 {
        self.port
    }
}

/// Reserve a loopback TCP port for a listener this process binds later.
///
/// The idiom this replaces bound `127.0.0.1:0`, read `local_addr()`, released
/// the listener and assumed the number stayed acquired. It does not: the number
/// comes out of the operating system's ephemeral pool, and the pool hands it to
/// whoever asks next. Under a second test run, or any process consuming
/// ephemeral ports, the real bind that follows fails with
/// `Address already in use` on a port this process had just been given.
///
/// Two things close that window here. The number is drawn from
/// [`PORT_POOL_START`], a range no ephemeral pool covers, so no third party
/// asking the operating system for a port can be handed one of these; and the
/// probe listener is returned to the caller inside [`ReservedPort`] instead of
/// being released on the spot, so another test process probing the same range
/// keeps skipping the number until the caller releases it, right before the
/// real bind.
pub(crate) fn reserve_port() -> ReservedPort {
    for _ in 0..PORT_POOL_LEN {
        let offset = port_cursor().fetch_add(1, Ordering::Relaxed) % PORT_POOL_LEN;
        let candidate = PORT_POOL_START + offset;
        if let Ok(listener) = std::net::TcpListener::bind(("127.0.0.1", candidate)) {
            return ReservedPort {
                port: candidate,
                _listener: listener,
            };
        }
    }
    panic!(
        "no free port in {}..{}",
        PORT_POOL_START,
        PORT_POOL_START + PORT_POOL_LEN
    );
}
