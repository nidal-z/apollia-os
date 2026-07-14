# ADR-047: Native TLS on the API TCP listener and fail-fast on insecure remote bind

- Status: Accepted
- Date: 2026-07-14

## Context

Apollia's wedge is an embeddable and federable backend that a host application
consumes. The runtime API server (`crates/apollia-runtime/src/api/server.rs`)
serves two surfaces from one axum `Router`: a Unix domain socket (local-trust,
guarded by filesystem permissions, never token-authenticated) and an optional TCP
listener on `bind_addr:tcp_port`. The TCP surface is gated by a constant-time
Bearer token when `[api].require_token` is set.

Two gaps remained on the remote-exposure surface, both of which matter once the
daemon is bound beyond loopback for a host or a federated peer.

First, the TCP transport is cleartext. The Bearer token is the only credential,
and without transport encryption it travels in the clear on every request, along
with every payload. [ADR-044](ADR-044-agent-isolation-hardening.md) hardened the
local tool surface and made the posture honest; it did not address transport
confidentiality for the remote API. `docs/internal/integrations/remote-daemon-deployment.md`
documents TLS only through an external reverse proxy, which is a valid deployment
but leaves the daemon itself unable to terminate TLS, so the simplest exposed
configuration is unencrypted.

Second, binding a non-loopback address without a token is only a warning. When
`tcp_port` is set, `bind_addr` is not loopback, and no token is configured, the
server logs `warn!("api.tcp.unauthenticated")` and then serves the full API to
any interface with no authentication. This contradicts principle 4 (fail fast:
any startup-detectable error is detected at startup) and the runtime rule that
invalid configuration fails fast (`crates/apollia-runtime/AGENTS.md` section 6).
A misconfigured operator gets a silently public runtime instead of a refusal.

## Decision

We add optional native TLS to the API TCP listener and turn the insecure-bind
warning into a startup refusal. Both changes harden the same surface: how the
daemon is safely exposed beyond loopback. The Unix socket, the loopback path, and
the embedded (loopback-only) host are unchanged.

### Optional native TLS via rustls

The TCP listener terminates TLS when, and only when, a certificate and a private
key are configured (`[api].tls_cert` and `[api].tls_key`, PEM paths). Absent that
pair, the listener stays cleartext and behaves exactly as before, preserving
strict backward compatibility. Configuring exactly one of the pair is a
configuration error caught at startup (both-or-neither).

The implementation uses `tokio-rustls` (rustls 0.23, ring provider). The rustls
stack is already vendored in `Cargo.lock` transitively (via `reqwest` and
`hyper-rustls`), so this is a new usage of an existing dependency, not a new
sovereignty surface. PEM parsing uses the `rustls-pki-types` PEM helpers already
in the tree, so no additional PEM crate is introduced. The certificate chain and
key are loaded once at startup; a load or parse failure is a fail-fast error, so a
daemon configured for TLS never falls back to cleartext.

The axum TCP path previously used `axum::serve`, which owns the `TcpListener` and
cannot serve pre-accepted TLS streams. The TCP path becomes a manual accept loop
that mirrors the existing Unix-socket loop (`hyper-util` `serve_connection` over a
`TowerToHyperService`), wrapping each accepted connection in a `TlsAcceptor` when
TLS is active. A failed handshake drops that one connection and is logged; it
never stops the accept loop. Graceful shutdown continues to be driven by the
existing `watch` channel.

### Fail-fast on non-loopback bind without a token

At startup, before binding the TCP listener, the server refuses to start when the
bind address is not loopback and no token is configured. Loopback is
`localhost`, or any address that parses as an IP and is loopback (`127.0.0.0/8`,
`::1`); anything else, including an unparseable host, is treated as non-loopback
and therefore requires a token. Loopback without a token stays allowed for
development and embedded use. The Unix socket is never affected. The former
`warn!` becomes a dedicated error variant returned from server startup, so the
supervisor's ordered startup rolls back cleanly.

TLS does not remove the token requirement: encryption protects the token in
transit, it does not authenticate the caller. A non-loopback bind still needs a
token whether or not TLS is on.

## Alternatives considered

### Reverse-proxy TLS termination only (rejected as the default, kept as an option)
- Pros: no TLS code in the runtime; battle-tested proxies (Caddy, nginx).
- Cons: the daemon alone cannot be exposed safely; every remote deployment needs
  extra infrastructure, and the naive path stays cleartext. Native TLS makes the
  secure configuration reachable without a second component. The proxy path stays
  documented and supported for operators who prefer it.

### Keep the warning, do not fail fast (rejected)
- Pros: no behavior change; a determined operator can still bind publicly with no
  token.
- Cons: violates principle 4 and the fail-fast configuration rule. A silently
  public runtime is exactly the outcome the fail-fast principle exists to prevent.
  An operator who truly wants an unauthenticated public bind can still do so by
  binding loopback behind their own proxy, or the requirement can be revisited
  with an explicit opt-out flag in a later cycle.

### Generate a self-signed certificate automatically when TLS is unset (rejected)
- Pros: TLS on by default with zero configuration.
- Cons: self-signed-by-default trains clients to skip verification, which is worse
  than honest cleartext on loopback. TLS is opt-in with operator-provided material
  so the trust chain is explicit. Automatic certificate provisioning (ACME) is a
  possible later addition, tracked, not built here.

### Add `rustls-pemfile` / `rcgen` for parsing and test fixtures (rejected)
- Pros: familiar PEM ergonomics; programmatic self-signed cert generation in tests.
- Cons: both are new crates not currently in the tree. The `rustls-pki-types` PEM
  API already vendored covers parsing, and tests embed a fixed self-signed fixture,
  so no new dependency is added for either.

## Consequences

- Positive: the daemon can terminate TLS itself, so a remote or federated exposure
  no longer requires a reverse proxy to be encrypted; a non-loopback bind without a
  token now refuses to start instead of serving a public unauthenticated API; the
  only new direct dependency (`tokio-rustls`) is already vendored.
- Negative / trade-off: operators who relied on the tolerated warn-only public bind
  must now provide a token (or bind loopback); TLS is opt-in, so the default remote
  path is still cleartext unless a certificate is configured, and that trade-off
  must keep being documented honestly.
- Watch: `docs/internal/integrations/remote-daemon-deployment.md` sections on
  proxy-only TLS and the warn-only misconfiguration must be updated to describe
  native TLS and the fail-fast refusal (tracked follow-up). Automatic certificate
  provisioning and an explicit unauthenticated-bind opt-out remain open.

## Architectural principles

- Principle #1 (Local-first): unchanged; TLS terminates in the runtime, adds no
  external service, and the loopback and Unix paths are untouched.
- Principle #2 (Zero external dependency): the one new direct dependency,
  `tokio-rustls`, is already vendored transitively; no new crate enters the tree.
- Principle #4 (Fail fast): a non-loopback bind without a token, and a TLS
  configuration that cannot be loaded, are both detected at startup and refuse to
  start rather than degrading.
- Principle #8 (Human CLI, machine API): unchanged; the TLS and bind policy are
  configuration, not new API surface.

## Related

- [ADR-003](ADR-003-sandbox-trust-platform-scope.md) sets the trust model this
  hardening operates within.
- [ADR-044](ADR-044-agent-isolation-hardening.md) hardened the local tool surface
  and the posture honesty this ADR extends to the remote transport.
- [ADR-045](ADR-045-supervisor-fail-fast-degrade.md) established fail-fast at
  startup then degrade as the supervision model; this ADR adds one more
  startup-detectable refusal in that spirit.
