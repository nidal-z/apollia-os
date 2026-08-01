# FORBIDDEN

> NEVER list. Read this file before writing any code or documentation for Apollia.
> Each rule has a reason and a correct example. Violating a rule without a documented
> exemption (ADR or inline `// SAFETY:` / `# REASON:` comment) is a regression.

---

## Rust

**NEVER `anyhow` in the workspace.** Use `thiserror` enums per crate. The only allowed
exception is `apollia-cli` `main()` as the last-resort barrier between the runtime and
the user shell. Reason: `anyhow` erases the type of errors, which breaks structured
matching, exit-code mapping, and structured tracing.

```rust
// WRONG
fn load() -> anyhow::Result<Config> { ... }

// RIGHT
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read {path}")]
    Io { path: PathBuf, #[source] source: std::io::Error },
    #[error("invalid toml at {path}")]
    Parse { path: PathBuf, #[source] source: toml::de::Error },
}
fn load() -> Result<Config, ConfigError> { ... }
```

**NEVER `.unwrap()` or `.expect("...")` in production code.** Tests only.
Exception: when a SAFETY invariant is documented inline.

```rust
// RIGHT (test only)
#[test]
fn test_parse_ok() { let v = parse("1").unwrap(); assert_eq!(v, 1); }

// RIGHT (production with SAFETY)
// SAFETY: regex compiled from a literal string at build time.
let re = Regex::new(r"^\d+$").unwrap();
```

**NEVER `todo!()`, `unimplemented!()`, or `panic!()` in committed code.** If you cannot
complete an implementation now, do not commit. Create a story instead.

**NEVER `println!`, `eprintln!`, or `dbg!` in committed code.** The CLI binary
`apollia-cli` is the only exception, and only for user-facing output. Everywhere else,
use `tracing::event!(Level::*, ...)`.

**NEVER format strings inside log macros.** Use structured fields.

```rust
// WRONG
tracing::info!("user {} performed {}", user_id, action);

// RIGHT
tracing::info!(user_id = %user_id, action = %action, "user.action");
```

**NEVER `Arc<Mutex<T>>` shared between actors.** Each Tokio actor owns its state and
communicates via `mpsc::channel` + a clonable `Handle`. Reason: shared locks across
async tasks deadlock and defeat the actor model.

**NEVER `#[async_trait]` in new traits.** Use return-position `impl Trait` in traits
(RPITIT) with `Send` bounds. `#[async_trait]` boxes futures and is now unnecessary on
stable Rust.

**NEVER modules > 800 lines outside tests.** Split into submodules. The runtime
threshold for review attention drops sharply past that size.

**NEVER `unsafe` code without a SAFETY doc-comment** explaining the invariant being
upheld. Workspace lint: `unsafe_code = "deny"` unless explicitly allowed per crate.

**NEVER tests that depend on ordering.** Use `serial_test` if a global mutex is
required.

**NEVER `#[ignore]` merged without a story link.** A skipped test that no one tracks
becomes dead code.

**NEVER `pub use crate::internal::*`** or any wildcard re-export from internal modules.
Re-export only the public contract.

---

## Python

**NEVER `from __future__ import annotations` in modules that define TypedDict.** PEP
563 turns all annotations into strings, which breaks `TypedDict.__required_keys__`
that AgentKit reads at runtime to build skill schemas. Reason: agents become silently
malformed at registration time.

**NEVER relative imports** (`from .module import X`, `from ..pkg import Y`). Use
absolute imports from the package root.

```python
# WRONG
from .schemas import EmailPayload

# RIGHT
from my_agent.schemas import EmailPayload
```

**NEVER `print()` in agent or SDK code.** Use `ctx.logger` (a stdlib
`logging.Logger` routed to the runtime tracer), e.g. `ctx.logger.info(...)`.

**NEVER Pydantic when TypedDict suffices.** Apollia agents are stdlib-only by default.
Use Pydantic only when runtime validation of external API responses is unavoidable,
and justify it in an ADR.

**NEVER add a third-party dependency without an ADR.** Workers and agents are
stdlib-only by default. Each dep is a sovereignty surface and a maintenance liability.

**NEVER `from typing import *`.** Import explicit symbols.

**NEVER subclass `Exception` directly for new error types.** Subclass `AgentError` so
the dispatcher can map it to an `AIPResult`.

---

## Frontend (Svelte 5 + Tauri)

**NEVER non-strict TypeScript.** `strict: true` is non-negotiable in `tsconfig.json`.

**NEVER Svelte 4 reactive declarations (`$:`)** in new code. Use runes (`$state`,
`$derived`, `$effect`).

**NEVER hardcoded CSS values when a design token exists.** Tokens live in
`crates/apollia-desktop/ui/src/app.css` as HSL custom properties. See
`crates/apollia-desktop/ui/AGENTS.md` for the propagation rules.

**NEVER FR-only or EN-only strings hardcoded in components.** All user-facing text
goes through `svelte-i18n` with parallel FR + EN entries.

---

## Markdown that is a resource, not documentation

**NEVER delete or reword a `.md` file without grepping its name across `crates/`,
`sdk/`, `scripts/`, `agents/`, `.github/` and `justfile` first.** A handful of
markdown files are program input. Deleting one breaks the build; editing one
changes what the product says to a user.

The three families, and how to recognise a new one:

- `crates/apollia-llm/prompts/meta/*.md`, pulled in by `include_str!` from
  `meta_orchestrator.rs`. These are LLM prompts. `include_str!` resolves at
  compile time, so removing a file is a compilation error, and rewording one
  changes model behaviour with no test to catch it.
- `agents/system/apollia-guide/knowledge/*.md`, also `include_str!`, and
  additionally written to disk at first run. These are what the companion agent
  answers with. Stale content here is a product defect, not a stale document.
- `scripts/automation/seed/files/**/*.md`, copied by `build-seed.sh` into the
  throwaway `HOME` of the end-to-end automaton. Removing one breaks the suite.

The test is mechanical: `grep -rn '<basename>.md' crates sdk scripts agents
.github justfile`. A hit inside an `include_str!`, a `cp`, or a path join means
the file is a resource. A hit inside a comment means it is a document, and two of
those exist as known false positives:
`crates/apollia-desktop/ui/src/lib/i18n/operator-glossary.md` and
`crates/apollia-desktop/ui/src/lib/design/breakpoints.md`.

This rule exists because a documentation audit proposed deleting fifteen files
that would have broken the build, and the trap was caught by one grep rather than
by review.

---

## Documentation and prose

**NEVER use em-dash `—` in any prose, comment, or documentation file.** It is a
deliberate house-style choice for typographic consistency across the corpus. Use
comma, parenthesis, colon, period, or hyphen `-` instead.

**NEVER mix French and English in the same file.** Each file is one language.
Current allocation: `docs/site/` is bilingual (en + fr, one language per file),
`docs/agents/*.md` and `docs/adr/` are English, code doc-comments are English,
in-code inline comments are French until L2 sanitize.

**NEVER AI-stock phrases.** Forbidden tokens (non-exhaustive):
- "as an AI", "as a language model", "I'm here to help", "Certainly!", "I'd be happy
  to"
- "it's important to note that", "it's worth noting that", "il convient de noter",
  "il est important de noter", "comme mentionné précédemment"
- "in conclusion", "in summary" as standalone section headers
- "let's", "we will see", "let me explain" in technical docs

**NEVER comments that translate the code into English.** A comment that paraphrases
the line below it is noise.

```rust
// WRONG
// Increment the counter by 1
i += 1;

// RIGHT (no comment)
i += 1;
```

**NEVER `TODO:` or `FIXME:` without a story link.** Format: `TODO(story-NNN):
short description`.

**NEVER hardcoded secrets, API keys, tokens, or PII** in any committed file. Use
`SecretStore` backends (Keyring or AgeFile). See `docs/agents/SECURITY.md`.

**NEVER link to raw ADR files from `docs/site/` public pages.** The ADR corpus
lives in `docs/adr/` and is not published on the site; cite ADRs by their bare
identifier (`ADR-018`) instead of a hyperlink.

**NEVER CSS values in designer briefs.** The designer knows the charter. Briefs
describe structure, wording, and intent only.

**NEVER copy an existing agent in `agents/` as a reference template.** Confidence in
those agents is low. Sources of truth: the SDK type contract
(`sdk/apollia/types.py` + `sdk/apollia/context/*.py`), `@agent` / `@skill`
decorator implementations, and ADR-023 / ADR-024.

**NEVER references to internal artifacts in public-facing files.** Specifically, no
`STORY-NNN`, `sprint-N`, `[Lot N]`, `[Bloc X]` tokens in code comments or in any
file outside `docs/internal/`.

---

## Git and commits

**NEVER `Co-Authored-By: Claude` (or any AI co-author trailer) in commit messages.**
Apollia commits are authored by Nidal. AI assistance is implicit.

**NEVER skip hooks** (`git commit --no-verify`, `--no-gpg-sign`, etc.) unless the
user has explicitly asked for it. If a hook fails, fix the underlying issue.

**NEVER `git push --force` on `main`.** Use force-with-lease on feature branches if
truly needed.

**NEVER interactive rebase (`git rebase -i`).** It blocks on a terminal editor and
breaks LLM tool execution.

**NEVER modify `.git/config` or run `git config`.**

**NEVER empty commits** unless the user has requested it.

**NEVER `git add -A` or `git add .` blindly.** Stage specific files. Prevents
accidental inclusion of `.env`, credentials, lockfiles, build artifacts.

---

## Architecture and workflow

**NEVER violate the 8 non-negotiable principles** (see
`docs/agents/ARCHITECTURE.md` Section A) without an ADR that documents the
deviation, its scope, and its expiration condition.

**NEVER pull request > 800 lines of diff** without an explicit split rationale in
the description.

**NEVER skip the GIVEN / WHEN / THEN structure in tests.** Comments mark each block.
Reason: maintains the discipline that exposed every CLI bug in the Sprint 43 E2E
sweep.

**NEVER reintroduce `op` as a skill dispatch key in A2A workers.** Full `skill_id`
propagation is the canonical path (resolved 2026-05-19). See ADR-023 onward.

**NEVER commit with a failing `cargo test --workspace`.** Pre-commit hook enforces
this; do not bypass.

**NEVER feature-flag dead code** to preserve a half-finished implementation. Either
ship the feature or remove the code.

---

## Process

If a rule above seems wrong for your current task, do not silently violate it. Either:

1. Document an exemption inline (`// SAFETY:`, `# REASON:`, ADR reference), or
2. Stop, surface the conflict to the user, and propose a rule update.

Rules are negotiable. Silent violations are not.
