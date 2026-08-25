//! The one HTTP surface the workspace is allowed to reach the network through.
//!
//! Two rules used to live in three copies each, and both were missing wherever
//! the copy had not been made: the SSRF policy (refuse a host that resolves to
//! a private, loopback or otherwise internal range, on the first URL *and* on
//! every redirect hop) and the body cap (stop reading once a response exceeds
//! its budget, instead of buffering whatever the peer sends). This module holds
//! both, and `scripts/check_http_clients.py` refuses a `reqwest::Client` built
//! anywhere else or a response body consumed outside [`read_capped_bytes`],
//! [`read_capped_text`] and [`read_capped_json`].
//!
//! # Gap documented for v1
//!
//! [`assert_public`] is a *name-level* check. A domain that resolves to a public
//! address when the policy runs and to a private one when the socket connects
//! (DNS rebinding) is not mitigated, on the first URL or on a hop. Closing that
//! gap needs a `reqwest::dns::Resolve` implementation that pins the resolved
//! address for the connection.

use thiserror::Error;

/// SSRF-policy violation surfaced by [`assert_public`].
///
/// Carries the rejected host so callers can wrap it into their own taxonomy
/// without losing what to write in the audit trail.
#[derive(Debug, Error)]
#[non_exhaustive]
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
            // IPv4-mapped embedding: if the IPv4 half is private or loopback,
            // block as well (::ffff:127.0.0.1 and friends).
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

/// Validate a URL held as a string.
///
/// Same policy as [`assert_public`], for the call sites that carry a configured
/// string and have no other reason to depend on the `url` crate. A string that
/// does not parse is refused, not accepted.
///
/// # Errors
///
/// [`SsrfError::InvalidUrl`] when the string does not parse or carries no host,
/// [`SsrfError::PrivateAddress`] for an internal destination.
pub fn assert_public_str(url: &str) -> Result<(), SsrfError> {
    let parsed = url::Url::parse(url).map_err(|e| SsrfError::InvalidUrl(e.to_string()))?;
    assert_public(&parsed)
}

/// Redirect-chain cap applied when a call site does not pick its own. Mirrors
/// reqwest's own default of 10 hops.
pub const DEFAULT_MAX_REDIRECTS: usize = 10;

/// Cap for a response whose whole point is to be small: an API's JSON answer,
/// a checksum file, a hook decision. 1 MiB is two orders of magnitude above
/// what any of those legitimately carries.
pub const MAX_METADATA_BYTES: u64 = 1024 * 1024;

/// Cap for a response that carries a document rather than metadata: a release
/// archive, a model file listing, an exported drive file.
pub const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;

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
    /// Hop target resolves to a private or internal destination: refuse.
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
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RedirectBlocked {
    /// A hop resolved to a private or internal destination.
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
/// reqwest follows 3xx `Location` headers, which the peer controls, so each hop
/// is re-validated here. A refused hop aborts the chain *before* the socket to
/// the blocked host is opened; the resulting error surfaces through
/// `send().await` as a transport failure carrying the [`RedirectBlocked`]
/// message.
pub fn public_redirect_policy(max_redirects: usize) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        match evaluate_redirect(attempt.url(), attempt.previous().len(), max_redirects) {
            RedirectDecision::Follow => attempt.follow(),
            RedirectDecision::Block(err) => attempt.error(RedirectBlocked::Ssrf(err)),
            RedirectDecision::TooMany => attempt.error(RedirectBlocked::TooMany(max_redirects)),
        }
    })
}

/// A [`reqwest::ClientBuilder`] that already carries the SSRF redirect policy.
///
/// Call sites add their own timeout, user agent and headers on top. Building a
/// client any other way is refused by `scripts/check_http_clients.py`, because
/// the policy is exactly what every copy of this code kept forgetting.
#[must_use]
pub fn safe_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().redirect(public_redirect_policy(DEFAULT_MAX_REDIRECTS))
}

/// A [`reqwest::ClientBuilder`] for an endpoint the operator configured on
/// purpose and which may legitimately be internal.
///
/// A local MCP server, the embedded runner, a self-hosted LLM: refusing a
/// loopback or LAN destination on those would refuse the endpoint's whole
/// point, so [`assert_public`] is deliberately not applied. The hop cap still
/// is, so a configured endpoint cannot walk the client through an unbounded
/// redirect chain. Prefer [`safe_client_builder`] wherever the destination is a
/// third-party host: this one is the named exception, not the default.
#[must_use]
pub fn configured_endpoint_client_builder() -> reqwest::ClientBuilder {
    configured_endpoint_client_builder_with_redirects(DEFAULT_MAX_REDIRECTS)
}

/// The same builder with an explicit hop cap.
#[must_use]
pub fn configured_endpoint_client_builder_with_redirects(
    max_redirects: usize,
) -> reqwest::ClientBuilder {
    reqwest::Client::builder().redirect(reqwest::redirect::Policy::limited(max_redirects))
}

/// The same builder with an explicit hop cap.
///
/// A call site that means to follow fewer hops than reqwest's default says so
/// here rather than by installing a policy of its own, which is how the copies
/// this module replaced came to differ from one another.
#[must_use]
pub fn safe_client_builder_with_redirects(max_redirects: usize) -> reqwest::ClientBuilder {
    reqwest::Client::builder().redirect(public_redirect_policy(max_redirects))
}

/// A ready-to-use client carrying the SSRF redirect policy.
///
/// # Errors
///
/// Propagates the `reqwest` build error, which on this workspace means the TLS
/// backend failed to initialise.
pub fn safe_client() -> Result<reqwest::Client, reqwest::Error> {
    safe_client_builder().build()
}

/// Failure of a capped body read.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReadCappedError {
    /// The body exceeded `limit` bytes and was abandoned mid-stream.
    #[error("response body exceeds {limit} bytes (stopped at {read})")]
    TooLarge {
        /// The cap that was exceeded, in bytes.
        limit: u64,
        /// Bytes seen when the read was abandoned, cap included. Callers that
        /// report a size to the user report this one: the cap alone would say
        /// the body was exactly as large as the ceiling, which is never what
        /// was measured.
        read: u64,
    },

    /// The transport failed while the body was being streamed.
    #[error("response body read failed")]
    Transport(#[source] reqwest::Error),

    /// The body was read whole but is not the JSON the caller asked for.
    #[error("response body is not the expected json")]
    Json(#[source] serde_json::Error),
}

/// Read a response body into memory, aborting once `limit` bytes are exceeded.
///
/// Streams chunk by chunk (`reqwest::Response::chunk`) so an oversized or
/// never-ending body is refused *before* it is fully buffered, which is what
/// `Response::bytes` cannot do: it buffers first and lets the caller measure
/// afterwards, by which point the memory is already spent.
///
/// # Errors
///
/// [`ReadCappedError::TooLarge`] when the body crosses `limit`,
/// [`ReadCappedError::Transport`] when the stream fails.
pub async fn read_capped_bytes(
    mut response: reqwest::Response,
    limit: u64,
) -> Result<Vec<u8>, ReadCappedError> {
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(ReadCappedError::Transport)? {
        let seen = buf.len() as u64 + chunk.len() as u64;
        if seen > limit {
            return Err(ReadCappedError::TooLarge { limit, read: seen });
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// Read a response body into a `String`, aborting once `limit` bytes are
/// exceeded.
///
/// Bytes are decoded lossily: every caller of this helper already assumed UTF-8
/// through `Response::text`, and a replacement character is a better answer
/// than an error the caller would map to the same message anyway.
///
/// # Errors
///
/// Same as [`read_capped_bytes`].
pub async fn read_capped_text(
    response: reqwest::Response,
    limit: u64,
) -> Result<String, ReadCappedError> {
    let bytes = read_capped_bytes(response, limit).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Read a response body under `limit` bytes and deserialise it.
///
/// # Errors
///
/// Same as [`read_capped_bytes`], plus [`ReadCappedError::Json`] when the
/// bounded body does not deserialise into `T`.
pub async fn read_capped_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    limit: u64,
) -> Result<T, ReadCappedError> {
    let bytes = read_capped_bytes(response, limit).await?;
    serde_json::from_slice(&bytes).map_err(ReadCappedError::Json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(u: &str) -> url::Url {
        url::Url::parse(u).expect("valid url")
    }

    #[test]
    fn rejects_ipv4_loopback() {
        // GIVEN a loopback URL
        let url = parse("http://127.0.0.1/");
        // WHEN the policy runs
        let err = assert_public(&url).expect_err("loopback");
        // THEN it is refused as a private address
        assert!(matches!(err, SsrfError::PrivateAddress(_)));
    }

    #[test]
    fn rejects_ipv4_private_ranges() {
        // GIVEN the three RFC 1918 ranges and the link-local metadata address
        for host in [
            "http://10.0.0.1/",
            "http://172.16.0.1/",
            "http://192.168.1.1/",
            "http://169.254.169.254/latest/meta-data/",
            "http://224.0.0.1/",
        ] {
            // WHEN the policy runs
            let err = assert_public(&parse(host)).expect_err(host);
            // THEN each is refused
            assert!(matches!(err, SsrfError::PrivateAddress(_)), "{host}");
        }
    }

    #[test]
    fn rejects_ipv6_internal_forms() {
        // GIVEN loopback, unique-local and IPv4-mapped-loopback v6 URLs
        for host in [
            "http://[::1]/",
            "http://[fd00::1]/",
            "http://[::ffff:127.0.0.1]/",
        ] {
            // WHEN the policy runs
            let err = assert_public(&parse(host)).expect_err(host);
            // THEN each is refused
            assert!(matches!(err, SsrfError::PrivateAddress(_)), "{host}");
        }
    }

    #[test]
    fn rejects_internal_domains() {
        // GIVEN the internal-domain suffixes
        for host in [
            "http://localhost/admin",
            "http://router.local/",
            "http://wiki.internal/",
            "http://box.localdomain/",
        ] {
            // WHEN the policy runs
            let err = assert_public(&parse(host)).expect_err(host);
            // THEN each is refused
            assert!(matches!(err, SsrfError::PrivateAddress(_)), "{host}");
        }
    }

    #[test]
    fn accepts_public_destinations() {
        // GIVEN public addresses and domains
        for host in [
            "https://8.8.8.8/",
            "https://[2606:4700:4700::1111]/",
            "https://example.com/article",
            "https://xn--bcher-kva.example/",
        ] {
            // WHEN the policy runs
            // THEN it accepts
            assert!(assert_public(&parse(host)).is_ok(), "{host}");
        }
    }

    #[test]
    fn rejects_missing_host() {
        // GIVEN a URL with no host component
        let url = url::Url::parse("file:///etc/passwd").expect("valid url");
        // WHEN the policy runs
        let err = assert_public(&url).expect_err("no host");
        // THEN it is refused as invalid rather than accepted
        assert!(matches!(err, SsrfError::InvalidUrl(_)));
    }

    #[test]
    fn assert_public_str_refuses_what_does_not_parse() {
        // GIVEN a string that is not a URL, and one that is an internal URL
        // WHEN each goes through the string form of the policy
        // THEN neither is accepted, and a public one is
        assert!(matches!(
            assert_public_str("not a url"),
            Err(SsrfError::InvalidUrl(_))
        ));
        assert!(matches!(
            assert_public_str("http://169.254.169.254/"),
            Err(SsrfError::PrivateAddress(_))
        ));
        assert!(assert_public_str("https://example.com/hook").is_ok());
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

    #[test]
    fn safe_client_builds_with_the_redirect_policy() {
        // GIVEN nothing but the helper
        // WHEN a client is built
        let built = safe_client();
        // THEN it builds, which is the only observable the reqwest API exposes
        //      about a redirect policy; the policy itself is covered by the
        //      `evaluate_redirect` cases above, which is why the decision lives
        //      outside the closure.
        assert!(built.is_ok());
    }
}
