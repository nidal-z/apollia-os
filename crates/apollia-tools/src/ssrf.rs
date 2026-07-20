//! Shared SSRF guard: reject URLs whose host resolves to a private,
//! loopback, or otherwise internal range before any socket is opened.
//!
//! Used by `http_fetch`, `web_read`, and the webhook notification channel.
//!
//! Redirects are covered too: [`public_redirect_policy`] re-runs
//! [`assert_public`] on the target of every hop, so a public endpoint cannot
//! `302` the client onto a private destination.
//!
//! # Gap documented for v1
//!
//! This guard is a *name-level* check. A malicious domain that resolves to a
//! public IP at check-time and to a private one at connect-time (DNS
//! rebinding) is not mitigated, and neither is a redirect whose host rebinds
//! between the policy check and the socket connect. Closing that gap requires a
//! custom `reqwest::dns::Resolve` implementation that pins the resolved IP for
//! the connection, scheduled as a follow-up.

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

/// Redirect-chain cap applied when a call site does not pick its own. Mirrors
/// reqwest's own default of 10 hops.
pub const DEFAULT_MAX_REDIRECTS: usize = 10;

/// Outcome of evaluating a single redirect hop against the SSRF policy.
///
/// Pure and constructible in tests: reqwest's `Attempt` has no public
/// constructor, so the decision logic lives here and is unit-tested directly,
/// while [`public_redirect_policy`] only translates the decision into a reqwest
/// action.
#[derive(Debug)]
pub enum RedirectDecision {
    /// Hop target is public and within the hop budget: follow it.
    Follow,
    /// Hop target resolves to a private / internal destination: refuse.
    Block(SsrfError),
    /// The redirect chain reached its cap.
    TooMany,
}

/// Decide what to do with one redirect hop.
///
/// `hops_so_far` is the number of redirects already followed (reqwest exposes
/// this as `attempt.previous().len()`). A target is followed only when the
/// budget is not exhausted *and* [`assert_public`] accepts it.
pub fn evaluate_redirect(
    target: &url::Url,
    hops_so_far: usize,
    max_redirects: usize,
) -> RedirectDecision {
    if hops_so_far >= max_redirects {
        return RedirectDecision::TooMany;
    }
    match assert_public(target) {
        Ok(()) => RedirectDecision::Follow,
        Err(err) => RedirectDecision::Block(err),
    }
}

/// Reason a redirect hop was refused, surfaced through reqwest's redirect
/// machinery so the transport error carries an explanatory message.
#[cfg(feature = "http")]
#[derive(Debug, Error)]
pub enum RedirectBlocked {
    /// A hop resolved to a private / internal destination.
    #[error("ssrf blocked on redirect: {0}")]
    Ssrf(#[source] SsrfError),

    /// The redirect chain exceeded its configured cap.
    #[error("too many redirects (limit {0})")]
    TooMany(usize),
}

/// Build a redirect policy that re-runs [`assert_public`] on every hop and caps
/// the chain at `max_redirects`.
///
/// The initial-URL check performed by call sites is not enough on its own:
/// reqwest follows 3xx `Location` headers, which an attacker controls, so each
/// hop is re-validated here. A refused hop aborts the chain *before* the socket
/// to the blocked host is opened; the resulting error surfaces through
/// `send().await` as a transport failure carrying the [`RedirectBlocked`]
/// message.
#[cfg(feature = "http")]
pub fn public_redirect_policy(max_redirects: usize) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        match evaluate_redirect(attempt.url(), attempt.previous().len(), max_redirects) {
            RedirectDecision::Follow => attempt.follow(),
            RedirectDecision::Block(err) => attempt.error(RedirectBlocked::Ssrf(err)),
            RedirectDecision::TooMany => attempt.error(RedirectBlocked::TooMany(max_redirects)),
        }
    })
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

    #[test]
    fn evaluate_redirect_follows_public_target_within_budget() {
        // GIVEN a public redirect target and an unexhausted hop budget
        let target = parse("https://example.com/next");
        // WHEN the hop is evaluated
        let decision = evaluate_redirect(&target, 2, 5);
        // THEN it is followed
        assert!(matches!(decision, RedirectDecision::Follow));
    }

    #[test]
    fn evaluate_redirect_blocks_private_targets() {
        // GIVEN redirect targets pointing at internal destinations
        for host in [
            "http://169.254.169.254/latest/meta-data/",
            "http://127.0.0.1/admin",
            "http://10.0.0.1/",
        ] {
            let target = parse(host);
            // WHEN the hop is evaluated with budget remaining
            let decision = evaluate_redirect(&target, 0, 5);
            // THEN it is refused
            assert!(
                matches!(
                    decision,
                    RedirectDecision::Block(SsrfError::PrivateAddress(_))
                ),
                "expected Block for {host}, got {decision:?}"
            );
        }
    }

    #[test]
    fn evaluate_redirect_stops_when_budget_exhausted() {
        // GIVEN a public target but a hop count that has reached the cap
        let target = parse("https://example.com/loop");
        // WHEN the hop is evaluated
        let decision = evaluate_redirect(&target, 5, 5);
        // THEN the chain is stopped before the public check even matters
        assert!(matches!(decision, RedirectDecision::TooMany));
    }
}
