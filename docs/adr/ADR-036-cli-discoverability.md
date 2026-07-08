# ADR-036: CLI discoverability (completions, palette, guide, did-you-mean)

- Status: Accepted
- Date: 2026-07-06
- Depends on: ADR-034 (CLI taxonomy v2, the stable surface these features expose)

## Context

The CLI has no shell completion, no command palette or fuzzy finder, and no
interactive help beyond clap's `--help` (absences confirmed by grepping the crate:
no `clap_complete` dependency, no `visible_alias`, no completion module). This is
the pure developer-experience gap versus Claude Code, on a surface that is
otherwise powerful. ADR-034 gives a stable, canonical v2 command surface, so
making it discoverable is decided here.

Two choices touch dependencies, which the project gates (root `AGENTS.md`: "adding
any third-party dependency ... ADR-justified"), so they are settled in this ADR.

## Decision

We add four discoverability features, keeping the dependency footprint minimal.

- **Shell completion via `clap_complete`.** A `apollia-os completions <shell>`
  subcommand generates completion scripts for bash, zsh, fish, and powershell from
  the existing clap derive. We add the `clap_complete` crate (the clap-official
  companion). It runs at compile/generation time and emits static scripts, so it
  adds no runtime or system dependency and does not affect Principle #2 (the binary
  still runs on a clean machine with no prior install).

- **Command palette / fuzzy finder in the REPL, dependency-free.** A key binding in
  the chat REPL opens a finder over the command list plus slash-commands; a small
  hand-written subsequence scorer ranks matches; selecting one executes it. No
  fuzzy-matching library is added; a subsequence scorer is sufficient for a surface
  of roughly fifty commands and stays testable and sovereign.

- **`apollia-os guide <topic>`.** Short, task-oriented topic help (chat,
  governance, audit, agents, ...) beyond clap's reference `--help`. Content is
  hand-written and lives with the CLI.

- **"Did you mean" on an unknown command, deterministic.** On an unrecognized noun
  or verb, the CLI suggests the nearest valid one using edit distance (Levenshtein
  over the v2 catalog). No LLM is involved; the suggestion is instant, offline, and
  deterministic. (The LLM-backed natural-language path is `do`, ADR-035; this is
  the cheap typo path.)

- **No ergonomic aliases.** We keep one canonical name per command (the ADR-034
  verbs), with no `ls`/`rm`/`ps` shortcuts. This preserves the clean, single-name
  surface established by the pre-release rename.

## Alternatives considered

### Hand-rolled completion scripts (rejected)
**For:** zero dependency.
**Against:** brittle, maintained by hand, and drifts from the clap definitions on
every command change. `clap_complete` derives them automatically and stays in sync.

### A fuzzy-matching library, nucleo or fuzzy-matcher (rejected)
**For:** higher-quality scoring and highlighting.
**Against:** a dependency for a modest need. A subsequence scorer is enough for the
command surface and avoids the sovereignty and maintenance cost of another crate.

### Ergonomic aliases (`ls`, `rm`, `ps`) (rejected)
**For:** faster to type; familiar muscle memory.
**Against:** reintroduces surface redundancy immediately after ADR-034 canonicalized
to one verb per intent. One name per command keeps the surface clean and
teachable.

### LLM-backed "did you mean" (rejected)
**For:** could infer intent, not just spelling.
**Against:** edit distance is instant, deterministic, and offline; the model is not
needed for a typo. Intent-level mapping is already covered by `do` (ADR-035).

### Chosen: clap_complete + dependency-free fuzzy + guide + edit-distance suggestion, canonical-only
**For:** closes the discoverability gap with a single, well-justified dependency;
completion auto-tracks the clap tree; the surface stays clean.
**Trade-offs:** one new compile-time crate; a small maintained scorer and
hand-written guide content.

## Consequences

**Positive:**
- Shell completion, an in-REPL palette, topic guides, and typo suggestions close
  the DX gap versus Claude Code.
- Only one new crate (`clap_complete`), compile-time and completion-only.
- The command surface stays single-name and consistent with ADR-034.

**Negative / trade-offs:**
- One new dependency (`clap_complete`).
- The fuzzy scorer and the `guide` content are hand-maintained.

**Neutral / to watch:**
- `guide` topic content must track the taxonomy; completion is derived, so it
  auto-tracks.
- The edit-distance catalog is generated from the clap tree, not hand-listed.

## Principles impacted

- Principle #2 (Zero external dependency): we add `clap_complete`, justified here.
  It is compile-time only and emits static scripts, so the runtime deployment
  footprint (a clean machine, no prior install) is unchanged.
- Principle #8 (Human CLI, machine API): completion, palette, and guides are pure
  human-side ergonomics; `--json` and exit codes are untouched.

## Links

- Depends on: ADR-034 (taxonomy v2)
- Related: ADR-035 (AI-native surface; `do` is the intent path, this ADR is the
  discoverability path)
- Stories: to be created after this ADR is accepted
