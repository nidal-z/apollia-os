---
sidebar_position: 7
title: Repository layout
---

# Repository layout

This page explains what every top-level file and directory in the Apollia OS
repository is for, so a newcomer can orient quickly and a maintainer never has
to guess what an unfamiliar root file does. It is descriptive: it says what each
entry is, not how to change it.

Apollia OS is a Cargo workspace of Rust crates at its core, wrapped by a Python
SDK, generated API clients, example agents, a documentation site, and the usual
build, quality, and governance scaffolding. The sections below group the root
entries by role.

## Source directories

| Path | What it is |
|---|---|
| `crates/` | The Rust workspace. Each subdirectory is one crate with a single responsibility. Members and their shared dependency versions are declared in the root `Cargo.toml`. |
| `sdk/` | The Python SDK (`apollia` package, "AgentKit"): the `@agent` / `@skill` decorators, the `Ctx` type contract, and the minimal duck-typed interface an agent implements. Packaged with `pyproject.toml`. This is the kit for *authoring* agents that run *inside* the runtime. |
| `agents/` | Bundled agents. `agents/system/` holds the agents Apollia ships with (the onboarding agent, the in-app guide); `agents/examples/` holds a minimal `hello` sample. Treat these as illustrations, not canonical templates. |
| `clients/` | Typed API clients a *host application* uses to *drive* a running Apollia runtime over its HTTP API (submit tasks, stream results, read the audit trail). TypeScript (`clients/ts`) and Python (`clients/python`) clients are generated from `clients/openapi.json` via `clients/regen.sh`, so they cannot drift from the wire contract. This is the opposite direction from `sdk/`. |
| `tests/` | The workspace-level integration and end-to-end test crate (`apollia-e2e-tests`), plus CLI (`tests/cli/`) and Python (`tests/python/`) suites that exercise the runtime across crate boundaries. Per-crate unit tests live inside each crate instead. |
| `fuzz/` | The `cargo-fuzz` crate: libFuzzer targets (`fuzz/fuzz_targets/`) and their corpora (`fuzz/seeds/`). Built on its own with a nightly toolchain and excluded from the stable workspace, so the normal gates never try to compile it. |
| `packaging/` | Scripts and manifests that assemble a self-contained, relocatable Python runtime to bundle with the app (fetch a standalone interpreter, build the universal bundle, pin `requirements-bundled.txt`), plus platform launchers. Supports the zero-external-dependency principle. |
| `scripts/` | Developer and CI helper scripts: build wrappers (`build.sh`, `build.ps1`), data/cache reset utilities, the model-evaluation harness (`model-eval/`), and the desktop end-to-end automaton (`automation/`). |
| `docs/` | All documentation (detailed below). |

## Documentation (`docs/`)

| Path | What it is |
|---|---|
| `docs/site/` | The public documentation site (Docusaurus, English + French, Diataxis structure). This page lives here. |
| `docs/adr/` | Architecture Decision Records: numbered, append-only, English. The committed record of significant technical decisions and their rationale. |
| `docs/agents/` | The long-form rulebook for contributors (human and LLM): coding patterns, naming, testing, security, and the forbidden-practices list. English only. |
| `docs/diagrams/` | Source diagrams referenced by the documentation. |
| `docs/internal/` | Release planning and internal notes. Gitignored and never shipped; not referenced from any public file. |

## The Rust workspace (`crates/`)

The workspace is defined in the root `Cargo.toml`. The default build excludes
`apollia-desktop` (a heavy Tauri dependency, built explicitly) and two crates
built in isolation under special flags (`apollia-loom-models` and the top-level
`fuzz` crate). The main crates:

| Crate | Responsibility |
|---|---|
| `apollia-core` | Shared types and the public contract used by every other crate. |
| `apollia-runtime` | The event bus, the Tokio actor supervisor, and the axum HTTP API (Unix socket + TCP 7771). |
| `apollia-oria` | The ORIA execution engine: classify, plan, gate, execute, verify, with the enforced `StepBudget`. |
| `apollia-aip` | The PyO3 bridge that runs Python agents in-process (`Bound<'py, T>`, `pyo3-async-runtimes`). |
| `apollia-llm` | Local and cloud LLM routing (llama-cpp-2 for local GGUF inference; OpenAI-compatible and Anthropic cloud backends behind a feature). |
| `apollia-runner` | The out-of-process inference runner sidecar. |
| `apollia-stt` | Local speech-to-text (whisper). |
| `apollia-mcp` | The Model Context Protocol client (stdio / HTTP / SSE transports, untrusted-response caps). |
| `apollia-tools` | Built-in tool implementations. |
| `apollia-memory` | Agent memory persistence (SQLite + FTS5). |
| `apollia-permissions` | The permission model and gating. |
| `apollia-auth` | Authentication and OAuth2 (PKCE), secret storage via the OS keychain. |
| `apollia-connectors` | External service connectors. |
| `apollia-triggers` | Scheduled and event-driven triggers (cron, filesystem watch). |
| `apollia-notifications` | Notification delivery. |
| `apollia-workspace` | Workspace and filesystem sandboxing. |
| `apollia-prompts` | Centralized prompt templates. |
| `apollia-eval` | Agent evaluation harness. |
| `apollia-cli` | The `apollia` command-line binary (clap, noun-verb, exit codes 0-5). The only crate where `anyhow` and user-facing stdout are allowed. |
| `apollia-desktop` | The Tauri v2 + Svelte 5 desktop app (its UI lives in `crates/apollia-desktop/ui/`). |

## Governance and community files

| File | What it is |
|---|---|
| `README.md` | Project overview and entry point. |
| `AGENTS.md` | The standard entry point for LLM coding assistants; routes to the `docs/agents/` rulebook. |
| `CONTRIBUTING.md` | How to contribute (workflow, expectations). |
| `CODE_OF_CONDUCT.md` | Community conduct standards. |
| `GOVERNANCE.md` | How decisions are made and who maintains the project. |
| `SECURITY.md` | How to report a vulnerability and the supported-versions policy. |
| `ROADMAP.md` | Public direction and planned work. |
| `SPONSORS.md` | Funding and sponsorship information. |
| `CHANGELOG.md` | Human-readable record of notable changes per release. |
| `llm.txt` | A dense, accurate project description written for AI coding agents, so their context windows are used efficiently and they make fewer wrong assumptions. |

## Licensing

Apollia OS is dual-licensed under **MIT or Apache-2.0, at your option**. This is
the de facto standard across the Rust ecosystem (rustc, tokio, serde, axum,
Tauri) and maximises downstream compatibility: a consumer picks whichever
license fits their needs.

| File | What it is |
|---|---|
| `LICENSE` | A short index that states the dual license and points to the two full texts. It also confirms that contributions are dual-licensed under the same terms. It is not itself a license text. |
| `LICENSE-APACHE` | The full Apache License 2.0 text. |
| `LICENSE-MIT` | The full MIT License text. |

## Build and quality configuration

Each file below is read by a specific tool. The comments describe what it
actually configures in this repository.

| File | What it configures |
|---|---|
| `Cargo.toml` | The workspace root: crate members, shared dependency versions (each crate uses `{ workspace = true }` rather than inline versions), workspace lints (`unsafe_code = "deny"`, `unwrap_used = "deny"`), and the release/dev profiles. |
| `Cargo.lock` | The exact resolved dependency graph. Committed so every build and CI run compiles the identical versions. |
| `rust-toolchain.toml` | Pins the build toolchain to Rust `1.95.0` with `rustfmt`, `clippy`, `rust-src`, and `rust-analyzer`, so local and CI output match byte for byte. The declared MSRV floor (`rust-version = 1.89` in `Cargo.toml`) is lower and separate. |
| `clippy.toml` | Clippy thresholds: MSRV `1.89`, cognitive-complexity `30`, type-complexity `250`, at most `5` function arguments, and the `800`-line module limit. |
| `rustfmt.toml` | Formatting: edition 2021, `max_width = 100`, reordered imports, field-init and try shorthands, Unix newlines. |
| `deny.toml` | `cargo-deny` policy: an allowlist of acceptable licenses, denial of unknown registries, a warning on duplicate dependency versions, and a documented, per-release list of ignored security advisories (each with its lift condition). |
| `mutants.toml` | `cargo-mutants` (mutation testing) config: timeout multipliers, and exclusion of crates that do not build in isolation (`apollia-desktop`, `tests/`) and of build scripts. Dev-only test-quality tooling. |
| `Cross.toml` | `cross-rs` cross-compilation: `pre-build` steps that install the Linux system libraries (ALSA, PulseAudio, JACK, CMake, clang) into the build container for the x86_64 and aarch64 GNU/Linux targets. |
| `sonar-project.properties` | SonarQube analysis config (local Community Build, not part of the public release): source and test roots, exclusions, the imported Clippy report path, and documented per-rule exemptions. |
| `justfile` | `just` task recipes: the canonical commands for building, testing, linting, and running the desktop automaton. |
| `.pre-commit-config.yaml` | Pre-commit hooks: `ruff format`, `ruff check`, `rustfmt`, `clippy`, and `cargo check`. Not to be bypassed. |
| `.editorconfig` | Editor-agnostic whitespace rules: LF endings, final newline, and per-language indent sizes (4 for Rust/Python/TOML, 2 for web and Markdown). |
| `.python-version` | Pins the local Python interpreter (`3.13.7`) for tooling like `pyenv`. |
| `.mailmap` | Canonicalizes contributor name and email across git history. |
| `.cargo/` | Cargo defaults: `config.toml` sets build environment (macOS deployment target, an accelerated dev profile) and `audit.toml` holds the `cargo-audit` advisory ignores (mirrored from `deny.toml`). |
| `.github/` | GitHub configuration: CI, CodeQL, nightly, and release workflows (`.github/workflows/`), plus `CODEOWNERS`, issue and PR templates, `dependabot.yml`, and `FUNDING.yml`. |
| `.gitignore` | Paths git does not track (see below). |

## Local-only entries (gitignored, never shipped)

These appear in a working checkout but are not tracked, so they never reach the
public repository or a release artifact. Presence depends on what you have run
locally.

| Path | What it is |
|---|---|
| `CLAUDE.md` | Claude Code session overlay that imports the rulebook. Local-only. |
| `.claude/` | Claude Code project settings, skills, and worktrees. |
| `docs/internal/` | Release planning and internal notes. |
| `target/` | The Cargo build output directory. |
| `.venv/`, `.venv-agents/` | Local Python virtual environments. |
| `.apollia-automation/`, `.apollia-seed-home/` | Throwaway state produced by the desktop end-to-end automaton (a seeded, disposable `HOME` so the real `~/.apollia` is never touched). |
| `.pytest_cache/`, `.ruff_cache/` | Tool caches. |
| `.DS_Store` | macOS Finder metadata. |

`AGENTS.local.md` and `AGENTS.override.md` are also gitignored by convention
(per-machine and per-session contributor overrides); they may not exist in a
given checkout.
