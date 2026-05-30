//! Shared SSRF guard: reject URLs whose host resolves to a private,
//! loopback, or otherwise internal range before any socket is opened.
//!
//! Used by `http_fetch`, `web_read`, and the webhook notification channel.
//!
//! # Gap documented for v1
//!
//! This guard is a *name-level* check. A malicious domain that resolves to a
//! public IP at check-time and to a private one at connect-time (DNS
//! rebinding) is not mitigated. Closing that gap requires a custom
//! `reqwest::dns::Resolve` implementation and a matching connector policy,
//! scheduled as a follow-up.

use thiserror::Error;

/// SSRF-policy violation surfaced by [`assert_public`].
///
/// Carries enough context for callers to wrap into their own error taxonomy
/// while preserving the rejected host string for audit logging.
#[derive(Debug, Error)]
pub enum SsrfError {
    /// URL is missing a host component (e.g. `file:///etc/passwd`).
    #[error("invalid url: {0}")]
    InvalidUrl(String),

    /// Host resolves to a loopback / private / link-local / multicast /
    /// unique-local / IPv4-mapped-private / `.local` / `.internal` /
    /// `.localdomain` / `localhost` destination.
    #[error("private address: {0}")]
    PrivateAddress(String),
}

/// Validate that *url* points to a public, routable host.
///
/// # Errors
///
/// Returns [`SsrfError::InvalidUrl`] when the URL has no host component.
/// Returns [`SsrfError::PrivateAddress`] for loopback, RFC 1918 private,
/// link-local, multicast, unique-local IPv6, IPv4-mapped private, or
/// internal-domain destinations (`localhost`, `*.local`, `*.internal`,
/// `*.localdomain`).
pub fn assert_public(url: &url::Url) -> Result<(), SsrfError> {
    let host = url
        .host()
        .ok_or_else(|| SsrfError::InvalidUrl("URL has no host component".to_string()))?;

    match host {
        url::Host::Ipv4(ip) => {
            if ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_unspecified()
            {
                return Err(SsrfError::PrivateAddress(ip.to_string()));
            }
        }
        url::Host::Ipv6(ip) => {
            let segments = ip.segments();
            // Unique-local addresses: fc00::/7 (high byte 0xfc or 0xfd).
            let is_unique_local = (segments[0] & 0xfe00) == 0xfc00;
            // IPv4-mapped embedding: if the IPv4 half is private/loopback,
            // block as well (::ffff:127.0.0.1 etc.).
            let is_v4_mapped_private = segments[0] == 0
                && segments[1] == 0
                && segments[2] == 0
                && segments[3] == 0
                && segments[4] == 0
                && segments[5] == 0xffff
                && {
                    let lo = segments[6];
                    let hi = segments[7];
                    let v4 = std::net::Ipv4Addr::new(
                        (lo >> 8) as u8,
                        (lo & 0xff) as u8,
                        (hi >> 8) as u8,
                        (hi & 0xff) as u8,
                    );
                    v4.is_loopback() || v4.is_private() || v4.is_link_local()
                };

            if ip.is_loopback()
                || ip.is_multicast()
                || ip.is_unspecified()
                || is_unique_local
                || is_v4_mapped_private
            {
                return Err(SsrfError::PrivateAddress(ip.to_string()));
            }
        }
        url::Host::Domain(name) => {
            let lowered = name.to_ascii_lowercase();
            if lowered == "localhost"
                || lowered.ends_with(".localhost")
                || lowered.ends_with(".local")
                || lowered.ends_with(".internal")
                || lowered.ends_with(".localdomain")
            {
                return Err(SsrfError::PrivateAddress(lowered));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(u: &str) -> url::Url {
        url::Url::parse(u).expect("valid url")
    }

    #[test]
    fn rejects_ipv4_loopback() {
        let err = assert_public(&parse("http://127.0.0.1/")).expect_err("loopback");
        assert!(matches!(err, SsrfError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv4_private_10() {
        let err = assert_public(&parse("http://10.0.0.1/")).expect_err("rfc1918");
        assert!(matches!(err, SsrfError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv4_link_local_metadata() {
        let err = assert_public(&parse("http://169.254.169.254/latest/meta-data/"))
            .expect_err("AWS metadata");
        assert!(matches!(err, SsrfError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv6_loopback() {
        let err = assert_public(&parse("http://[::1]/")).expect_err("v6 loopback");
        assert!(matches!(err, SsrfError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_localhost_domain() {
        let err = assert_public(&parse("http://localhost/admin")).expect_err("localhost");
        assert!(matches!(err, SsrfError::PrivateAddress(_)));
    }

    #[test]
    fn accepts_public_ipv4() {
        assert!(assert_public(&parse("https://8.8.8.8/")).is_ok());
    }

    #[test]
    fn accepts_public_domain() {
        assert!(assert_public(&parse("https://example.com/article")).is_ok());
    }

    #[test]
    fn rejects_missing_host() {
        let parsed = url::Url::parse("file:///etc/passwd").expect("valid url");
        let err = assert_public(&parsed).expect_err("no host");
        assert!(matches!(err, SsrfError::InvalidUrl(_)));
    }
}
