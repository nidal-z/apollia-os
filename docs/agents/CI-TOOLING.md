# CI-TOOLING

> Editor configs, lint configs, MSRV, pre-commit, and CI pipeline. Read this
> when setting up a new machine or modifying the build infrastructure.

This file is the reference. The actual config files in the repo are the
source of truth at runtime.

---

## 1. Files at a glance

| File | Purpose |
|---|---|
| `.editorconfig` | per-language indentation, line endings, EOL whitespace |
| `rustfmt.toml` | Rust formatter config (edition, max width, import grouping) |
| `clippy.toml` | Clippy config (MSRV, complexity thresholds) |
| `rust-toolchain.toml` | pinned Rust toolchain (channel, components) |
| `pyproject.toml` | Python project metadata, Ruff config, pytest config |
| `.pre-commit-config.yaml` | pre-commit hook definitions |
| `Cargo.toml` (root) | `[workspace.dependencies]`, `[workspace.lints]`, `[workspace.package]` |
| `package.json` (desktop UI) | Node tooling, scripts |
| `.github/workflows/*.yml` | CI pipelines |
| `scripts/linux-check.sh` + `scripts/linux-check.Dockerfile` | ask the Linux question locally, in a container, before pushing |

---

## 2. The config files, and why they say what they say

The files themselves are the source. `cat` them; they are short and they are
what the tools actually read. What follows is the part a file cannot tell you.

| File | Run it with |
|---|---|
| `.editorconfig` | your editor, automatically |
| `rustfmt.toml` | `cargo fmt --all --check` to check, `cargo fmt --all` to apply |
| `clippy.toml` | `cargo clippy --workspace --all-targets -- -D warnings` |
| `rust-toolchain.toml` | `rustup` picks it up inside the repo |
| `sdk/pyproject.toml` | `ruff format --check`, `ruff check`, `cd sdk && mypy apollia` |

**`imports_granularity` and `group_imports` are deliberately absent from
`rustfmt.toml`.** Both are nightly-only. Keeping them forced the whole tree to
be formatted by a nightly rustfmt that stable CI could never reproduce, so the
fmt gate failed on every file. Edition stays at 2021; moving to 2024 is a
separate ADR and its own PR.

**Two version numbers, and confusing them wastes an afternoon.** The build and
gate toolchain is pinned in `rust-toolchain.toml`, and every blocking Rust job
installs that exact version rather than `@stable`, so CI and local output match
byte for byte. The MSRV floor is a different number, declared in `Cargo.toml`
`rust-version` and `clippy.toml` `msrv`: it is the minimum the dependency tree
supports, not the version anything runs on. A `clippy-stable` job runs on
`@stable` as a non-blocking advisory, to surface lints arriving in a future
toolchain.

Without `rustup`, a Homebrew-only Rust will not honour the pin. Keep it at the
pinned version by hand or accept fmt and clippy drift against CI.

**Ruff runs a curated subset rather than `select = ["ALL"]`**, so its warnings
stay actionable. Expand it when the codebase is clean for the current set.
`mypy --strict` is the type gate, and the only one: `strict = true` sits under
`[tool.mypy]` in `sdk/pyproject.toml`, and the `python-types` job runs
`mypy apollia` from `sdk/`. No second type checker is configured or invoked.

---

## 3. Workspace lints

`[workspace.lints]` is applied, not planned. `unsafe_code = "deny"` and
`unwrap_used = "deny"` are set at the root and inherited by every crate that
writes `[lints] workspace = true`.

The trap is that Cargo **replaces** rather than merges: a crate declaring any
local `[lints]` table loses the inheritance entirely. Five crates needed an FFI
`unsafe_code = "allow"`, wrote their own table, and silently lost
`unwrap_used`. `scripts/check_crate_lints.py` runs in `prose-guard` and now
fails the build on that shape. The full explanation is in
`docs/agents/RUST-PATTERNS.md`.

---

## 4. Pre-commit hooks

`.pre-commit-config.yaml` is the source; read it there. What it runs, in one
line: the usual hygiene hooks plus `detect-private-key` and a 500 KB file cap,
`ruff` and `ruff-format` on Python, `rustfmt`, `clippy -D warnings` and
`cargo check` on the workspace, and `conventional-pre-commit` on the message.

```sh
pre-commit install && pre-commit install --hook-type commit-msg
pre-commit run --all-files    # on demand
```

Never `--no-verify`. A failing hook is the cheapest place a problem is ever
going to be found.

---

## 5. CI pipeline (GitHub Actions)

Actual structure :

```
.github/workflows/
├── ci.yml            # PR gate (see jobs below)
├── codeql.yml        # CodeQL: rust + python + javascript-typescript
├── nightly.yml       # heavy / advisory: e2e, feature-matrix, coverage HTML,
│                     #   deep-audit, geiger (unsafe surface), loom, miri, kani, fuzz-deep
├── release.yml       # tag-triggered: build binaries, attach to release
└── auto-close-prs.yml
```

`ci.yml` blocking jobs (Rust jobs on the pinned `1.95.0`) :

1. `fmt` : `cargo fmt --all -- --check`
2. `clippy` : `cargo clippy --workspace --all-targets -- -D warnings`
3. `test` : `cargo test --workspace --no-fail-fast` (+ python-tests)
4. `test-macos` : `cargo test` on macOS Silicon
5. `machete` : `cargo machete` (unused deps)
6. `python-quality` : `ruff format --check` + `ruff check` + `pip-audit` on `sdk/`
7. `python-types` : `mypy apollia` (strict)
8. `coverage` : `cargo llvm-cov --workspace --fail-under-lines $COVERAGE_FLOOR`
9. `audit` / `deny` : `rustsec/audit-check` + `cargo-deny check` (full)
10. `vitest` (frontend), `prose-guard`, `links` (lychee)

Non-blocking (`continue-on-error`) advisory jobs :

**A `continue-on-error: true` job must carry `advisory` in its `name:`**, not
only in a comment above it. The name is what the checks list shows; a comment is
invisible there. Two jobs broke this rule and they are exactly the two that hid
a real failure for months: a `diagrams` job invoked a justfile recipe that does
not exist, and `python-tests` swallowed three failing SDK tests. Both are gone
now, the first deleted with the PlantUML corpus and the second made blocking
once its tests were fixed. A job whose failure nobody sees is worse than no job,
because it reads as coverage.

Advisory is a temporary posture. Every entry below states what would make it
blocking.

- `clippy-stable` : same clippy gate on `@stable`, surfaces future lints.
- `semver-checks` : `cargo semver-checks` on `apollia-core` / `apollia-runtime`
  against `origin/main`. Informative while the crates are unpublished / pre-1.0.
- `fuzz` : `cargo +nightly fuzz run` over each seed corpus for ~60s (nightly,
  libFuzzer). A short regression smoke on the untrusted-input parsers; the long
  session is `fuzz-deep` in `nightly.yml`. See `docs/agents/TESTING.md` 8b.

`python-tests` is **blocking**. It was advisory while three SDK tests failed;
they are fixed, so it gates.

Nightly-only advisory jobs (in `nightly.yml`) :

- `loom` : exhaustive interleaving check of the runtime actor algorithms on the
  pinned `1.95.0`, via `RUSTFLAGS="--cfg loom" cargo test --manifest-path
  crates/apollia-loom-models/Cargo.toml`. The crate is workspace-excluded (like
  `fuzz/`) because `--cfg loom` poisons Tokio.
- `miri` : the repo's first nightly-toolchain job. Installs `nightly` + the
  `miri` component and runs `cargo +nightly miri test -p apollia-aip --lib
  miri_pure` to check the FFI-adjacent pure helpers for UB. See
  `docs/agents/TESTING.md` 8c.
- `kani` : bit-precise symbolic proof of the cardinal invariants. Installs
  `kani-verifier` (`cargo install --locked kani-verifier && cargo kani setup`)
  and runs `cargo kani -p apollia-oria` (non-bypassable StepBudget) and
  `cargo kani -p apollia-runtime` (mailbox lease/ack fence). Kani links its own
  toolchain via rustup, so it is CI-only (the Homebrew dev machines have no
  rustup); the in-tree proptest mirrors are the runnable local proof. See
  `docs/agents/TESTING.md` 8d.

Caching : `Swatinem/rust-cache@v2` (keyed on `Cargo.lock`).

Toolchain : the pinned `1.95.0` on the gate jobs, `@stable` only on the
advisory `clippy-stable`. No `[1.85, stable]` matrix : 1.85 does not compile
(the tree needs >=1.89).

### Action pinning (all workflows)

Every `uses:` is pinned to a full 40-char commit SHA, with the human-readable
version in a trailing comment (`uses: actions/checkout@<sha> # v5`). Mutable
tags (`@v5`) and rolling branches (`@stable`) are never used directly: a tag
can be re-pointed at malicious code, a SHA cannot. Dependabot's `github-actions`
ecosystem bumps the SHA and rewrites the comment.

Two ref forms carry the toolchain / tool selection in the ref itself, so pinning
the SHA requires moving that selection into an input:

- `dtolnay/rust-toolchain@<sha>` with an explicit `toolchain:` input (the ref
  name no longer selects the channel once it is a SHA).
- `taiki-e/install-action@<sha>` with an explicit `tool:` input.

### Release supply-chain (`release.yml`)

On top of the per-artifact `SHA256` checksums and the secret-gated native code
signing (Apple notarization, Windows Authenticode, Linux GPG), each release
carries:

- **SBOM** : `anchore/sbom-action` (syft) scans each assembled bundle directory
  and emits a CycloneDX (`.cdx.json`) and an SPDX (`.spdx.json`) SBOM. Scanning
  the bundle, not just the Cargo graph, captures the embedded CPython (and, for
  desktop, the npm frontend) that actually ships.
- **Signatures** : `cosign sign-blob` (keyless, Sigstore OIDC) signs every
  published file, emitting a detached `.sig` and the signing `.pem` certificate.
  No long-lived key: the signing identity is the release workflow.
- **Provenance** : `actions/attest-build-provenance` produces a SLSA
  build-provenance attestation over the binaries and SBOMs.

Signing and attestation are confined to the final `release` job, the only job
that escalates the token to `id-token: write` + `attestations: write` (plus the
`contents: write` needed to publish). Every workflow defaults to
`permissions: contents: read` at the top level; jobs escalate only what they
need (`audit` / `deep-audit` add `checks: write`). Verification commands for
consumers live in `SECURITY.md`.

---

## 6. Coverage

The `coverage` job in `ci.yml` enforces a line-coverage floor :

```sh
cargo llvm-cov --workspace --lcov --output-path lcov.info \
  --fail-under-lines $COVERAGE_FLOOR
```

`COVERAGE_FLOOR` is a workflow-level env var set just below the measured
baseline so the gate is green from day one, then ratcheted up over time and
never lowered. The full HTML report is produced by the nightly `coverage` job.

Target : > 80% lines / > 70% branches on core crates. Aspirational
workspace-wide.

---

## 7. Local setup quickstart

```sh
# Clone
git clone git@github.com:Apollia-OS/apollia-os.git
cd apollia-os

# Toolchain (rust-toolchain.toml handles the Rust side)
rustup show

# Python
uv venv .venv
source .venv/bin/activate
uv pip install -e ./sdk

# Pre-commit
pre-commit install
pre-commit install --hook-type commit-msg

# First build
cargo build --workspace
cargo test --workspace --no-fail-fast
```

### Ask the Linux question before you push

Every local gate above runs on the platform that measures. In August 2026 the
workspace stopped compiling on Linux for a week while every one of them stayed
green: a `setrlimit` type that is `c_int` on the Apple libc and `c_uint` on
glibc. Four CI jobs went red, three more were skipped behind `needs: clippy`,
and the macOS test job was one of the three. Being the platform that measures
protects from nothing.

```sh
just linux-check              # x86_64-unknown-linux-gnu, the release target
just linux-check arm          # aarch64-unknown-linux-gnu, native on Apple Silicon
```

It runs `cargo clippy --workspace --all-targets --locked -- -D warnings` in a
container, on the working tree mounted read-only, with `CARGO_TARGET_DIR` in a
volume so the host `target/` is never touched. It is clippy and not check
because on the very tree it was written against, `cargo check` returned 0 where
clippy returned 101, on dead-code enum variants behind a `cfg`.

Three exit codes, and the third one is the point:

| Code | Meaning |
|---|---|
| 0 | the tree compiles on the measured target |
| 1 | the tree does not compile; cargo's output above is the verdict |
| 2 | nothing was measured: no docker, daemon down, image not built, unknown argument, or a container reporting a different triple |

2 is distinct from 1 so "I could not measure" is never read as "the tree is
fine". The daemon is deliberately not started for you.

Every run prints the perimeter it measured before it measures it: image,
platform, triple read out of the container with `rustc -vV`, release preset,
workspace member count read from `cargo metadata`, and what it does not cover
(the other Linux triple, both Windows triples, and the feature presets). Requires
a running Docker daemon and roughly 7.5 GB of image and volumes per
architecture; without one, read the verdict of the `Clippy` job of
`.github/workflows/ci.yml` on a pushed branch instead.

### Linked worktrees need one command first

A `git worktree` receives only what git tracks, and four of the expensive gates
read paths that git does not carry. Left unprepared, they do not merely fail:
`npx svelte-check` exits 1 over 853 files and 2050 errors instead of 4943 files
and 1 error, which reads as a repository regression and is not one.

```sh
git worktree add --detach ../wt-topic HEAD
cd ../wt-topic
just worktree-prep rust          # Python bundle link + the CLI binary
just worktree-prep ui docs       # npm ci where a gate needs it
just worktree-prep full          # all three
```

Groups are cumulative and there is no default: `just worktree-prep` with no
argument lists them and exits 1. A default would be a guess, and the guess is
what costs the false verdict above. `rust` links `target/Resources/python` to
the bundle of the main working tree, which the recipe resolves through git and
prints as an absolute path, and it builds `apollia-os` because
`tests/cli/cli-e2e.sh` resolves that binary and never builds it.

To check that a worktree measures what the main tree measures, record both and
compare. The comparison is on each guard's characteristic measure, not on the
exit code alone, and it refuses two records made on different commits:

```sh
just worktree-verdicts /tmp/main.json        # from the main working tree
just worktree-verdicts /tmp/wt.json          # from the worktree
just worktree-compare /tmp/main.json /tmp/wt.json
```

A guard whose precondition is missing is recorded as `not prepared` and never
counts as agreeing. Read `scripts/worktree-prep.sh` for what each group lays
down; it is the one place to update when a gate starts reading a new untracked
path.

---

## 8. When the rules block you

- A lint flags legitimate code : add `#[allow(clippy::<lint>)]` with an
  inline justification. Never `--allow` blanket on the command line.
- A pre-commit hook is too slow : split the hook (formatter fast, clippy
  pushed to pre-push). Document in this file.
- CI matrix grows past 6 jobs : reorganize into parallel workflows
  before adding more.
