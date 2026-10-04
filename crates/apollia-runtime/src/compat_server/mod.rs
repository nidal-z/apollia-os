//! The compatible server: a second listener of the runtime, separate from the
//! native API.
//!
//! The native API (`/api/v1/...`, `[api].port` and the Unix socket) is the
//! runtime's own surface; the desktop and the CLI drive it. The compatible
//! server is the surface other applications of the machine call as they would
//! call OpenAI or Anthropic: its paths and bodies follow those formats
//! (`/v1/...`, `snake_case`), and it answers on its own port, 7772 by default,
//! on the loopback interface, or on every interface when `[server]
//! serve_on_lan` opens it to the local network. Starting, stopping or
//! restarting it never touches the native API, which stays on the loopback
//! interface in every case, so the desktop keeps its runtime whatever happens
//! here.
//!
//! Every request carries its [`CallOrigin`], the machine or the local network.
//! Models answer the network with or without a token, as `require_auth` says;
//! a call to an agent or a workflow from the network needs a token in any case
//! ([`admits_agent_call`]).
//!
//! The server is off until someone turns it on. Its switch and its port are
//! the `[server] enabled` and `[server] port` settings of `apollia.toml`,
//! resolved through [`apollia_core::config::resolve_setting`]; starting and
//! stopping it writes `enabled`, so a runtime that restarts opens the listener
//! again if it was open.
//!
//! When `[server] require_auth` is on, every request must present a token of
//! the server (`apo_...`), created, listed and revoked through the native API
//! and kept by [`ServerTokensHandle`] as a SHA-256 in `system.db`; the
//! internal token of the desktop never opens this server.
//!
//! The models it serves are the local models the user loaded, relayed to the
//! engine that runs them (`llama-server` or `vllm-mlx`) with their `model`
//! field set to what that engine expects; see [`LocalModels`]. A model of a
//! remote backend is never relayed.
//!
//! The agents and workflows the user marked exposed are served as models too,
//! named `apollia/<id>` ([`ExposedAgents`]). A chat completion on one runs the
//! agent as a batch of one item, or the workflow through its engine, and
//! answers as a model would: the steps in `reasoning_content`, the answer in
//! `content`, an `approval_pending` error when the agent waits for a decision.
//!
//! Every request it answers, and every line the local `llama-server` prints,
//! enters a bounded ring of recent lines that the native API streams (see
//! [`ServerLogsHandle`]); clearing it never touches the audit journal.
//!
//! [`CompatServerHandle`] is the actor that owns the listener. Every path its
//! router mounts is listed on the public page of compatible endpoints, which
//! `scripts/check_compat_routes.py` holds.

mod actor;
mod agent_call;
mod agent_reply;
mod agent_run;
mod agents;
mod auth;
mod errors;
mod logs;
mod models;
mod origin;
mod relay;
mod router;
mod tokens;

#[cfg(test)]
mod agents_tests;
#[cfg(test)]
mod auth_tests;
#[cfg(test)]
mod lan_tests;
#[cfg(test)]
mod logs_tests;
#[cfg(test)]
mod relay_tests;
#[cfg(test)]
pub(crate) mod tests;

pub use actor::{CompatServerHandle, CompatServerStatus};
pub use agents::ExposedAgents;
pub use logs::{
    ServerLogLevel, ServerLogLine, ServerLogSource, ServerLogSubscription, ServerLogsError,
    ServerLogsHandle, DEFAULT_CAPACITY as SERVER_LOGS_CAPACITY,
};
pub use models::LocalModels;
pub use origin::{
    admit_agent_call, admits_agent_call, reachable_address, CallOrigin, AGENT_TOKEN_REQUIRED,
};
pub use tokens::{CreatedServerToken, ServerTokensError, ServerTokensHandle, SERVER_TOKEN_PREFIX};

use apollia_core::config::SettingError;

/// Why the compatible server could not be started, stopped or read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CompatServerError {
    /// Another program already listens on the port.
    #[error("port {port} is already in use by another program")]
    PortInUse {
        /// The port asked for.
        port: u16,
    },
    /// The listener could not be opened for another reason.
    #[error("failed to listen on port {port}")]
    Bind {
        /// The port asked for.
        port: u16,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The `[server]` settings could not be read or written.
    #[error("failed to read or write the server settings")]
    Settings {
        /// The underlying error.
        #[from]
        source: SettingError,
    },
    /// The client that reaches the local engines could not be built.
    #[error("failed to build the client of the local engines")]
    Client(#[source] reqwest::Error),
    /// The tokens of the server could not be counted.
    #[error(transparent)]
    Tokens(#[from] ServerTokensError),
    /// The actor that owns the listener has stopped.
    #[error("the compatible server has stopped with the runtime")]
    ActorGone,
}
