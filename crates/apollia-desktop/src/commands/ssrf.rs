//! SSRF guard duplicated from `apollia-tools::web_read::ssrf` (US-SP42-035).
//!
//! We intentionally duplicate the helper (it is `pub(crate)` in apollia-tools)
//! instead of widening the public surface of that crate.  Keep the two
//! copies in sync if SSRF policy changes — see ADR-072 for the policy.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SsrfError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("private or loopback address refused: {0}")]
    PrivateAddress(String),
}

/// Reject URLs whose host is loopback, private, link-local, multicast,
/// unique-local, `*.localhost`, `*.local`, `*.internal`, or `*.localdomain`.
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
            let is_unique_local = (segments[0] & 0xfe00) == 0xfc00;
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
    fn rejects_loopback() {
        assert!(assert_public(&parse("http://127.0.0.1/")).is_err());
        assert!(assert_public(&parse("http://localhost/")).is_err());
    }

    #[test]
    fn rejects_private() {
        assert!(assert_public(&parse("http://10.0.0.1/")).is_err());
        assert!(assert_public(&parse("http://192.168.1.1/")).is_err());
        assert!(assert_public(&parse("http://169.254.169.254/")).is_err());
    }

    #[test]
    fn accepts_public() {
        assert!(assert_public(&parse("https://example.com/")).is_ok());
        assert!(assert_public(&parse("https://8.8.8.8/")).is_ok());
    }
}
