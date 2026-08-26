# AGENTS.md

Apollia OS is a sovereign Rust runtime for autonomous AI agents. It runs any
Python agent (LangGraph, CrewAI, custom) in isolation, locally, with tools, and
without a cloud dependency.

This file is the standard entry point for LLM coding assistants (Codex, Claude
Code, Cursor, Gemini CLI, Aider, Continue, Windsurf, GitHub Copilot, and
others). It is the short brief; the long-form rulebook is `docs/agents/`.

---

## Why these invariants exist

Apollia is a large, fast-moving codebase revised by many hands. The
invariants below exist to preserve coherence across a high volume of
changes. Follow them. When they conflict with your task, surface the
conflict instead of silently bending the rule. The escalation process is at the
bottom of this file.

---

## Tech stack

| Layer | Tech |
|---|---|
| Async runtime | Rust 1.89+ , Tokio 1.x |
| Python bridge | PyO3 0.24 , pyo3-async-runtimes |
| Local LLM | embedded llama-server (upstream llama.cpp; GGUF, Metal, CUDA, OpenAI-compatible HTTP, `--jinja`) |
| Local STT | whisper-rs (GGML) |
| Persistence | SQLite + rusqlite + FTS5 (WAL mode) |
| HTTP API | axum on Unix socket + TCP 7771 |
| CLI | clap v4 derive |
| Desktop | Tauri v2 + Svelte 5 + Tailwind 3.4 |
| SDK | `apollia` package (AgentKit). Source floor Python 3.10 (`sdk/pyproject.toml` `requires-python`); the runtime embeds a 3.12+ interpreter, which is the supported configuration |

---

## Build and test commands

Use file-scoped commands first. Reach for the workspace-wide ones only after.

```sh
# Scoped (preferred while iterating)
cargo test -p apollia-<crate> <test_name>
cargo clippy -p apollia-<crate> -- -D warnings

# Full sweep (run it yourself before a commit; CI runs the same three)
cargo test --workspace --no-fail-fast
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check

# Docs
cd docs/site && npm run build

# CLI end-to-end (Track 1 offline, Tracks 2 and 3 opt-in)
bash tests/cli/cli-e2e.sh
```

The CLI end-to-end suite runs against a deterministically seeded, throwaway
`HOME` built by `tests/cli/seed/build-seed.sh`, so a run never writes to
the real `~/.apollia`; when `APOLLIA_TEST_MODEL_GGUF` is unset, Track 3 reads
one file from it (the default model GGUF), read-only. Track 1 is offline and
always runs; Tracks 2 and 3 are
gated by `APOLLIA_REQUIRE_RUNTIME` and `APOLLIA_TEST_MODEL_GGUF`. Read
`tests/cli/README.md` before changing a track or the fixture: the suite asserts
the seed's exact row counts, so a fixture change is a suite change.

`--all-features` is not part of that sweep, and adding it does not work:
`apollia-runner` exposes `local-cuda`, `local-metal` and `local-rocm`, which
turn on three mutually exclusive `whisper-rs` backends at once. CI omits it for
the same reason (`ci.yml`), and the feature matrix is exercised in
`nightly.yml`.

The desktop UI is covered by Vitest unit tests and by Playwright suites under
`crates/apollia-desktop/ui/tests/`. There is no WebDriver for WKWebView on
macOS, so nothing drives the packaged bundle; a dev build of the real
application is driven by the gestural automaton in `scripts/automation/`, which
addresses the interface by `data-testid` against a seeded throwaway `HOME` and
is tree-shaken out of release builds. Read `scripts/automation/README.md`
before touching a recipe. The runtime paths behind the UI are also covered
through the CLI suite above.

Pre-commit hooks guard every commit, and `.pre-commit-config.yaml` is the list
to read rather than a copy to trust: it holds more than formatting and lints,
the prose rules and the documentation-site build among them. Two of its entries
run elsewhere, `clippy` on push and the commit-message convention at
`commit-msg`. What it does **not** hold is the test suite: the Rust entry is
`cargo check --workspace`, so a green hook run says nothing about
`cargo test`. Do not bypass any of them.

---

## The 8 non-negotiable principles

1. **Local-first** : zero user data leaves the machine without an explicit action.
2. **Zero external dependency** : the binary runs on any clean Linux without
   prior install.
3. **Minimal contract** : a class decorated with `@agent`, one `@skill` or one
   `@on_message` async method, and nothing else. The legacy `manifest()` plus
   `run()` escape hatch is gone: the bridge refuses an object without
   `__apollia_dispatch__`.
4. **Fail fast** : any startup-detectable error is detected at startup.
5. **One actor, one responsibility** : Tokio actor pattern, no shared state
   between actors.
6. **Memory at agent initiative** : never inject memory context into an agent's
   prompt. Three exceptions, all unreachable from an agent execution path: two
   inside the built-in conversational assistant (a user-persona brief at the
   `long_autonomous` tier, past session summaries on the first message of a
   free chat), and one in the desktop prompt-rewrite command (the user profile
   work context). See `docs/site/docs/explanation/the-8-principles.md`.
7. **Non-negotiable safeguards** : `StepBudget` enforced by the runtime, never
   bypassable.
8. **Human CLI, machine API** : `--json` global, TTY auto-detected.

Source : `docs/site/docs/explanation/the-8-principles.md`. Detailed rationale and worked
examples in `docs/agents/ARCHITECTURE.md`.

---

## Where to read next

| Your task | Read |
|---|---|
| Any Rust change | `docs/agents/RUST-PATTERNS.md` + nearest crate `AGENTS.md` |
| Any Python change | `docs/agents/PYTHON-PATTERNS.md` + `sdk/AGENTS.md` |
| Desktop UI change | `crates/apollia-desktop/ui/AGENTS.md` |
| CLI change | `docs/agents/RUST-PATTERNS.md` + `crates/apollia-cli/AGENTS.md` |
| Writing tests | `docs/agents/TESTING.md` |
| Writing a commit | `docs/agents/COMMITS.md` |
| Writing documentation | `docs/agents/DOCS-WRITING.md` |
| Anything involving secrets, audit, or external calls | `docs/agents/SECURITY.md` |
| Anything that emits events or logs | `docs/agents/OBSERVABILITY.md` |
| Naming something new | `docs/agents/NAMING.md` |
| Setting up local tooling | `docs/agents/CI-TOOLING.md` |

Always cross-check against `docs/agents/FORBIDDEN.md` before committing.

---

## ALWAYS

- Use `thiserror` for errors in libraries. `anyhow` is allowed only in
  `apollia-cli` `main()`.
- Use `tracing::event!(Level::*, field = %val, "domain.action")`. No format
  strings in logs.
- Use bounded `mpsc::channel` between actors. Never `Arc<Mutex<T>>` across
  actors.
- Use `TypedDict` for agent payload schemas. Never `from __future__ import
  annotations` in those modules.
- Use absolute Python imports (`from apollia.foo import bar`).
- Run `cargo test --workspace --no-fail-fast` before `git commit`. No hook does
  it for you. Without the flag cargo stops at the first failing test binary, so a
  single red test silently hides every test that would have run after it.
- Write tests in GIVEN / WHEN / THEN structure. `scripts/check_rust_tests.py`
  holds it on a descending ratchet: a new test without the three markers fails
  the guard, and the backlog of older ones can only shrink.

## ASK FIRST

- Adding any third-party dependency (Cargo or Python). Each one is a
  sovereignty surface, and the decision is stated in
  `docs/site/docs/architecture/08-decisions.md` before it lands.
- Changing a decision recorded in the architecture chapter of `docs/site/`.
- Modifying a public API in `apollia-core` (used by every other crate).
- Touching anything in `docs/internal/` (gitignored, source of truth for
  release planning).
- Force-pushing, rewriting history, deleting branches, or anything that
  rewrites shared state.

## NEVER

- `anyhow`, `unwrap()`, `expect()`, `todo!()`, `panic!()`, `println!`, `dbg!`
  in production Rust.
- `from __future__ import annotations` in any module with `TypedDict`.
- Relative imports in Python (`from .module import X`).
- em-dash `—` in any prose, comment, or documentation file.
- Any co-author trailer in a commit message. One commit, one author.
- Mixing French and English in the same file.
- AI stock phrases ("as an AI", "it's important to note", "il convient de
  noter", etc.).
- Committing with a failing `cargo test --workspace --no-fail-fast`.

Full list with reasons : `docs/agents/FORBIDDEN.md`.

---

## Sub-`AGENTS.md` in the repo

A sub-`AGENTS.md` applies to its subtree and overrides this file where they
conflict. The nearest one wins.

| Path | Scope |
|---|---|
| `crates/apollia-cli/AGENTS.md` | CLI binary, noun-verb taxonomy, exit codes 0-5 |
| `crates/apollia-aip/AGENTS.md` | PyO3 bridge, `Bound<'py, T>`, `pyo3-async-runtimes` |
| `crates/apollia-mcp/AGENTS.md` | MCP client transports, untrusted-response byte cap |
| `crates/apollia-oria/AGENTS.md` | ORIA engine, StepBudget, ResilienceLayer, plan cache |
| `crates/apollia-runtime/AGENTS.md` | EventBus, actor supervisor, axum API |
| `crates/apollia-desktop/ui/AGENTS.md` | Tauri v2 + Svelte 5, design system, i18n |
| `sdk/AGENTS.md` | Apollia AgentKit decorators, schemas, minimal contract |

Create a new one when a subtree passes ~500 lines of code AND introduces
patterns the global rules do not cover. Document the trigger in its header.

---

## When a rule blocks you

Rules are negotiable. Silent violations are not. Three options, in order:

1. Document an exemption inline (`// SAFETY:`, `# REASON:`).
2. Surface the conflict and propose a rule update.
3. Change the decisions chapter of the documentation site, in the same commit
   as the code, if the conflict reflects a real architectural shift.

Never circumvent a rule by restating it in vaguer terms.

---

## Overrides

- `AGENTS.local.md` (gitignored) : per-machine, per-user preferences.
- `AGENTS.override.md` (gitignored) : temporary session override. Document
  reason at the top.

Precedence (highest first) : chat prompt > `AGENTS.override.md` > nearest
`AGENTS.md` climbing up the tree > this file.

---

Before writing code, read the sub-`AGENTS.md` for the area you touch. The cost
of reading is 2 minutes. The cost of an unaligned commit is one PR cycle.
