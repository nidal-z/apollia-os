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
| `justfile` + `scripts/lib/*.sh` | the task recipes, grouped, and the shell functions they share; `.env.example` lists their settings, `justfile.local.example` shows a personal recipe file |
| `scripts/linux-check.sh` + `scripts/linux-check.Dockerfile` | ask the two Linux questions locally, in a container, before pushing: does it compile, do the suites pass |

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
fmt gate failed on every file. Edition stays at 2021; moving to 2024
rewrites `#stack-and-runtime` in
`docs/site/docs/architecture/08-decisions.md` and takes its own PR.

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

`.pre-commit-config.yaml` is the source; read it there. What it runs at commit
time, in one line: the usual hygiene hooks plus `detect-private-key` and a
500 KB file cap, `ruff-check` and `ruff-format` on `sdk/`, `rustfmt` and
`cargo check` on the workspace, `check-prose` on the five prose rules,
`docs-site-build` when `docs/site/` or `clients/openapi.json` changes, and a
subset of the guard scripts of `scripts/`.

Which subset, and how large, is a moving number, and this document does not
carry a copy of it. It carried one for months and the copy went stale in both
directions at once, understating the hook and understating the recipe. Count it
instead :

```sh
grep -oE 'python3 scripts/check_[a-z_]+\.py' .pre-commit-config.yaml | sort -u | wc -l
just --show guards
```

The shape does not move. Two guards whose subject is the whole repository
(`check-claim-anchors`, `check-claims`) carry no filter; every other hook entry
is filtered on the roots its own source declares, and every filter also names
the guard's own file, so a change to a guard cannot escape it. A filter decides
when a defect is seen first, never whether it is seen: `just guards` runs the
whole corpus unfiltered and reports every red one, and `just ci` starts with it.

A guard stays out of the hook for one of three reasons, and the third is not a
decision. It needs a built tree, so a hook entry would refuse the first commit
of a contributor who has not run `npm ci` or `cargo build`
(`check_guard_verdicts.py`, `check_instrument_verdicts.py`). Or it costs more
than the commits it would sit on (`check_no_font_cdn.py`, `check_selftest.py`;
`check_panic_free.py` re-reads every production Rust file whatever the diff
holds, 2.7 seconds on three runs of this tree, and a filter on `crates/` would
not lower that on the commits that touch Rust). Or nobody has put it in, which
is a state, not a measurement.

Two entries are not commit-time hooks, and reading the list as one is how a
contributor gets surprised: `clippy -D warnings` is staged on `pre-push`, and
`conventional-pre-commit` judges the message at `commit-msg`.

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
├── weekly.yml        # heavy / advisory: e2e, feature-matrix, coverage HTML,
│                     #   deep-audit, geiger (unsafe surface), loom, miri, kani, fuzz-deep
├── release.yml       # tag-triggered: build binaries, attach to release
├── engines.yml       # manual: sign and publish the engine channel
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
9. `deny` : `cargo-deny check advisories bans licenses sources`
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
  session is `fuzz-deep` in `weekly.yml`. See `docs/agents/TESTING.md` 8b.

`python-tests` is **blocking**. It was advisory while three SDK tests failed;
they are fixed, so it gates.

Weekly-only advisory jobs (in `weekly.yml`) :

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
  published file, emitting one Sigstore bundle (`.cosign.bundle`) that carries
  the signature and the signing certificate together. The suffix is not
  cosmetic: an output written as `.sig` lands on the updater signatures the
  contract declares, and `check_release_artifacts.py` refuses that.
  No long-lived key: the signing identity is the release workflow.
- **Provenance** : `actions/attest-build-provenance` produces a SLSA
  build-provenance attestation over the binaries and SBOMs.

Signing and attestation are confined to the final `release` job, the only job
that escalates the token to `id-token: write` + `attestations: write` (plus the
`contents: write` needed to publish). Every workflow defaults to
`permissions: contents: read` at the top level; jobs escalate only what they
need. Verification commands for consumers live in `SECURITY.md`.

### Engine channel (`engines.yml`)

The engines the application downloads after install (the CUDA `llama-server`,
Node.js) are not built by `release.yml` and do not follow its tags. `engines.yml`
is started by hand with a channel and a llama.cpp tag, and runs
`packaging/build-engine-manifest.py`: every upstream archive is checked against
its SHA-256 pinned in `packaging/engine-sources.json`, each package is
repackaged as one zip, and the manifest is signed in the minisign format by
`cargo tauri signer sign` with the distribution key.

- **Two keys, never mixed.** `APOLLIA_DIST_SIGNING_KEY` signs distribution
  metadata; `TAURI_SIGNING_PRIVATE_KEY` signs application updates and is not
  read by this workflow. The script refuses a run where both hold one value.
- **The trigger is the fence.** The distribution and object-storage secrets sit
  at repository level, so the workflow runs on `workflow_dispatch` only;
  `check_ci_workflows.py` (rule `dist-secrets`) refuses a pull-request trigger
  on any job that names them.
- **Proved before signed.** `just test-engine-manifest` runs the script on
  stand-in archives with a throwaway key and hands the output to the engine
  store; the workflow runs it first, then verifies the real manifest against
  the public key compiled into the tree before anything is uploaded.
- **Adding a tag is a commit.** A tag is publishable once its archives are
  pinned in `engine-sources.json`; `check_download_sums.py` holds every source
  to an https address and a 64-hex sum.

---

## 6. Coverage

The `coverage` job in `ci.yml` enforces a line-coverage floor :

```sh
cargo llvm-cov --workspace --lcov --output-path lcov.info \
  --fail-under-lines $COVERAGE_FLOOR
```

`COVERAGE_FLOOR` is a workflow-level env var set just below the measured
baseline so the gate is green from day one, then ratcheted up over time and
never lowered. The full HTML report is produced by the weekly `coverage` job.

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

# Which tools the recipes need, present and absent (exit 2 when one is missing)
just doctor
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
workspace member count read from `cargo metadata`, the parallelism it used and
where that number came from, and what it does not cover (the other Linux triple,
both Windows triples, and the feature presets). Requires a running Docker
daemon; without one, read the verdict of the `Clippy` job of
`.github/workflows/ci.yml` on a pushed branch instead.

Disk, measured with `docker system df -v` on an Apple Silicon host on
2026-08-19. An architecture that has only ever been asked the compile question
holds a 3.8 GB image, a 392 MB registry volume and a 7.58 GB target volume, so
about 12 GB. The test question shares that same target volume and grows it: the
architecture that has been asked it holds 43.75 GB of targets, so budget about
48 GB for it.

### Ask the Linux test question too, it is a different question

`linux-check` answers whether the tree compiles. It links no test binary, so a
tree can compile on Linux and still fail its suites there, and until August 2026
the only run that ever asked the second question was a `docker run` typed by
hand: a mount, two volumes and an environment variable that lived in no tracked
file, so nobody could replay it.

```sh
just linux-test               # aarch64-unknown-linux-gnu, native, measured
just linux-test x86           # x86_64-unknown-linux-gnu, emulated, cost unmeasured
```

It runs `cargo test --workspace --no-fail-fast --locked`, the first of the two
steps of the `Rust Tests` job of `.github/workflows/ci.yml:143-146`, in the same
container, on the same read-only mount, sharing the same target volume as
`linux-check`. The default target differs from `linux-check` on purpose: this
question is the slow one, and `aarch64` is the only one that has been measured.

**It sets its own parallelism, and that is the point.** Left at the container's
default of 24 jobs for 7.65 GiB, linking the test binaries dies on
`collect2: fatal error: ld terminated with signal 9 [Killed]`. The script reads
the container's core count and `MemTotal` in the same probe that reads the
triple, then uses one job per 3 GiB capped by the core count, and passes it as
`CARGO_BUILD_JOBS`. Nothing has to be exported by the caller, and the perimeter
block prints the value together with the two numbers it came from, so the same
line read on another machine says where its own number came from.

The three exit codes are the same three, read for this question: 0 the suites
passed, 1 they failed or the tree did not compile, 2 nothing was measured. On
green and on red alike the last line carries the counts extracted from the run
by `scripts/worktree_verdicts.py`, in the form `exit 101, 78 bin, 4379 tst`, and
the word `not measured` where a count could not be extracted, never `0`.

What it does not cover, beyond what `linux-check` already does not cover:
`cargo test -p apollia-e2e-tests --features python-tests`, the second step of the
same job. That suite declares it needs the SDK installed in the interpreter
(the header of `tests/integration/test_package_tools.rs`), the image is built without a
context so it cannot install it, and the tree is mounted read-only. And no run of
this channel confronts the image's system packages with the runner's, so a green
here is not a promise of a green there. Both facts are printed by the run itself.

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

Resolving it is not the same as trusting it. The suite, and the three guards
that read the same artefact (`check_cli_json_contract.py`,
`check_cli_e2e_coverage.py`, `check_entry_doc_commands.py`), first ask
`scripts/binary_freshness.py` whether cargo produced it from this working
tree, by reading the dep-info cargo writes beside it. When it did not, the
four answer 2, nothing measured, and print `cargo build -p apollia-cli --bin
apollia-os`. They never answer 1: an artefact from another tree, or from
before the last edit, is not a defect of the tree, and five verdicts of one
release campaign said it was.

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

### The task recipes, and why they do what they do

`just` with no argument lists the recipes by group: `build`, `test`,
`quality`, `docs`, `desktop`, `llm`, `automation`, `release`, `linux`,
`maintainer`. The names are regular (`build-*`, `test-*`, `automation-*`,
`release-*`), and the justfile itself stays a table of contents: one line of
documentation per recipe, the shared shell in `scripts/lib/`, and the reasons
here.

**Settings.** Every setting is an environment variable declared once at the
head of the justfile with its default, and listed in `.env.example`. `just`
loads a `.env` file at the repository root, and a variable exported in the
shell wins over it. No recipe names a model or a path of one machine:
a recipe that needs a model reads `APOLLIA_LLAMA_MODEL` (or its argument) and
answers one message when there is none. Recipes of your own go in
`justfile.local`, which is untracked and imported when present.

**The Python bundle** (`scripts/lib/python-bundle.sh`). The application sets
`PYTHONHOME` to the interpreter bundle under `target/`. A build that links
whatever `libpython` the host resolves therefore runs two different CPython
builds, the interpreter cannot find its own standard library, and every agent
fails at boot with `ModuleNotFoundError: No module named '_opcode'`, while the
application itself starts. `python_bundle_activate` points `PYO3_PYTHON` and
`RUSTFLAGS` at the bundle before any recipe compiles the desktop or the CLI.
Three details ride along:

- the bundle is relocatable but its `sysconfig` still names its build path, so
  the library directory is passed with `-L`, as the release workflow does;
- on Linux an RPATH is added, because `-L` informs the linker and not the
  loader: without it a locally built binary starts only through a launcher
  that exports `LD_LIBRARY_PATH`, and the CLI suite reports every assertion
  red on a binary that could not load;
- on macOS `target/Resources/python` is linked to the bundle, because the
  install name is `@executable_path/../Resources/python/lib`, the packaged
  layout, and dyld resolves it before the process can correct anything.

A tree with no bundle is refused with exit 1 and the command that builds one.
A warning was not enough: it scrolls past during compilation, and the run that
follows spends its time on agents that cannot load. `build-cli` goes through
the same function because PyO3 refuses an interpreter newer than 3.13, which
is what a current distribution ships, while the bundle carries 3.13.

**The STT runners.** `build-runner` and `build-runner-release` source
`host_runner_cmake_env`: on MSVC the `cmake` crate strips the optimisation
flags of whisper.cpp unless the caller sets them, and a runner built without
them transcribes about fifty times slower than real time. It is a no-op on
every other host. `build-runner` also leaves a copy named after its backend,
which is how the daemon detects it, and signs that copy ad hoc on macOS, where
a copied binary loses the linker's signature and is killed at launch.
`build-runners` builds `cpu` everywhere and `metal` on macOS alone, the Metal
backend existing on no other system.

**The desktop bundle.** `build-desktop` with no argument builds for the host,
into `target/release`; with a triple it builds into `target/<triple>/release`.
The runner set and the engine default to what the host can run, so a clone on
Windows or Linux never fetches a macOS interpreter or a Metal engine. The
release presets take the runner set as their one argument.

**llama-server** (`scripts/lib/llama.sh`). The server is started with
`--jinja`, so tool calling is driven by the model's template; without it the
server falls back on a tool grammar it may fail to parse. `-c` is the total
context, split across the `-np` slots, so one slot gives a single conversation
the whole window at no extra memory. The automation recipes default to a
131072-token context, greedy sampling and a fixed seed: a model book asserts
what the model does next, and eight slots over 16384 tokens left 2048 tokens
per slot, which the server answers with a 400. A model file under a megabyte
is refused by name: it is one of the placeholders the automation seed
installs, and llama-server would otherwise die on `unknown model
architecture`, which reads as a corrupt model.

**The desktop automaton** (`scripts/lib/automation.sh`). `automation-seeded`
rebuilds a throwaway `HOME` from `tests/cli/seed` and swaps `HOME` and
`USERPROFILE` for the application alone; `CARGO_HOME` and `RUSTUP_HOME` are
pinned first, since their defaults derive from the real home. The narrative
overlay is stripped for the assertion recipes, because it changes the row
counts they assert on, and kept by `automation-screenshots`, where an empty
timeline is the defect. A seeded run points the MCP registry at a closed port,
so the catalogue answers from the seeded cache: that cache goes stale after
fifteen minutes, and a long book would otherwise meet the public registry
mid-run. `automation-first-run` builds a blank home holding only the two model
files, cloned rather than linked, so the run measures a first launch and not a
download. `automation-real` and `automation-real-llama` drive your real
profile and ask for confirmation first. `scripts/lib/selftest.sh` holds these
choices with every external command stubbed, and `guards` launches it.

**The guards.** `guards` rebuilds `target/debug/apollia-os` before judging:
five guards interrogate the binary rather than the source, and on a stale
binary one turns red while the others have nothing to measure, which reads as
a broken tree. Guards are named one by one, never globbed, because
`scripts/check_selftest.py` looks each name up in the files that launch it.
The recipe answers the three codes its guards do: 0 when everything measured
and everything is green, 1 as soon as one guard is red, 2 when nothing is red
but at least one guard measured nothing. Reds accumulate instead of stopping
the run, and skips are named at the end. An external gate whose tool is not
installed answers the shell's 127 and counts as a skip; a missing cargo
subcommand answers 101 and still reads red, cargo giving no way to tell it
from a subcommand that found something. clippy runs first among the external
gates because it is the one that can hide the others: it stops at the first
crate that fails to compile.

`ci` is `guards`, the format check, then `test`. clippy runs once, inside
`guards`; `lint` remains the quick pair of format check and clippy.
`check-features` builds the three configurations the default features hide,
which no other command compiles. `audit-deps` audits the bundled Python
packages, the desktop UI runtime dependencies and the Rust crates against
their upstream advisory databases, and answers 2 offline.

**The test recipes.** `test` wraps the workspace suite in a sentinel `HOME`
and fails if the run wrote to `~/.apollia`. `test-cli-runtime` points the
model variable at a file that does not exist, so Track 3 records a justified
skip whatever the environment holds. `test-engine-manifest` runs the engine
manifest script on stand-in archives, signs them with a key pair generated on
the spot and hands the result to the engine store: accepted intact, refused
after one altered byte. `test-cli-sweep` plays one real invocation of every
CLI leaf on a throwaway `HOME` and names the leaves it refuses to play.
`test-evals-load` loads the two agent evaluation suites and holds their
invariants; running them needs a loaded model and an outfitted agent, which no
recipe provides.

---

### The desktop in dev mode and the macOS keychain

macOS ties each keychain entry to the signature of the program that wrote it.
An unsigned dev binary carries a new signature after every build, so the
keychain asks for its password again, once per entry the application reads at
startup, and "Always allow" lasts until the next build of the Rust side.

`just desktop-dev-identity` makes a self-signed code-signing identity named
`Apollia Dev` in the login keychain, once. From then on `just desktop-dev` and
`just desktop-dev-llama` build through `scripts/lib/dev-sign-cargo.sh`, which
signs the binary with that identity under one identifier before running it:
every build is the same program to the keychain. `APOLLIA_DEV_SIGN_IDENTITY`
names another identity. Without any, the recipes run as before. The identity
signs nothing that ships: release builds are signed by the release workflow.

## 8. When the rules block you

- A lint flags legitimate code : add `#[allow(clippy::<lint>)]` with an
  inline justification. Never `--allow` blanket on the command line.
- A pre-commit hook is too slow : split the hook (formatter fast, clippy
  pushed to pre-push). Document in this file.
- CI matrix grows past 6 jobs : reorganize into parallel workflows
  before adding more.
