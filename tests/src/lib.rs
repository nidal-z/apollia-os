// apollia-e2e-tests: workspace-level integration test crate.
// Actual tests are in integration/*.rs (see [[test]] sections in Cargo.toml).

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

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

/// Reserve a loopback TCP port for a listener this process binds later.
///
/// The idiom this replaces bound `127.0.0.1:0`, read `local_addr()`, released
/// the listener and assumed the number stayed acquired. It does not: the number
/// comes out of the operating system's ephemeral pool, and the pool hands it to
/// whoever asks next. Under a second test run, or any process consuming
/// ephemeral ports, the real bind that follows fails with
/// `Address already in use` on a port this process had just been given.
///
/// Here the number is never obtained from the operating system. It is drawn
/// from [`PORT_POOL_START`], a range no ephemeral pool covers, so no third
/// party asking for a port can be handed one of these. The candidate is
/// probe-bound only to skip a port some unrelated service already holds, never
/// to learn its number, and the cursor never yields the same number twice
/// inside one process.
///
/// Each integration test binary is its own process, so each gets its own
/// cursor. The runtime crate carries the same helper for its in-crate tests,
/// in `crates/apollia-runtime/src/test_support.rs`; a `#[cfg(test)]` module
/// there cannot be reached from here, and this crate cannot be a dependency of
/// the crate it tests.
pub fn reserve_port() -> u16 {
    for _ in 0..PORT_POOL_LEN {
        let offset = port_cursor().fetch_add(1, Ordering::Relaxed) % PORT_POOL_LEN;
        let candidate = PORT_POOL_START + offset;
        if std::net::TcpListener::bind(("127.0.0.1", candidate)).is_ok() {
            return candidate;
        }
    }
    panic!(
        "no free port in {}..{}",
        PORT_POOL_START,
        PORT_POOL_START + PORT_POOL_LEN
    );
}
