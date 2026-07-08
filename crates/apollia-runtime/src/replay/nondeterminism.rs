//! Instrumented sources of non-determinism for deterministic replay.
//!
//! Run logic that needs the wall clock or randomness goes through these traits
//! instead of calling `std::time` or `rand` directly. In production the real
//! implementations are injected; the replay harness substitutes cursors backed
//! by the captured journal so a replayed run observes the exact same values as
//! the original. The traits introduce no third-party dependency: they wrap
//! `std::time` and the already-present `rand` crate.

/// Abstraction over the system clock for deterministic replay.
///
/// Production code is given a [`RealClock`]; the replay harness substitutes a
/// clock backed by captured [`crate::replay::ClockSample`] entries.
pub trait ClockSource: Send + Sync {
    /// Returns the current time as a Unix timestamp in milliseconds.
    fn now_ms(&self) -> u64;
}

/// Real-time clock backed by [`std::time::SystemTime`].
///
/// The single place real wall-clock time is read in an instrumented run path.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealClock;

impl ClockSource for RealClock {
    fn now_ms(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// Abstraction over random value generation for deterministic replay.
///
/// Production code is given a [`RealRandom`]; the replay harness substitutes a
/// source backed by captured [`crate::replay::RandomSample`] entries.
pub trait RandomSource: Send + Sync {
    /// Returns 16 bytes of random data (enough to seed a UUID).
    fn random_bytes(&self) -> [u8; 16];
}

/// Real random source backed by the `rand` crate.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealRandom;

impl RandomSource for RealRandom {
    fn random_bytes(&self) -> [u8; 16] {
        use rand::RngCore;
        let mut buf = [0u8; 16];
        rand::rng().fill_bytes(&mut buf);
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_real_clock_returns_recent_timestamp() {
        // GIVEN the real clock
        let clock = RealClock;

        // WHEN reading the time
        let now = clock.now_ms();

        // THEN it is well past 2020-01-01 (1_577_836_800_000 ms)
        assert!(
            now > 1_577_836_800_000,
            "expected a recent timestamp, got {now}"
        );
    }

    #[test]
    fn test_real_random_draws_vary() {
        // GIVEN the real random source
        let random = RealRandom;

        // WHEN drawing twice
        let a = random.random_bytes();
        let b = random.random_bytes();

        // THEN the draws differ (a fixed value would signal a broken source)
        assert_ne!(a, b, "two real random draws should not collide");
    }
}
