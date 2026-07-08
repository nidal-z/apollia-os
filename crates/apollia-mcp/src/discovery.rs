//! mDNS-based discovery and announcement of MCP servers on the local network.
//!
//! MCP servers advertise themselves under the `_apollia-mcp._tcp.local.`
//! service type. Discovery is disabled by default (`mdns_discovery = false`
//! in `mcp.toml`) and must be opted into explicitly; see [`McpConfig`].
//!
//! [`McpConfig`]: crate::config::McpConfig

use std::collections::HashMap;
use std::time::{Duration, Instant};

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};
use thiserror::Error;

/// mDNS service type under which Apollia MCP servers are registered.
pub const SERVICE_TYPE: &str = "_apollia-mcp._tcp.local.";

/// How long [`discover_mcp_servers`] scans before returning results.
const SCAN_TIMEOUT: Duration = Duration::from_secs(3);

/// Errors produced by mDNS discovery and announcement operations.
#[derive(Debug, Error)]
pub enum DiscoveryError {
    /// The mDNS daemon could not be created, or service registration failed.
    #[error("mDNS daemon error: {0}")]
    Daemon(String),

    /// A background blocking task panicked unexpectedly.
    #[error("background task panicked: {0}")]
    TaskPanic(String),
}

impl From<mdns_sd::Error> for DiscoveryError {
    fn from(err: mdns_sd::Error) -> Self {
        DiscoveryError::Daemon(err.to_string())
    }
}

/// An MCP server discovered via mDNS on the local network.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiscoveredServer {
    /// Human-readable service instance name (the mDNS full name, without trailing dot).
    pub name: String,

    /// IP addresses at which the server is reachable.
    pub addresses: Vec<String>,

    /// TCP port the server listens on.
    pub port: u16,

    /// Tools advertised in the `tools` TXT record (comma-separated in the wire format).
    pub tools: Vec<String>,
}

/// Announces this process as an MCP server on the local network via mDNS.
///
/// Registers a `_apollia-mcp._tcp.local.` service with the given `name`,
/// `port`, and `tools` list. The tools are stored in the `tools` TXT property
/// as a comma-separated string. A `version` property is also included.
///
/// The returned [`ServiceDaemon`] must be kept alive for the announcement to
/// remain visible; dropping it unregisters the service.
pub async fn announce_mcp_server(
    name: &str,
    port: u16,
    tools: &[String],
) -> Result<ServiceDaemon, DiscoveryError> {
    let instance_name = name.to_string();
    let tools_value = tools.join(",");

    tokio::task::spawn_blocking(move || {
        let daemon = ServiceDaemon::new().map_err(DiscoveryError::from)?;

        let host_name = build_host_name();

        let mut properties: HashMap<String, String> = HashMap::new();
        properties.insert("tools".to_string(), tools_value);
        properties.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());

        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            &instance_name,
            &host_name,
            (),
            port,
            properties,
        )
        .map_err(DiscoveryError::from)?;

        daemon
            .register(service_info)
            .map_err(DiscoveryError::from)?;

        tracing::info!(
            service = %instance_name,
            port = port,
            "MCP server announced via mDNS"
        );

        Ok::<_, DiscoveryError>(daemon)
    })
    .await
    .map_err(|e| DiscoveryError::TaskPanic(e.to_string()))?
}

/// Scans the local network for MCP servers advertised via mDNS.
///
/// Browses `_apollia-mcp._tcp.local.` for up to 3 seconds and returns every
/// [`DiscoveredServer`] that was fully resolved before the deadline. Returns
/// an empty `Vec` when no servers are found; this is not an error.
pub async fn discover_mcp_servers() -> Result<Vec<DiscoveredServer>, DiscoveryError> {
    tokio::task::spawn_blocking(|| {
        let daemon = ServiceDaemon::new().map_err(DiscoveryError::from)?;
        let receiver = daemon.browse(SERVICE_TYPE).map_err(DiscoveryError::from)?;

        let deadline = Instant::now() + SCAN_TIMEOUT;
        let mut discovered: Vec<DiscoveredServer> = Vec::new();

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match receiver.recv_timeout(remaining) {
                Ok(ServiceEvent::ServiceResolved(info)) => {
                    let server = resolved_server_from_info(&info);
                    tracing::debug!(
                        name = %server.name,
                        port = server.port,
                        "MCP server resolved via mDNS"
                    );
                    discovered.push(server);
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }

        daemon.stop_browse(SERVICE_TYPE).ok();

        Ok::<_, DiscoveryError>(discovered)
    })
    .await
    .map_err(|e| DiscoveryError::TaskPanic(e.to_string()))?
}

/// Builds an mDNS-compatible fully-qualified host name from the system hostname.
fn build_host_name() -> String {
    match hostname::get() {
        Ok(raw) => {
            let s = raw.to_string_lossy();
            if s.contains('.') {
                format!("{s}.")
            } else {
                format!("{s}.local.")
            }
        }
        Err(_) => "apollia-host.local.".to_string(),
    }
}

/// Converts a resolved mDNS [`ResolvedService`] into a [`DiscoveredServer`].
fn resolved_server_from_info(info: &ResolvedService) -> DiscoveredServer {
    let addresses = info.get_addresses().iter().map(|a| a.to_string()).collect();

    let tools = info
        .get_property_val_str("tools")
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();

    DiscoveredServer {
        name: info.get_fullname().trim_end_matches('.').to_string(),
        addresses,
        port: info.get_port(),
        tools,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discovered_server_serialization() {
        // GIVEN a DiscoveredServer with known fields
        let server = DiscoveredServer {
            name: "test-server".into(),
            addresses: vec!["192.168.1.10".into()],
            port: 8080,
            tools: vec!["search".into(), "query".into()],
        };
        // WHEN serialized to JSON
        let json = serde_json::to_string(&server).expect("serialization must succeed");
        // THEN the output contains the server name
        assert!(json.contains("test-server"));
        assert!(json.contains("8080"));
    }

    #[test]
    fn test_service_type_format() {
        // GIVEN the SERVICE_TYPE constant
        // THEN it matches the expected mDNS service type
        assert_eq!(SERVICE_TYPE, "_apollia-mcp._tcp.local.");
    }

    #[test]
    fn test_build_host_name_returns_fqdn() {
        // GIVEN the current system hostname
        let name = build_host_name();
        // THEN it ends with a dot (FQDN format required by mDNS)
        assert!(name.ends_with('.'), "FQDN must end with '.', got: {name}");
    }

    #[test]
    fn test_resolved_server_empty_tools() {
        // GIVEN a DiscoveredServer with no tools
        let server = DiscoveredServer {
            name: "empty".into(),
            addresses: vec![],
            port: 9090,
            tools: vec![],
        };
        // THEN tools field is empty, not an error
        assert!(server.tools.is_empty());
    }
}
