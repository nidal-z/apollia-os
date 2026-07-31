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

---

## 2. `.editorconfig`

```ini
root = true

[*]
end_of_line = lf
insert_final_newline = true
trim_trailing_whitespace = true
charset = utf-8
indent_style = space

[*.{rs,toml}]
indent_size = 4

[*.{py,pyi}]
indent_size = 4

[*.{ts,tsx,js,svelte,json,jsonc,html,css}]
indent_size = 2

[*.md]
trim_trailing_whitespace = false  # markdown line breaks need trailing spaces
indent_size = 2

[Makefile]
indent_style = tab
```

---

## 3. `rustfmt.toml`

```toml
edition = "2021"
max_width = 100
use_small_heuristics = "Default"
reorder_imports = true
use_field_init_shorthand = true
use_try_shorthand = true
newline_style = "Unix"
```

Run : `cargo fmt --check` in CI, `cargo fmt` to apply.

`imports_granularity` and `group_imports` are deliberately absent : both are
nightly-only rustfmt options. Keeping them forced the whole tree to be
formatted with a nightly rustfmt, which stable CI could never reproduce (the
fmt gate then failed on every file). The gate now runs on the pinned stable
toolchain and the tree is formatted to match it.

Edition 2021 reflects the current workspace state. Migration to edition
2024 is a planned follow-up (separate ADR + dedicated PR).

---

## 4. `clippy.toml`

```toml
msrv = "1.89"
cognitive-complexity-threshold = 30
type-complexity-threshold = 250
too-many-arguments-threshold = 5
```

Run : `cargo clippy --workspace --all-targets -- -D warnings`.

---

## 5. `rust-toolchain.toml`

```toml
[toolchain]
channel = "1.95.0"
components = ["rustfmt", "clippy", "rust-src", "rust-analyzer"]
profile = "minimal"
```

Committed for reproducibility. This is the exact-pinned **build / gate
toolchain**, a recent stable. Every blocking Rust job in `ci.yml` installs the
same version via `dtolnay/rust-toolchain@1.95.0` (never `@stable`), so CI fmt
and clippy output match local dev byte for byte and cannot drift against
rolling stable. A separate `clippy-stable` job runs on `@stable` as a
non-blocking advisory to surface lints coming in a future toolchain.

Toolchain policy, two distinct numbers :

- **Build / gate toolchain** = `1.95.0` (this file). What CI and local dev
  actually compile with.
- **MSRV floor** = `1.89` (`Cargo.toml` `rust-version`, `clippy.toml` `msrv`).
  The real minimum the dependency tree supports (notify-rust 4.18 requires
  1.89; time / serde_with / image require 1.88). It is a declared floor, not
  the version the gate runs on.

Local dev : with `rustup` installed, `cargo` inside the repo auto-selects the
pinned `1.95.0` from this file. Without `rustup` (e.g. a Homebrew-only Rust),
keep the local toolchain at the pinned version to avoid fmt / clippy drift.

---

## 6. `[workspace.lints]` in root `Cargo.toml` (target state)

Not yet applied to the workspace. Promotion path :

1. Add the block in a dedicated PR.
2. Start with everything at `warn`.
3. Run `cargo clippy --workspace --all-targets` and inventory current
   warnings.
4. Per warning category, either fix the sites or downgrade the lint to
   `warn` permanently with a documented reason.
5. Promote to `deny` once the workspace is clean for that category.

Target block once the audit is done :

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"
unreachable_pub = "warn"
unused_must_use = "deny"

[workspace.lints.clippy]
all = "deny"
correctness = "deny"
suspicious = "deny"
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"
dbg_macro = "deny"
missing_errors_doc = "warn"
missing_panics_doc = "warn"
pedantic = "warn"
```

Each crate inherits via `[lints] workspace = true` in its own
`Cargo.toml`. Pedantic allow-list is per-crate, justified inline.

`unsafe_code = "forbid"` is workspace-wide. A crate that genuinely needs
unsafe overrides with `[lints.rust] unsafe_code = "deny"` plus a
top-of-crate explanation.

Until promotion : the rules in `docs/agents/FORBIDDEN.md` and
`docs/agents/RUST-PATTERNS.md` are the policy. Reviewers enforce them.

---

## 7. Ruff config (in `sdk/pyproject.toml`)

```toml
[tool.ruff]
target-version = "py312"
line-length = 100
src = ["apollia", "tests"]

[tool.ruff.lint]
select = [
    "E", "F", "W",       # pycodestyle + pyflakes
    "I",                  # isort
    "B",                  # bugbear
    "UP",                 # pyupgrade
    "RUF",                # ruff-specific
    "S",                  # bandit (security)
    "SIM",                # simplify
    "ANN",                # annotations
    "ASYNC",              # async-specific
    "TCH",                # type-checking
    "PT",                 # pytest
    "D",                  # pydocstyle (google)
]
ignore = [
    "D100", "D104",       # missing module/package docstring
    "ANN101", "ANN102",   # self/cls annotation
    "COM812",             # trailing comma (conflicts with formatter)
    "ISC001",             # implicit string concat (conflicts with formatter)
]

[tool.ruff.lint.per-file-ignores]
"tests/**/*.py" = ["S101", "D", "ANN"]

[tool.ruff.lint.pydocstyle]
convention = "google"

[tool.ruff.format]
quote-style = "double"
indent-style = "space"
line-ending = "lf"
```

Run : `ruff format --check` + `ruff check` in CI. `ruff format` + `ruff
check --fix` to apply.

This is a curated subset rather than `select = ["ALL"]` to keep
warnings actionable. Expand when the codebase is clean for the current
set.

---

## 8. Pyright config (in `sdk/pyproject.toml`)

```toml
[tool.pyright]
include = ["apollia"]
pythonVersion = "3.12"
strict = ["apollia/**"]
reportMissingImports = "error"
reportUnusedImport = "warning"
reportMissingTypeStubs = "warning"
```

Run : `pyright sdk/apollia`.

`mypy` is acceptable as a secondary checker but `pyright` is the gate.

---

## 9. pytest config (in `pyproject.toml`)

```toml
[tool.pytest.ini_options]
asyncio_mode = "strict"
testpaths = ["sdk/tests", "agents"]
addopts = [
  "--strict-markers",
  "--strict-config",
  "-ra",
]
markers = [
  "unit: fast unit test",
  "integration: integration test, may touch the filesystem",
  "slow: skipped unless --run-slow is passed",
]
```

---

## 10. Pre-commit hooks (`.pre-commit-config.yaml`)

```yaml
repos:
  - repo: https://github.com/pre-commit/pre-commit-hooks
    rev: v4.6.0
    hooks:
      - id: trailing-whitespace
      - id: end-of-file-fixer
      - id: check-merge-conflict
      - id: check-yaml
      - id: check-toml
      - id: check-added-large-files
        args: ["--maxkb=500"]
      - id: detect-private-key

  - repo: https://github.com/astral-sh/ruff-pre-commit
    rev: v0.5.0
    hooks:
      - id: ruff
        args: ["--fix"]
      - id: ruff-format

  - repo: local
    hooks:
      - id: rustfmt
        name: rustfmt
        entry: cargo fmt --
        language: system
        types: [rust]

      - id: clippy
        name: clippy
        entry: cargo clippy --workspace --all-targets -- -D warnings
        language: system
        types: [rust]
        pass_filenames: false

      - id: cargo-check
        name: cargo check
        entry: cargo check --workspace
        language: system
        types: [rust]
        pass_filenames: false

  - repo: https://github.com/compilerla/conventional-pre-commit
    rev: v3.4.0
    hooks:
      - id: conventional-pre-commit
        stages: [commit-msg]
        args: []
```

Install : `pre-commit install && pre-commit install --hook-type
commit-msg`.

Run on demand : `pre-commit run --all-files`.

Never `--no-verify`. If a hook fails, fix the underlying issue.

---

## 11. CI pipeline (GitHub Actions)

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
3. `test` : `cargo test --workspace` (+ python-tests)
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

## 12. Coverage

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

## 13. Local setup quickstart

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
cargo test --workspace
```

---

## 14. When the rules block you

- A lint flags legitimate code : add `#[allow(clippy::<lint>)]` with an
  inline justification. Never `--allow` blanket on the command line.
- A pre-commit hook is too slow : split the hook (formatter fast, clippy
  pushed to pre-push). Document in this file.
- CI matrix grows past 6 jobs : reorganize into parallel workflows
  before adding more.
