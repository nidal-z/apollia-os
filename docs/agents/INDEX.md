# docs/agents/ INDEX

> Required reading for any LLM or human contributor about to write code or
> documentation for Apollia. Read this first. Then open the thematic files you
> need. Then read the nearest sub-`AGENTS.md` for the area you are touching.

---

## What this corpus is

`docs/agents/` is the long-form, English-only rulebook for Apollia. It is the
authoritative companion to the root `AGENTS.md` router. Every rule here exists
because following it has, at some point, prevented a regression or because
violating it has caused one.

The corpus assumes the Apollia codebase is 100% AI-generated. Each rule has an
explicit reason so an LLM can judge edge cases instead of pattern-matching.

---

## Reading order

1. **First time on the project** : `ARCHITECTURE.md` -> `FORBIDDEN.md` -> the language
   file matching your task (`RUST-PATTERNS.md` / `PYTHON-PATTERNS.md` /
   `FRONTEND-PATTERNS.md`).
2. **Returning, focused task** : the relevant thematic file + the nearest
   sub-`AGENTS.md`.
3. **Writing tests** : `TESTING.md` (matrix by scope and level).
4. **Writing documentation** : `DOCS-WRITING.md` + the corpus you target.
5. **Writing a commit** : `COMMITS.md`.
6. **Setting up a new machine** : `CI-TOOLING.md`.

---

## Files in this corpus

| File | Scope | When to read |
|---|---|---|
| `ARCHITECTURE.md` | 8 non-negotiable principles, Apollia patterns, top ADRs | First time, and whenever an architectural decision is being made |
| `RUST-PATTERNS.md` | Errors, async, tracing, Cargo, lints, PyO3 | Any Rust change |
| `PYTHON-PATTERNS.md` | SDK decorators, typing, asyncio, exceptions, packaging | Any Python change in `sdk/` or `agents/` |
| `FRONTEND-PATTERNS.md` | Svelte 5 runes, TypeScript strict, Tauri IPC, design tokens, i18n | Any change in `crates/apollia-desktop/ui/` |
| `NAMING.md` | Naming conventions Rust + Python + events + tracing fields + files | Before introducing any new name |
| `TESTING.md` | Matrix unit / integration / E2E / property / snapshot per scope | Before writing or reviewing tests |
| `COMMITS.md` | Conventional commits, scopes, footers, branch naming | Before each commit |
| `DOCS-WRITING.md` | Per-corpus responsibilities, rustdoc, prose style, ADR workflow | Before writing any documentation |
| `OBSERVABILITY.md` | Tracing field names, log levels, semantic conventions | Any code that emits events |
| `SECURITY.md` | Secrets, `SecretStore` backends, audit trail, scope `local_only` | Any code that handles credentials, user data, or external calls |
| `CI-TOOLING.md` | Editor config, lint configs, pre-commit, MSRV | Local setup, CI changes |
| `FORBIDDEN.md` | Hard NEVER list with reasons and examples | Before every commit |

---

## Sub-`AGENTS.md` in the repo

Sub-`AGENTS.md` files apply locally to their subtree and override the root
`AGENTS.md` where they conflict. The nearest one wins.

| Path | Scope |
|---|---|
| `crates/apollia-cli/AGENTS.md` | CLI binary, ADR-004 noun-verb, exit codes 0-5 |
| `crates/apollia-aip/AGENTS.md` | PyO3 bridge, `Bound<'py, T>`, `pyo3-async-runtimes` |
| `crates/apollia-oria/AGENTS.md` | ORIA engine, StepBudget, ResilienceLayer, plan cache |
| `crates/apollia-runtime/AGENTS.md` | EventBus, actor supervisor, axum API |
| `crates/apollia-desktop/ui/AGENTS.md` | Tauri v2 + Svelte 5, design system, i18n |
| `sdk/AGENTS.md` | Apollia AgentKit decorators, schemas, contract minimal |

Create a new sub-`AGENTS.md` when the subtree exceeds ~500 lines of code AND
introduces patterns not covered by the global rules. Document the trigger in
the file header.

---

## Override and escape valves

- `AGENTS.local.md` (gitignored) at the root or in any subtree : personal,
  per-machine preferences. Never committed.
- `AGENTS.override.md` (gitignored by default) : temporary scope override for a
  specific session. Use sparingly. Document the reason at the top.

Precedence (highest wins) : current chat prompt > `AGENTS.override.md` >
nearest `AGENTS.md` > parent `AGENTS.md` (climbing) > root `AGENTS.md`.

---

## What to do when a rule blocks you

Rules are negotiable. Silent violations are not. Three options, in order:

1. Document an exemption inline (`// SAFETY:`, `# REASON:`, ADR reference).
2. Surface the conflict to the user and propose a rule update.
3. Open an ADR if the conflict reflects a real architectural shift.

Never circumvent a rule by reformulating it in vague terms.

---

## Related corpora outside this directory

- `docs/book/` : pedagogical mdBook, French, end-user developer onboarding.
- `docs/wiki/` : technical reference, currently being rebuilt in English (L2b).
- `docs/help/` : operator help, French, desktop app focus.
- `docs/adr/` : architectural decision records, numbered, append-only.
- `wiki/Architecture-Principes.md` : authoritative source for the 8 principles.
- `wiki/DESIGN-SYSTEM.md` : design tokens, components, propagated into
  `FRONTEND-PATTERNS.md` and `crates/apollia-desktop/ui/AGENTS.md`.

The `docs/agents/` corpus does not duplicate those references. It cites them.
