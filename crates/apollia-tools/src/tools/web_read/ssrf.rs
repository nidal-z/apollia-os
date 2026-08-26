//! `web_read`-flavoured SSRF guard.
//!
//! Thin wrapper over [`apollia_core::net::assert_public`] that adapts the
//! generic [`SsrfError`] into the tool's [`WebReadError`] taxonomy.
//!
//! [`SsrfError`]: apollia_core::net::SsrfError

use apollia_core::net::{assert_public as shared_assert_public, SsrfError};

use super::error::WebReadError;

impl From<SsrfError> for WebReadError {
    fn from(err: SsrfError) -> Self {
        match err {
            SsrfError::InvalidUrl(msg) => WebReadError::InvalidUrl(msg),
            SsrfError::PrivateAddress(host) => WebReadError::PrivateAddress(host),
            other => WebReadError::InvalidUrl(other.to_string()),
        }
    }
}

/// Validate that *url* points to a public, routable host.
///
/// See [`apollia_core::net::assert_public`] for the full policy.
pub(crate) fn assert_public(url: &url::Url) -> Result<(), WebReadError> {
    shared_assert_public(url).map_err(WebReadError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(u: &str) -> url::Url {
        url::Url::parse(u).expect("valid url")
    }

    #[test]
    fn rejects_ipv4_loopback() {
        // GIVEN a URL pointing at the IPv4 loopback
        // WHEN the guard is asked whether the target is public
        let err = assert_public(&parse("http://127.0.0.1/")).expect_err("loopback");
        // THEN the request is refused as a private address
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv4_private_10() {
        // GIVEN a URL pointing into the 10.0.0.0/8 private range
        // WHEN the guard is asked whether the target is public
        let err = assert_public(&parse("http://10.0.0.1/")).expect_err("rfc1918");
        // THEN the request is refused as a private address
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv4_private_192_168() {
        // GIVEN a URL pointing into the 192.168.0.0/16 private range
        // WHEN the guard is asked whether the target is public
        let err = assert_public(&parse("http://192.168.1.1/")).expect_err("rfc1918");
        // THEN the request is refused as a private address
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv4_link_local() {
        // GIVEN a URL pointing at the link-local address cloud metadata is served on
        // WHEN the guard is asked whether the target is public
        let err = assert_public(&parse("http://169.254.169.254/latest/meta-data/"))
            .expect_err("link-local (AWS metadata)");
        // THEN the request is refused as a private address
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv4_multicast() {
        // GIVEN a URL pointing at an IPv4 multicast address
        // WHEN the guard is asked whether the target is public
        let err = assert_public(&parse("http://224.0.0.1/")).expect_err("multicast");
        // THEN the request is refused as a private address
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv6_loopback() {
        // GIVEN a URL pointing at the IPv6 loopback
        // WHEN the guard is asked whether the target is public
        let err = assert_public(&parse("http://[::1]/")).expect_err("loopback v6");
        // THEN the request is refused as a private address
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv6_unique_local() {
        // GIVEN a URL pointing into the IPv6 unique-local range
        // WHEN the guard is asked whether the target is public
        let err = assert_public(&parse("http://[fd00::1]/")).expect_err("ULA");
        // THEN the request is refused as a private address
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv6_v4_mapped_private() {
        // GIVEN a URL pointing at the loopback through an IPv4-mapped IPv6 address
        // WHEN the guard is asked whether the target is public
        let err =
            assert_public(&parse("http://[::ffff:127.0.0.1]/")).expect_err("v4-mapped loopback");
        // THEN the request is refused as a private address
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_localhost_domain() {
        // GIVEN a URL whose host is the localhost name
        // WHEN the guard is asked whether the target is public
        let err = assert_public(&parse("http://localhost/admin")).expect_err("localhost");
        // THEN the request is refused as a private address
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_dot_local_domain() {
        // GIVEN a URL whose host ends in the mDNS `.local` suffix
        // WHEN the guard is asked whether the target is public
        let err = assert_public(&parse("http://router.local/")).expect_err(".local mDNS");
        // THEN the request is refused as a private address
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_dot_internal_domain() {
        // GIVEN a URL whose host ends in the `.internal` suffix
        // WHEN the guard is asked whether the target is public
        let err = assert_public(&parse("http://wiki.internal/")).expect_err(".internal");
        // THEN the request is refused as a private address
        assert!(matches!(err, WebReadError::PrivateAddress(_)));
    }

    #[test]
    fn accepts_public_ipv4() {
        // GIVEN a URL pointing at a public IPv4 address
        // WHEN the guard is asked whether the target is public
        // THEN the request is allowed through
        assert!(assert_public(&parse("https://8.8.8.8/")).is_ok());
    }

    #[test]
    fn accepts_public_ipv6() {
        // GIVEN a URL pointing at a public IPv6 address
        // WHEN the guard is asked whether the target is public
        // THEN the request is allowed through
        assert!(assert_public(&parse("https://[2606:4700:4700::1111]/")).is_ok());
    }

    #[test]
    fn accepts_public_domain() {
        // GIVEN two URLs on public domain names
        // WHEN the guard is asked whether the target is public
        // THEN the request is allowed through
        assert!(assert_public(&parse("https://example.com/article")).is_ok());
        assert!(assert_public(&parse("https://www.rust-lang.org/")).is_ok());
    }

    #[test]
    fn accepts_punycode_domain() {
        // GIVEN a URL on a punycode domain name
        // WHEN the guard is asked whether the target is public
        // THEN the request is allowed through
        assert!(assert_public(&parse("https://xn--bcher-kva.example/")).is_ok());
    }

    #[test]
    fn rejects_missing_host() {
        // GIVEN a URL carrying no host at all
        let parsed = url::Url::parse("file:///etc/passwd").expect("valid url");
        // WHEN the guard is asked whether the target is public
        let err = assert_public(&parsed).expect_err("no host");
        // THEN the request is refused as an invalid URL, not as a private address
        assert!(matches!(err, WebReadError::InvalidUrl(_)));
    }
}
