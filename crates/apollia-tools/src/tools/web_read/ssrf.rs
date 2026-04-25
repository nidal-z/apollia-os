//! SSRF guard — reject URLs whose host resolves to a private or loopback
//! range before any socket is opened.
//!
//! # Gap documented for v1
//!
//! This guard is a *name-level* check. A malicious domain that resolves to a
//! public IP at check-time and to a private one at connect-time (DNS
//! rebinding) is not mitigated. Closing that gap requires a custom
//! `reqwest::dns::Resolve` implementation and a matching connector policy —
//! scheduled as a follow-up story (ADR-072).

use super::error::WebReadError;

/// Validate that *url* points to a public, routable host.
///
/// # Errors
///
/// Returns [`WebReadError::InvalidUrl`] when the URL is malformed or has no
/// host component. Returns [`WebReadError::PrivateAddress`] for loopback,
/// private (RFC 1918 / unique-local), link-local, multicast, or otherwise
/// internal destinations.
pub(crate) fn assert_public(url: &url::Url) -> Result<(), WebReadError> {
    let host = url
        .host()
        .ok_or_else(|| WebReadError::InvalidUrl("URL has no host component".to_string()))?;

    match host {
        url::Host::Ipv4(ip) => {
            if ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_broadcast()
                || ip.is_unspecified()
            {
                return Err(WebReadError::PrivateAddress(ip.to_string()));
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
                return Err(WebReadError::PrivateAddress(ip.to_string()));
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
                return Err(WebReadError::PrivateAddress(lowered));
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
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv4_private_10() {
        let err = assert_public(&parse("http://10.0.0.1/")).expect_err("rfc1918");
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv4_private_192_168() {
        let err = assert_public(&parse("http://192.168.1.1/")).expect_err("rfc1918");
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv4_link_local() {
        let err = assert_public(&parse("http://169.254.169.254/latest/meta-data/"))
            .expect_err("link-local (AWS metadata)");
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv4_multicast() {
        let err = assert_public(&parse("http://224.0.0.1/")).expect_err("multicast");
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv6_loopback() {
        let err = assert_public(&parse("http://[::1]/")).expect_err("loopback v6");
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv6_unique_local() {
        let err = assert_public(&parse("http://[fd00::1]/")).expect_err("ULA");
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv6_v4_mapped_private() {
        let err =
            assert_public(&parse("http://[::ffff:127.0.0.1]/")).expect_err("v4-mapped loopback");
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_localhost_domain() {
        let err = assert_public(&parse("http://localhost/admin")).expect_err("localhost");
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_dot_local_domain() {
        let err = assert_public(&parse("http://router.local/")).expect_err(".local mDNS");
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_dot_internal_domain() {
        let err = assert_public(&parse("http://wiki.internal/")).expect_err(".internal");
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn accepts_public_ipv4() {
        assert!(assert_public(&parse("https://8.8.8.8/")).is_ok());
    }

    #[test]
    fn accepts_public_ipv6() {
        assert!(assert_public(&parse("https://[2606:4700:4700::1111]/")).is_ok());
    }

    #[test]
    fn accepts_public_domain() {
        assert!(assert_public(&parse("https://example.com/article")).is_ok());
        assert!(assert_public(&parse("https://www.rust-lang.org/")).is_ok());
    }

    #[test]
    fn accepts_punycode_domain() {
        // Internationalised domain → passes; no privacy risk.
        assert!(assert_public(&parse("https://xn--bcher-kva.example/")).is_ok());
    }

    #[test]
    fn rejects_missing_host() {
        // `url::Url::parse("file:///tmp/foo")` returns a URL with no host.
        let parsed = url::Url::parse("file:///etc/passwd").expect("valid url");
        let err = assert_public(&parsed).expect_err("no host");
        assert!(matches!(err, WebReadError::InvalidUrl(_)));
    }
}
