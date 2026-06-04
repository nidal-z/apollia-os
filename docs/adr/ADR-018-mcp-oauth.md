# ADR-018: MCP OAuth client and orchestration

- Status: Accepted
- Date: 2026-06-04

## Context

The MCP specification normalizes HTTP transport security around OAuth 2.1 with a
bundle of mandatory RFCs. For Apollia to consume any official HTTP MCP server
(GitHub, Atlassian, Cloudflare, Stripe, Linear, Notion, Sentry, Slack) without
manual configuration, the client must implement the standard discovery and the
authorization code flow, and wire it end to end: detect a 401, discover the
protected resource and authorization server, identify the client, run PKCE, exchange
and refresh tokens, persist them, and inject a fresh bearer at call time. Until that
orchestration exists, the official HTTP servers in the catalogue are non-functional
at install time.

The normative pieces are RFC 9728 (protected resource metadata), RFC 8414 (AS
metadata) or OIDC Discovery, RFC 8707 (resource indicators, mandatory `resource=`),
RFC 7591 (dynamic client registration) as a fallback, Client ID Metadata Documents
(CIMD) as the recommended no-prior-relationship path, and PKCE S256 as mandatory.

## Decision

We implement a generic MCP OAuth 2.1 client in `apollia-auth`, with zero
provider-specific code, wired end to end by an orchestrator. The runtime runs the
same sequence for every spec-compliant HTTP MCP server.

### Generic OAuth client

The client implements discovery and the code flow per the spec: parse
`WWW-Authenticate` (RFC 6750) for the protected resource metadata URL with a
well-known fallback, fetch the PRM (RFC 9728) with an RFC 8414 fallback, fetch AS
metadata (RFC 8414 or OIDC Discovery), run PKCE S256 (RFC 7636), and bind the
audience with `resource=` at both the authorize and token endpoints (RFC 8707).
Client identification follows a priority order: static pre-registration for known
cases, then CIMD (a static JSON document hosted on a standard well-known path), then
dynamic client registration (RFC 7591). The loopback callback follows RFC 8252.

### Orchestrator

The orchestrator lives in `apollia-auth`, decoupled from the desktop and the
browser opener so it is reusable from the CLI. Its public surface is two functions:

- `negotiate_token(req, store, open_browser)` takes a `NegotiateRequest`
  (`server_name`, `server_url`, `www_authenticate`, `scopes`, and
  `client_id_override`), the secret store, and an injected browser opener. It runs
  the full sequence (parse 401, PRM, AS metadata, client identification, loopback
  bind, PKCE, authorize, callback with `state` validation, token exchange with
  repeated `resource=`) and persists the result.
- `ensure_fresh_token(server_name, store)` returns a valid access token, refreshing
  when it is within 60 seconds of expiry.

The browser opener is injected, so the CLI can present a paste-the-URL flow.

### Token persistence

Tokens are persisted through the existing `SecretStorage` (keyring or age fallback)
under the service `apollia-mcp-oauth`, keyed per server name, serialized as a
`StoredMcpToken` (access and refresh tokens, expiry, scope, resource URI, AS URL,
client id, optional identity claims). No new storage backend is introduced.

### Concurrency and callback

Refresh is guarded by a per-server singleflight (a Tokio mutex keyed by server name)
so a burst of expiring tool calls produces one refresh, not N. The loopback callback
router is unified: a single listener serves both `/callback` (native connectors) and
`/oauth/callback` (MCP), with identical capture of `code` and `state`, since only one
OAuth flow runs at a time.

### Wizard

The connector wizard probes the server when the auth step opens and branches into
three modes: success (no auth, continue), OAuth required (a scope selector over the
advertised scopes plus a "Connect to <provider>" button that runs the flow and shows
the returned identity, storing `Authorization = "${APOLLIA_OAUTH}"`), and error
(message plus retry). At call time the transport resolves `${APOLLIA_OAUTH}` to a
fresh bearer via `ensure_fresh_token`.

## Alternatives considered

### Static pre-registration only (rejected)
- Pros: simple.
- Cons: every new official MCP server would require an Apollia release.

### Dynamic client registration everywhere (rejected)
- Pros: one mechanism.
- Cons: depends on the AS exposing a registration endpoint and may demand a client
  secret, incompatible with a public PKCE client.

### Orchestrator inside the desktop crate (rejected)
- Pros: fewer layers.
- Cons: OAuth code becomes unreusable from the CLI and couples a security-critical
  flow to the UI.

### Dedicated MCP token store and separate callback listener (rejected)
- Pros: strongly typed store, strict flow isolation.
- Cons: duplicates the existing secret backend and adds a second coordinated
  listener for an identical callback semantics.

### Chosen: generic client in `apollia-auth`, orchestrator, reused SecretStorage, unified callback
- Pros: any spec-compliant HTTP MCP connects with no Apollia-specific code; real
  scale gain; tokens stay in local storage; RFC 8707 prevents cross-resource replay.
- Trade-offs: OAuth client code to maintain against spec evolution; operational
  dependency on the hosted CIMD document, mitigated by automatic DCR fallback.

## Consequences

- Positive: any compliant HTTP MCP server connects without per-provider code; tokens
  stay in local storage and are never relayed; singleflight avoids AS rate limits on
  bursts; the scope selector gives the operator granular control.
- Negative / trade-off: multi-account per MCP server is deferred (one stored token
  per server name under the `apollia-mcp-oauth` service); the scope selector exposes
  opaque scope strings unless the AS publishes descriptions.
- Watch: DPoP and step-up auth on `insufficient_scope` are active spec directions;
  on server-side token revocation, the orchestrator re-runs `negotiate_token` rather
  than a plain refresh.

## Architectural principles

- Principle #1 (Local-first): tokens in local storage, callback on loopback only.
- Principle #2 (Zero external dependency): no new crate dependency; the browser is
  invoked through an injected opener.
- Principle #4 (Fail fast): each discovery step fails with a dedicated error variant
  surfaced to the UI.
- Principle #7 (Non-negotiable safeguards): PKCE S256 and the RFC 8707 audience
  binding are mandatory and prevent confused-deputy attacks; the callback validates
  `state`.
- Principle #8 (Human CLI, machine API): the orchestrator is decoupled from the
  browser opener, so the CLI can render a paste-the-URL flow.

## Related

- [ADR-016](ADR-016-secrets-keyring-api-auth.md) provides the secret storage that persists MCP OAuth tokens.
