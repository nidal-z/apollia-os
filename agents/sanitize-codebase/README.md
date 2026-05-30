# sanitize-codebase

> Director L2 + 2 workers that reproduce the Claude sanitize pass on the
> Apollia OS public repo, locally, for zero euro of inference cost.

This agent is the storytelling hero of the v0.1.0-preview launch. The
Claude pass (Phase 1, branch `sanitize-claude`) is the assurance-quality
ground truth that feeds the orphan-init. The Apollia pass (Phase 2,
branch `sanitize-apollia`, off the same baseline) reproduces that diff
through a director plus two workers driven by a local quantized LLM.

## What it does

Three things, file by file:

1. **Sonar cleanup** : pull issues from the local SonarQube via MCP,
   ask the local LLM for a single behavior-preserving edit, gate via
   the language harness (cargo / ruff / pnpm), revert on red.
2. **Comment sanitize** : rewrite comments from French to English,
   strip em-dash and other anti-AI markers, neutralize internal
   references (ADR-NNN, STORY-NNN, sprint-N, docs/internal/*),
   preserve the why. Code lines stay byte-identical.
3. **Verification harness** : `cargo build -p <crate>`,
   `ruff check <path>`, `pnpm -C crates/apollia-desktop/ui check`.
   Red harness reverts the file from the pre-edit snapshot.

Progress is persisted in the SQLite memory namespace
`sanitize-codebase`. An interval trigger picks up the next batch every
10 minutes. Restarts resume from the manifest, no state is lost.

## Architecture (L2 director + 2 workers)

```
sanitize-codebase-director (interval-triggered)
  |
  +-- A2A sanitize.clean_file        --> sonar-cleanup-worker
  +-- A2A sanitize.translate_comments --> comment-sanitize-worker
  +-- bash_executor : harness gate
  +-- ctx.memory    : file:{path}, seen:{sha}, procedure "sanitize-batch"
  +-- ctx.notify    : desktop summary per batch
```

State machine (cf. `schemas/state.py`):

```
INIT -> LOAD_DATASOURCES -> LOAD_MANIFEST -> PICK_BATCH ->
DISPATCH_SONAR -> DISPATCH_COMMENTS -> VERIFY_HARNESS ->
PERSIST_PROGRESS -> NOTIFY -> DONE
```

## Four pillars

| Pilier | Where it lives |
|---|---|
| 1. Templates | `templates/sonar-fix-prompt.md.j2`, `templates/comment-translate-prompt.md.j2`, `templates/batch-summary.md.j2` ; `schemas/types.py` (TypedDict canonicals) |
| 2. Steps | `schemas/state.py` enum + dispatch table in the director |
| 3. Datasources | `datasources/rules.yaml`, `datasources/pool.yaml`, `datasources/exclusions.yaml`, priority `ctx.workspace > local > {}` |
| 4. Memory | namespace `sanitize-codebase` ; keys `file:{path}`, `seen:{sha}` ; procedure `sanitize-batch` ; episode per batch |

## Running it

See `SETUP.md` for the full setup. The short version:

```sh
# Install the package against the local Apollia daemon.
apollia agent install agents/sanitize-codebase

# Trigger one batch synchronously (does not require the interval trigger).
apollia agent run sanitize-codebase-director \
  --skill sanitize.run_batch \
  --input '{"prompt": "Run the next sanitize batch", "batch_size": 2}'

# Enable the interval trigger for continuous operation.
apollia trigger enable sanitize-interval
apollia trigger logs sanitize-interval --follow
```

## Replay-and-compare

The whole reason this agent exists is to compare its output to the
Claude pass. The procedure is in `eval/replay.md`. Short version:

```sh
git checkout -b sanitize-apollia pre-sanitize-baseline
apollia trigger enable sanitize-interval
# wait until counts.pending == 0
git diff sanitize-claude sanitize-apollia
```

The numbers that feed the launch blog post (token cost Phase 1 vs zero
Phase 2, wall clock Phase 2, diff stat) are also in `eval/replay.md`.

## Eval suite

`eval/cases.jsonl` lists 5 representative cases (comment translation
on Rust / Python / TypeScript, batch pickup, sonar cleanup on
`apollia-cli`). `eval/run-eval.py` runs each case N times against the
running daemon and reports success rate + latency percentiles.

```sh
python eval/run-eval.py --runs 3
```

## Limits and known residues

- The local LLM is the bottleneck. Quality depends entirely on the model
  configured in `apollia.toml`. The agent does not try to escalate to a
  cloud backend on its own. That is the whole point.
- Edits are anchored on unique substrings. Files with extreme
  repetition (e.g. boilerplate test fixtures) sometimes return
  `edit_anchor_ambiguous` and end up as residue.
- The Sonar pass only attempts one edit per call. Multi-issue files
  converge over several ticks. This is deliberate: smaller edits are
  easier to gate.
- The comment pass uses a stdlib-only line-based parser. Multi-line
  block comments split across language constructs (e.g. doc-strings
  mid-class) are detected as comment blocks but the boundary detection
  is line-prefix based, not AST-based.
