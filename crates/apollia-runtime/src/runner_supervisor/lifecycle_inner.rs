//! Internal handle shared between `lifecycle` and `proxy`.
//!
//! Avoids circular coupling: `proxy` needs access to the `RunnerInner`
//! (process + HTTP client) managed by `lifecycle`, but we cannot make
//! `lifecycle::RunnerInner` `pub` without breaking encapsulation (the `Child`
//! is not `Clone`).

use super::client::RunnerClient;

/// Handle separate from the private `RunnerInner` of `lifecycle.rs`. Re-exposes
/// only what the proxy needs: the cloneable HTTP client and the port.
pub(super) struct RunnerInnerHandle {
    pub(super) client: RunnerClient,
    #[allow(dead_code)]
    pub(super) port: u16,
}
