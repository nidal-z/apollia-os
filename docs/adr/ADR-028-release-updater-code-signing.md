# ADR-028: Release distribution: updater and code signing

- Status: Accepted
- Date: 2026-06-04

## Context

Apollia ships as a directly downloaded binary and bundle, from GitHub Releases,
without a package manager. Two release-time problems follow from that choice.

First, users who installed the binary directly need a way to update. Without an
updater they stay on stale versions and miss fixes. The update mechanism has to
fit the local-first model: explicit, user-triggered, with no silent background
update, and no dependency on a third-party package registry.

Second, the macOS bundle ships as a universal DMG, and without a code signature
Gatekeeper applies the quarantine bit to any binary downloaded from the internet
and refuses to run it with an alarming dialog. That friction is fatal for a
non-technical prospect who receives a download link: most users abandon at this
step. A full Apple Developer ID with notarization gives a native double-click
experience but costs an annual fee that is not sustainable at the current stage,
so signing must give the best achievable experience at zero cost while leaving a
clean path to Developer ID.

## Decision

We adopt a self-contained auto-updater that resolves the latest release through
the GitHub REST API and downloads the per-target binary directly from GitHub
Releases with a SHA256 integrity check, a lock file, and an atomic rename, plus
ad-hoc macOS code signing with hardened runtime and minimal entitlements for the
bundled Python interpreter, with a documented upgrade path to Developer ID.

### Auto-updater

The updater is driven from the CLI. A check command queries the GitHub REST API
(`GET /repos/{owner}/apollia-os/releases/latest`) and parses the `tag_name` of
the latest release; if it is strictly newer than the running binary, it offers
the update. The apply command resolves the release asset for the current target
triple (for example `apollia-os-x86_64-unknown-linux-musl`), downloads it along
with a per-binary companion checksum asset named `{binary}.sha256`, and verifies
the SHA256 of the download. It then writes a lock file at
`/tmp/apollia-update.lock` to prevent concurrent updates, stages the new binary
at `/tmp/apollia-new`, atomically replaces the current binary with a `rename`
from that staging file, and removes the lock.

The updater is written against a direct-binary model: each release exposes one
raw, uncompressed binary per target triple plus its `.sha256` companion. The
current release pipeline does not yet match that model; it publishes compressed
archives (`apollia-os-<preset>.tar.gz` on Linux/macOS, `.zip` on Windows, each
with a `.sha256`) under preset names rather than raw triple-named binaries. This
gap between the updater's expectation and the published assets is a known
reconciliation point: the raw-binary download path is the intended design and is
not claimed to work against the archives the pipeline currently ships. The
desktop and DMG jobs additionally emit an aggregate `SHA256SUMS` file, but the
CLI updater never reads it; that file belongs to the desktop bundle and the
separate `tauri-plugin-updater` flow.

Package managers (Homebrew, apt, `cargo install`) were rejected. They delegate to
third-party infrastructure and require releases published into separate
registries, adding operational complexity that direct binary distribution (the
pattern used by Ollama, Tauri, and Helix) avoids, and `cargo install` would
recompile from source on the target machine, requiring a Rust toolchain and
minutes of build time, which is unacceptable for a user update. Direct
distribution stays compatible with a future move to official packages.

The SHA256 check protects against download corruption and basic tampering, the
lock file protects against concurrent updates from two terminals, and the atomic
rename guarantees there is never a half-written binary if the update is
interrupted.

### macOS code signing

The `.app` bundle is signed ad-hoc during the CI build, in a post-bundle step
that runs after `cargo tauri build`. That single Tauri step produces both the
`.app` and the `.dmg`, so the DMG already exists when the ad-hoc signature is
applied to the `.app`; the signing happens on the packaged app, not before
packaging. Signing uses hardened runtime enabled and an entitlements file.
Hardened runtime is on even in ad-hoc, so the
runtime behavior is identical to what the notarized version will have, with no
surprise on the migration day.

The entitlements carry the minimal exceptions needed for the PyO3 bridge to run
against a bundled Python interpreter that Apple did not sign:

- Allow unsigned executable memory, because PyO3 executes dynamically compiled
  Python bytecode.
- Allow dyld environment variables, because the Python home and library paths are
  exported at runtime before the runtime initializes; without this entitlement
  hardened runtime would purge them silently.
- Disable library validation, because the app must load the bundled
  `libpython` shared library, which comes from a verifiable third-party
  distribution rather than from our own Apple Team ID; without this entitlement
  dyld would refuse to load it.

The Tauri bundle configuration sets the signing identity to the ad-hoc marker,
enables hardened runtime, and points at the entitlements file. The whole bundle
hierarchy, including the CLI binary and the bundled Python library under
`Contents/Resources/`, is signed in the post-bundle step on the packaged `.app`.

Ad-hoc signing leaves a Gatekeeper warning on first launch, so the first-launch
procedure (right-click then Open, or clearing the quarantine attribute from the
terminal) is documented in the install instructions, the landing page, and the
known-issues entry.

The upgrade path to Developer ID is clean: add the Apple signing secrets to CI,
replace the ad-hoc identity marker with the real signing identity, and add a
notarization submit step. No change to the application code or the entitlements
is required.

## Alternatives considered

### Package manager distribution and update (rejected)
- Pros: delegates installation and update to the OS package system.
- Cons: depends on third-party infrastructure, requires releases in separate
  registries, and adds operational complexity disproportionate to the current
  stage.

### Update via `cargo install` (rejected)
- Pros: always builds for the exact target machine.
- Cons: recompiles from source, requires a Rust toolchain, and takes minutes,
  which is unacceptable for a user update.

### No code signature (rejected)
- Pros: zero cost, no setup.
- Cons: Gatekeeper blocks the app outright with no native Open button, forcing
  the user to leave the app and clear the quarantine attribute from the terminal.

### Apple Developer ID plus notarization (deferred, not rejected)
- Pros: a native double-click experience with no warning.
- Cons: an annual fee that is not sustainable at the current stage. Kept as the
  documented next step once a commercial signal justifies it.

### Chosen: direct-download updater plus ad-hoc signing with hardened runtime
- Pros: a self-contained user-triggered update with integrity, concurrency, and
  atomicity guarantees, zero signing cost, hardened runtime active from the first
  release, and a clean Developer ID upgrade path.
- Trade-offs: SHA256 alone does not protect against a compromised release server
  (a signature step is the future hardening), the static binary is larger, the
  first-launch Gatekeeper warning remains and may cost some non-technical
  prospects, and disabling library validation weakens the macOS security model
  for the auditable bundled Python library.

## Consequences

- Positive: users update with one command and an integrity-checked, atomic
  replacement, the macOS bundle opens after a documented first-launch step at zero
  cost, and the runtime behavior already matches the future notarized build.
- Negative / trade-off: no cryptographic signature on the update payload yet, a
  larger static binary, a residual first-launch warning, and a weakened library
  validation entitlement.
- Watch: the download-to-first-run conversion as a proxy for first-launch
  abandonment, and the integrity of the release server pending a future signature
  step on the update payload.

## Architectural principles

- Principle #1 (Local-first): updates are explicitly triggered by the user, never
  silent or in the background.
- Principle #2 (Zero external dependency): ad-hoc signing creates no dependency on
  a third-party service, unlike notarization which calls an Apple API.
- Principle #4 (Fail fast): a SHA256 mismatch produces an explicit error with the
  expected and received hashes and the binary is not installed. The post-bundle
  `codesign --verify` step is informational and runs non-fatally, so it does not
  by itself gate the CI build.
- Principle #8 (Human CLI, machine API): the bundled CLI binary is signed too, so
  a command-line run after the bundle is installed does not trigger a separate
  Gatekeeper prompt.

## Related

- [ADR-020](ADR-020-desktop-architecture.md) the desktop bundle that ships the
  signed binaries and the embedded Python interpreter.
