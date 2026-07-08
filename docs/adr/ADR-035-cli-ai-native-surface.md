# ADR-035: AI-native CLI surface (do, explain, reprompt) on the local model

- Status: Accepted
- Date: 2026-07-06
- Depends on: ADR-034 (CLI taxonomy v2, the stable surface `do` targets)

## Context

The CLI is operation-oriented. Cloud assistants (Claude Code, Claude Desktop)
let a user type intent in natural language and offer prompt help; the Apollia CLI
does not. But Apollia ships a local LLM and, verified in the code, a working
GBNF-constrained decoding path: `CompletionRequest` carries a
`grammar: Option<String>` (`crates/apollia-llm/src/types.rs:121`) and the runner
applies it as a grammar sampler stage
(`crates/apollia-runner/src/backends/llama_cpp.rs:823`,
`LlamaSampler::grammar(model, gbnf, "root")`, hard-error on invalid grammar). A
GBNF generator already exists (`crates/apollia-llm/src/grammar.rs`).

This lets Apollia offer natural-language command entry, prompt improvement, and
error explanation **offline, free, and private**, with a safety and sovereignty
posture cloud CLIs cannot match: nothing leaves the machine by default, and the
command shape is constrained by a grammar rather than trusted from free text.
ADR-034 gives the stable v2 command surface these features map onto, so the
AI-native surface is decided now. Introducing an LLM front-end that can run
commands is a safeguard-boundary change (ARCHITECTURE Section E), decided here.

## Decision

We add three local-model-backed features to the CLI.

- **`apollia-os do "<natural language>"`** maps intent to a v2 command. The CLI
  introspects its own clap command tree to build a GBNF grammar that admits only a
  valid `<noun> <verb> [args]` string, or an `unknown` sentinel. It posts the
  natural-language query plus the grammar to `POST /api/v1/llm/complete` (the
  underlying `CompletionRequest` already carries `grammar`; the HTTP DTO is
  extended to pass it through). The runner constrains decoding with the grammar,
  so the model can only emit a syntactically valid command or `unknown`. The
  mapped command is shown as a **dry-run** and executed only after an explicit
  `[o/N]` confirmation; `-y` / `--yes` skips confirmation for non-interactive use.
  `unknown` reports that no command matched and suggests rephrasing.

- **`apollia-os explain "<command or error>"`** runs a local completion that
  explains a command or an error message in plain language. It is read-only and
  never executes anything.

- **`/reprompt`** in the chat REPL rewrites the pending prompt for clarity and
  context via a local completion; the user reviews and edits before sending. It
  never auto-sends.

- **Safety.** `do` never executes without confirmation, and the chosen command
  runs through the normal CLI dispatch, so it inherits the same permissions,
  governance, and audit as if typed by hand. `do` is a front-end, not a bypass.

- **Sovereignty.** These features use the default LLM backend (local in the
  standard configuration) and always print which backend handled the request, so
  there is never a silent cloud call. The user keeps their configured choice.

- **Ownership.** The command catalog and grammar are built in the CLI, which
  introspects its own clap tree (self-description, not business logic). LLM
  inference stays in the runtime (`AGENTS.md`: the CLI calls, it does not compute
  the LLM logic).

## Alternatives considered

### Free-text response, then parse (rejected)
**For:** simplest to start; no grammar.
**Against:** fragile. The model hallucinates flags and nouns, so the CLI must
guess and validate, and a mis-parse can produce a wrong command. Unsafe for a
feature that runs commands.

### Tool / function-calling selection (rejected)
**For:** each command as a callable "tool"; a familiar pattern.
**Against:** heavier to wire and less deterministic than a grammar. GBNF is
already wired and gives a hard guarantee on output shape.

### Build the catalog and grammar in the runtime (rejected)
**For:** keeps all LLM concerns server-side.
**Against:** the command surface is defined by the CLI's clap tree; shipping it to
the runtime duplicates the source of truth and drifts. The CLI introspecting its
own commands is self-description.

### Auto-run read-only commands (rejected)
**For:** fewer keystrokes for `list` / `show`.
**Against:** two behaviors to learn, and a mis-mapped "read-only" command could
still surprise. Uniform dry-run plus confirm is simpler and safer.

### Cloud-allowed assist by default (rejected)
**For:** better mapping quality from a large cloud model.
**Against:** command intent and prompts are sensitive; local-by-default keeps them
on the machine, matching the sovereignty thesis. Cloud stays available only when
the user has explicitly made it the default, and is always surfaced.

### Chosen: GBNF-constrained `do` + local `explain` / `reprompt`, dry-run + confirm, local by default
**For:** natural-language entry, prompt help, and error explanation that are
offline, free, and private; a live showcase of the local model and GBNF; a
front-end over real dispatch with no governance bypass.
**Trade-offs:** `do` quality is bounded by the local model; a small runtime DTO
change to pass the grammar; the CLI gains a clap-introspection plus
grammar-generation module.

## Consequences

**Positive:**
- Natural-language command entry, prompt improvement, and error explanation that
  work offline, at zero cost, and without leaving the machine.
- A concrete, user-facing showcase of the sovereign local model and GBNF.
- `do` is a front-end over the normal command dispatch, so permissions,
  governance, and audit are unchanged.

**Negative / trade-offs:**
- `do` mapping quality is bounded by the local model in use.
- The `/api/v1/llm/complete` HTTP DTO gains a `grammar` pass-through field.
- The CLI gains a clap-introspection and grammar-generation module; kept thin and
  derived from clap, never hand-authored.

**Neutral / to watch:**
- The grammar must be regenerated from the clap tree whenever the taxonomy
  changes; it is derived at runtime, not hand-maintained.
- `do` mapping accuracy is measured with a golden natural-language to command set
  (see the epic verification plan).

## Principles impacted

- Principle #1 (Local-first): the assist features run on the local model and
  nothing leaves the machine by default.
- Principle #8 (Human CLI, machine API): natural-language entry lowers the barrier
  while `--json` and exit codes 0-5 are untouched.
- Principle #7 (Non-negotiable safeguards): `do` routes through the normal
  dispatch with confirmation and governance, never a bypass.

## Links

- Depends on: ADR-034 (taxonomy v2)
- Related: ADR-C (discoverability). The "did you mean" typo suggestion on an
  unknown command is deferred to ADR-C as a deterministic edit-distance feature.
- Stories: to be created after this ADR is accepted
