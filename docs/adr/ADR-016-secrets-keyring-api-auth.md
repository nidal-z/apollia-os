# ADR-016: Secrets, keyring storage and local API auth

- Status: Accepted
- Date: 2026-06-04

## Context

Apollia stores long-lived secrets: OAuth refresh tokens for third-party
integrations (Google, Microsoft, Notion, GitHub) and static API keys. The
compromise of a refresh token grants persistent access to a user's services, so
these secrets must be protected at rest. The target audience includes headless
Linux users (servers, containers, minimal distributions) where no graphical
Secret Service daemon exists, which would otherwise crash the runtime at boot.

Separately, the local REST API exposes two surfaces. The Unix socket
(`~/.apollia/runtime.sock`) is reachable only by processes of the same UID, gated
by filesystem permissions. The TCP listener on `:7771` is a network surface: any
local process, and if misconfigured the local network, can reach destructive
endpoints. That surface must be authenticated without relying on a remote server
(principle #1) and must fail fast (principle #4).

## Decision

We store secrets in the OS keyring with an age-encrypted file fallback on headless
Linux, and we authenticate the TCP API with a static bearer token bound to loopback.

### Secret storage

Secret storage lives in `apollia-auth/src/secret_storage.rs`. The primary backend
is the OS keyring via the `keyring` crate: Keychain Services on macOS, Secret
Service (GNOME Keyring or KDE Wallet) on Linux, Credential Manager on Windows.
Entries are keyed per service and account. The OS keyring is audited and
maintained by the platform, so Apollia carries no encryption key management on the
common path.

On Linux without a Secret Service daemon, the keyring fails to initialize. The
fallback stores one age-encrypted file per secret in a directory,
`~/.apollia/secrets/<service>__<user>.age`, using the pure-Rust `age`
implementation (scrypt plus ChaCha20-Poly1305), with no system dependency and
identical behavior on every Linux variant. One file per secret avoids
lock-and-rewrite of a single bundle on every update. The passphrase is
mandatory: an empty passphrase fails fast at boot (`AgeFileSecretStore::new`
errors, and `from_env` requires `APOLLIA_TOKEN_PASSPHRASE`). There is no
UID- or machine-id-derived weak fallback and no silent plaintext write.
Activation is explicit via the `APOLLIA_TOKEN_STORAGE=file` environment
variable.

### Local API auth

At first runtime start a 256-bit token (32 random bytes from
`rand::thread_rng().fill_bytes`, encoded as 64 hex characters) is generated and
written to `~/.apollia/api-token` with mode `0600`. The `0600` mode is set on
generation; the file permissions are not re-verified on read. Every TCP request
must carry `Authorization: Bearer <token>`; an axum middleware compares it in
constant time (the `constant_time_eq` crate) before any handler and returns `401`
on mismatch or absence. TCP binds to `127.0.0.1` by default, never `0.0.0.0`; the
`api.bind` config opens wider use with an explicit network-exposure warning. The
Unix socket stays unauthenticated, relying on its filesystem permissions, and the
desktop app uses it exclusively. Rotation is manual via the CLI; the CLI reads the
token from the file automatically.

## Alternatives considered

### Encrypted file as the primary secret backend (rejected)
- Pros: no platform keyring dependency.
- Cons: the derivation key is either hardcoded (weak), prompted every boot
  (friction), or stored in the keyring (circular); correct file encryption with IV
  and MAC management is error-prone. The OS keyring is a better guarantee for the
  same effort.

### Environment variables for OAuth tokens (rejected)
- Pros: matches the static API key pattern.
- Cons: refresh tokens rotate and the process must write new values, impossible
  with inherited env vars.

### Linux fallback via D-Bus user session keyring (rejected)
- Pros: no user passphrase, consistent with macOS and Windows.
- Cons: fails silently on minimal distributions without a D-Bus user session,
  per-distro setup burden, "might work" behavior that violates fail-fast.

### OAuth2, JWT or mTLS for the local API (rejected)
- Pros: standard, scoped.
- Cons: needs an authorization server or a local CA for a single-user local
  process; no advantage over a static token for this threat model.

### Chosen: OS keyring with age fallback, static bearer token on loopback
- Pros: platform-native protection on the common path, works on every headless
  Linux, simple auditable API auth, no remote dependency.
- Trade-offs: keyring backups do not include the tokens (expected but surprising);
  the token does not expire and a stolen `api-token` file is a compromise until
  manual rotation.

## Consequences

- Positive: tokens protected by the same mechanism as browser passwords on the
  common path; the runtime boots on 100% of Linux targets; the TCP surface is
  closed with a constant-time check; existing Unix-socket clients are unaffected.
- Negative / trade-off: a forgotten age passphrase loses the tokens; the static API
  token has no automatic rotation.
- Watch: power-user feedback on the boot passphrase friction; a future
  multi-user mode will need per-token scopes.

## Architectural principles

- Principle #1 (Local-first): secrets and tokens stay on the machine; no remote
  endpoint verifies the API token.
- Principle #2 (Zero external dependency): the `keyring` crate compiles without
  external C on macOS and Windows and uses D-Bus on Linux desktops; `age` is pure
  Rust embedded for the fallback.
- Principle #4 (Fail fast): a missing token or a bad passphrase fails at boot,
  not at first request.

## Related

- [ADR-018](ADR-018-mcp-oauth.md) reuses this secret storage to persist MCP OAuth tokens.
