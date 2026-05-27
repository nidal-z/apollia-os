# AGENTS.md

Apollia OS is a sovereign Rust runtime for autonomous AI agents. It runs any
Python agent (LangGraph, CrewAI, custom) in isolation, locally, with tools, and
without a cloud dependency.

This file is the standard entry point for LLM coding assistants (Codex, Claude
Code, Cursor, Gemini CLI, Aider, Continue, Windsurf, GitHub Copilot, and
others). It briefs you in ~120 lines and points you to the long-form rulebook.

---

## This repo is 100% AI-coded

Every file here was authored or revised by an AI assistant. The invariants
below exist to preserve coherence across thousands of such revisions. Follow
them. When they conflict with your task, surface the conflict instead of
silently bending the rule. See `docs/agents/INDEX.md` for the escalation
process.

---

## Tech stack

| Layer | Tech |
|---|---|
| Async runtime | Rust 1.85+ , Tokio 1.x |
| Python bridge | PyO3 0.24 , pyo3-async-runtimes |
| Local LLM | llama-cpp-2 (GGUF, Metal, CUDA) |
| Local STT | whisper-rs (GGML) |
| Persistence | SQLite + rusqlite + FTS5 (WAL mode) |
| HTTP API | axum on Unix socket + TCP 7771 |
| CLI | clap v4 derive |
| Desktop | Tauri v2 + Svelte 5 + Tailwind 3.4 |
| SDK | Python 3.12+ , `apollia` package (AgentKit) |

---

## Build and test commands

Use file-scoped commands first. Reach for the workspace-wide ones only after.

```sh
# Scoped (preferred while iterating)
cargo test -p apollia-<crate> <test_name>
cargo clippy -p apollia-<crate> -- -D warnings

# Full sweep (before commit and in CI)
cargo test --workspace --all-features
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check

# Docs
mdbook build docs/book/

# CLI end-to-end (Phase A local, Phase B opt-in cloud)
bash tests/cli/cli-e2e.sh
```

Pre-commit hooks run `ruff format`, `ruff check`, `rustfmt`, `clippy`, and
`cargo check`. Do not bypass them.

---

## The 8 non-negotiable principles

1. **Local-first** : zero user data leaves the machine without an explicit action.
2. **Zero external dependency** : the binary runs on any clean Linux without
   prior install.
3. **Minimal contract** : Python duck typing, `manifest()` + `run()` async is
   enough.
4. **Fail fast** : any startup-detectable error is detected at startup.
5. **One actor, one responsibility** : Tokio actor pattern, no shared state
   between actors.
6. **Memory at agent initiative** : never automatically inject memory context.
7. **Non-negotiable safeguards** : `StepBudget` enforced by the runtime, never
   bypassable.
8. **Human CLI, machine API** : `--json` global, TTY auto-detected.

Source : `docs/wiki/Architecture-Principes.md`. Detailed rationale and worked
examples in `docs/agents/ARCHITECTURE.md`.

---

## Where to read next

| Your task | Read |
|---|---|
| Any Rust change | `docs/agents/RUST-PATTERNS.md` + nearest crate `AGENTS.md` |
| Any Python change | `docs/agents/PYTHON-PATTERNS.md` + `sdk/AGENTS.md` |
| Desktop UI change | `docs/agents/FRONTEND-PATTERNS.md` + `crates/apollia-desktop/ui/AGENTS.md` |
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
- Run `cargo test --workspace` before `git commit`.
- Write tests in GIVEN / WHEN / THEN structure.

## ASK FIRST

- Adding any third-party dependency (Cargo or Python). Each one is a
  sovereignty surface. ADR-justified.
- Touching an ADR in `docs/adr/` (numbered, append-only).
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
- `Co-Authored-By: Claude` (or any AI co-author trailer) in commit messages.
- Mixing French and English in the same file.
- AI stock phrases ("as an AI", "it's important to note", "il convient de
  noter", etc.).
- Committing with a failing `cargo test --workspace`.

Full list with reasons : `docs/agents/FORBIDDEN.md`.

---

## Overrides

- `AGENTS.local.md` (gitignored) : per-machine, per-user preferences.
- `AGENTS.override.md` (gitignored) : temporary session override. Document
  reason at the top.

Precedence (highest first) : chat prompt > `AGENTS.override.md` > nearest
`AGENTS.md` climbing up the tree > this file.

---

Before writing code, read `docs/agents/INDEX.md` and the sub-`AGENTS.md` for the
area you touch. The cost of reading is 2 minutes. The cost of an unaligned
commit is one PR cycle.
