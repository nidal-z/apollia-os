# ADR-053: Apollia embeds its Microsoft OAuth client, and no Google one

- Status: Accepted
- Date: 2026-08-04

## Context

The connector credentials resolve through three layers: a runtime environment
variable, then `~/.apollia/oauth-clients.toml`, then a constant compiled into
the binary. Until now the third layer had no producer at all. It read
`option_env!("APOLLIA_BUILD_*_CLIENT_ID")`, and nothing set those variables: not
`.github/workflows/release.yml`, not the `justfile`, not `scripts/build.sh`, and
`apollia-auth` has no `build.rs`. Every `.dmg` and `.msi` the project has
produced therefore carried an empty client id for both providers.

That was not a correctness defect after the credential gate landed. Connecting
is refused by name before a browser opens, and the documentation says plainly
that no client ships. It is a product cost: both connectors demanded a trip
through a cloud console before they did anything, which for Microsoft buys
nothing.

The maintainer registered a public client application for Apollia against the
Microsoft identity platform. The question this record settles is whether its
identifier belongs in the repository, and why Google is not treated the same
way.

### Is a native application's client id a secret

No, and this is the crux. RFC 8252 section 8.5 states that a native application
cannot hold a credential confidentially: anything shipped in a distributed
binary is extractable by whoever holds the binary. The registration is therefore
a public client with no secret at all, and PKCE carries the security of the
exchange. Hiding the identifier would protect nothing, because `strings` on the
application recovers it.

An alternative was considered and rejected: keep the value out of the source and
inject it at build time from a `.env` file consumed by the build scripts. It
costs real reliability for no security. At least nine entry points produce a
desktop or CLI binary (`crates/apollia-desktop/scripts/bundle-cli.sh`, several
`justfile` recipes for both dev and release, `scripts/build.sh`,
`scripts/test-build-macos.sh`, `.github/workflows/release.yml`), across three
operating systems. A scheme that misses one of them produces a binary with no
credential and fails silently, which is precisely the failure mode this
repository keeps rediscovering. A committed constant is greppable, therefore
guardable by `scripts/check_claims.py`, and makes a development build behave
exactly like a release build.

### Why Google cannot be treated the same way

Two independent obstacles, neither of which Apollia can remove for the user.

Google requires a verified OAuth consent screen before an application may serve
accounts outside its own project. Without verification the options are a Testing
status capped at 100 users whose refresh tokens expire after seven days, or a
Production status showing an unverified-application warning. Verification is
free for the scopes Apollia requests by default but takes several weeks.

Google's Desktop client type also issues a `client_secret` and requires it at
the token endpoint even under PKCE. Shipping that value would be publishing a
credential the provider treats as one, whatever Google's own documentation says
about installed applications.

A single shared Google client would additionally put every Apollia user behind
one quota and one consent screen, where one user's abuse suspends everyone.

## Decision

Apollia ships the client id of its own Microsoft public client registration as
`MICROSOFT_DEFAULT_CLIENT_ID` in
`crates/apollia-auth/src/connector_providers.rs`, and ships no Google client.

- Microsoft 365 connects with nothing to configure.
- Google Workspace continues to require the operator's own OAuth client, and the
  documentation states the asymmetry before the user reaches a connector page.
- No client secret is embedded for either provider. Microsoft's slot is
  hardcoded empty with no build-time hook, since a public client has none.
- The `APOLLIA_BUILD_*_CLIENT_ID` compile-time hook is kept. For Microsoft it
  overrides the shipped default; for Google it fills the empty slot. An empty
  value is treated as absent, so a build recipe exporting the variable without a
  value cannot ship a blank id that reads as configured.
- The existing overrides are unchanged and remain the way to bring your own
  registration: the environment variable, then `oauth-clients.toml`, then the
  constant. Clearing the override restores the shipped identifier.
- `build_microsoft_provider` keeps the `common` authority. The registration
  accepts personal Microsoft accounts and any directory, and a tenant-scoped
  authority would restrict sign-in to the directory that owns the registration.

## Consequences

Microsoft 365 becomes usable out of the box, and `detect_source` reports
`builtin` for it, which flips the desktop surface from a setup prompt to a live
Connect button with no frontend change.

The guard-rail in `docs/CLAIMS.toml` had to be split. One entry covered both
providers and its evidence substring was `APOLLIA_BUILD_GOOGLE_CLIENT_ID`, so
embedding a Microsoft constant would have left it green while half of what it
guarded turned false. It is now three entries: the resolution chain,
`oauth-microsoft-client-embedded` (wired, guarding that the constant is read and
not merely defined), and `oauth-google-client-not-embedded` (absent, firing if a
symmetric Google constant ever appears).

Apollia now carries an operational dependency it did not have: the registration
must stay alive and multi-tenant. If it were deleted, or narrowed to a single
tenant, every user outside the owning directory would fail after consenting with
an opaque provider error, and no gate in this repository could see it, since the
setting lives in the Entra portal rather than in the build. The escape hatch is
the override that already exists, and the Microsoft operator page documents it.

Two properties of the registration were verified without portal access, by
querying the device authorization endpoint with the shipped identifier and
confirming that a fabricated identifier produces `AADSTS700016` on the same
call: the application resolves in the personal-accounts directory
(`9188040d-6c67-4c5b-b112-36a304b66dad`) and in the `organizations` authority,
which is the multi-tenant plus personal-accounts configuration. The registered
redirect platform could not be verified this way, since the identity provider
defers redirect validation until after authentication and answers identically
for a registered and an unregistered value.
