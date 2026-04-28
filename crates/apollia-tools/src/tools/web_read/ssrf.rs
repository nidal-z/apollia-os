//! `web_read`-flavoured SSRF guard.
//!
//! Thin wrapper over the workspace-level [`crate::ssrf::assert_public`] that
//! adapts the generic [`SsrfError`] into the tool's [`WebReadError`] taxonomy.
//!
//! [`SsrfError`]: crate::ssrf::SsrfError

use crate::ssrf::{assert_public as shared_assert_public, SsrfError};

use super::error::WebReadError;

impl From<SsrfError> for WebReadError {
    fn from(err: SsrfError) -> Self {
        match err {
            SsrfError::InvalidUrl(msg) => WebReadError::InvalidUrl(msg),
            SsrfError::PrivateAddress(host) => WebReadError::PrivateAddress(host),
        }
    }
}

/// Validate that *url* points to a public, routable host.
///
/// See [`crate::ssrf::assert_public`] for the full policy.
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
        assert!(assert_public(&parse("https://xn--bcher-kva.example/")).is_ok());
    }

    #[test]
    fn rejects_missing_host() {
        let parsed = url::Url::parse("file:///etc/passwd").expect("valid url");
        let err = assert_public(&parsed).expect_err("no host");
        assert!(matches!(err, WebReadError::InvalidUrl(_)));
    }
}
