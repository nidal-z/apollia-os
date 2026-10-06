//! Where a call to the compatible server comes from, the address a client of
//! the network copies, and the rule that admits a call to an agent.
//!
//! Every connection is classified once, when it is accepted, from the address
//! of the peer: the loopback interface is the machine itself, any other address
//! is a device of the local network. The class travels with each request of the
//! connection as a [`CallOrigin`] extension, beside axum's `ConnectInfo`.
//!
//! Models answer the local network without a token when `[server]
//! require_auth` is off. An agent or a workflow does not: called from the
//! network, it needs a token of the server whatever `require_auth` says, so no
//! data the user's agents reach leaves the machine without a token the user
//! created. [`admits_agent_call`] is that rule.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

use axum::http::HeaderMap;
use axum::response::Response;
use serde::{Deserialize, Serialize};

use super::auth::{presented_token, unauthorized_with};
use super::tokens::ServerTokensHandle;

/// The refusal a device of the local network reads when it calls an agent or
/// a workflow without a token in use.
pub const AGENT_TOKEN_REQUIRED: &str =
    "A token is required to call an agent or a workflow from the local network. \
     Create one in Apollia and send it in the Authorization header as a Bearer \
     token, or in the x-api-key header.";

/// Where a call comes from, read from the peer address of its connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallOrigin {
    /// The machine itself, through the loopback interface.
    Loopback,
    /// Another device, through the local network.
    LocalNetwork,
}

impl CallOrigin {
    /// Classifies the peer of a connection. An IPv4 address carried in an
    /// IPv6 one (`::ffff:127.0.0.1`) is read as the IPv4 address it carries.
    pub fn of_peer(peer: SocketAddr) -> Self {
        let ip = match peer.ip() {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
            v4 => v4,
        };
        if ip.is_loopback() {
            Self::Loopback
        } else {
            Self::LocalNetwork
        }
    }
}

/// Whether a call to an agent or a workflow is admitted: always from the
/// machine, and from the local network only with a token in use.
pub fn admits_agent_call(origin: CallOrigin, token_in_use: bool) -> bool {
    match origin {
        CallOrigin::Loopback => true,
        CallOrigin::LocalNetwork => token_in_use,
    }
}

/// Applies [`admits_agent_call`] to a request: the token it presents, if any,
/// is checked against the tokens in use, and a refused call gets the 401 of
/// the endpoint `path` names, carrying [`AGENT_TOKEN_REQUIRED`].
///
/// # Errors
///
/// The 401 response to send back when the call is refused.
pub async fn admit_agent_call(
    origin: CallOrigin,
    path: &str,
    headers: &HeaderMap,
    tokens: &ServerTokensHandle,
) -> Result<(), Response> {
    let token_in_use = match (origin, presented_token(headers)) {
        (CallOrigin::Loopback, _) | (_, None) => false,
        (CallOrigin::LocalNetwork, Some(token)) => tokens.verify(token).await,
    };
    if admits_agent_call(origin, token_in_use) {
        return Ok(());
    }
    tracing::event!(
        tracing::Level::WARN,
        path = %path,
        "compat_server.agent_call.refused"
    );
    Err(unauthorized_with(path, AGENT_TOKEN_REQUIRED))
}

/// The address a client copies to reach the server: the loopback address when
/// it listens on the machine alone, the IPv4 address of the machine on the
/// local network when it listens on every interface.
///
/// The address of the network is the one the system would send from towards a
/// documentation address (RFC 5737), read from a UDP socket that is connected
/// but never sends: no packet leaves the machine. A machine with no network
/// route gets the loopback address.
pub fn reachable_address(serve_on_lan: bool, port: u16) -> String {
    let host = if serve_on_lan {
        local_network_ipv4().unwrap_or(Ipv4Addr::LOCALHOST)
    } else {
        Ipv4Addr::LOCALHOST
    };
    format!("http://{host}:{port}")
}

fn local_network_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv6Addr;

    use super::*;

    #[test]
    fn test_the_peer_address_gives_the_origin() {
        // GIVEN a loopback peer in IPv4, in IPv6, carried in IPv6, and a peer
        // of the local network
        let v4 = SocketAddr::from((Ipv4Addr::LOCALHOST, 50_000));
        let v6 = SocketAddr::from((Ipv6Addr::LOCALHOST, 50_000));
        let mapped = SocketAddr::from((Ipv4Addr::LOCALHOST.to_ipv6_mapped(), 50_000));
        let lan = SocketAddr::from((Ipv4Addr::new(192, 168, 1, 20), 50_000));

        // WHEN each is classified
        let origins = [v4, v6, mapped, lan].map(CallOrigin::of_peer);

        // THEN the first three are the machine, the last the local network
        assert_eq!(
            origins,
            [
                CallOrigin::Loopback,
                CallOrigin::Loopback,
                CallOrigin::Loopback,
                CallOrigin::LocalNetwork
            ]
        );
    }

    #[test]
    fn test_an_agent_call_from_the_network_needs_a_token() {
        // GIVEN the four cases of origin and token
        // WHEN the admission rule reads each
        // THEN only a call from the network without a token is refused
        assert!(admits_agent_call(CallOrigin::Loopback, false));
        assert!(admits_agent_call(CallOrigin::Loopback, true));
        assert!(admits_agent_call(CallOrigin::LocalNetwork, true));
        assert!(!admits_agent_call(CallOrigin::LocalNetwork, false));
    }

    #[test]
    fn test_the_refusal_names_the_token_and_the_network() {
        // GIVEN the refusal of an agent call
        // WHEN it is read
        // THEN it says a token is required from the local network
        assert!(AGENT_TOKEN_REQUIRED.starts_with(
            "A token is required to call an agent or a workflow from the local network."
        ));
    }

    #[test]
    fn test_the_reachable_address_is_loopback_or_a_network_ipv4() {
        // GIVEN the server on the machine alone, and on the local network
        // WHEN the address to copy is computed for each
        let loopback = reachable_address(false, 7772);
        let lan = reachable_address(true, 7772);

        // THEN the first is the loopback address, the second an IPv4 address
        // on the same port, never the unspecified one
        assert_eq!(loopback, "http://127.0.0.1:7772");
        let host = lan
            .strip_prefix("http://")
            .and_then(|rest| rest.strip_suffix(":7772"))
            .unwrap();
        let ip: Ipv4Addr = host.parse().unwrap();
        assert!(!ip.is_unspecified(), "{lan}");
    }
}
