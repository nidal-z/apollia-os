# Replay-and-compare procedure

This document describes how to test the Apollia OS sanitize agent against
the Claude sanitize pass that produced the `sanitize-claude` branch. The
goal is not to automate replay (it is operator driven) but to give a
clean, reproducible recipe for the comparison.

## Prerequisites

- Tag `pre-sanitize-baseline` exists locally (set right before the
  Claude pass started, cf. release journal 2026-05-28).
- Branch `sanitize-claude` exists locally and contains the Claude pass
  diff (~213 files modified).
- Local SonarQube + MCP wired (cf. SETUP.md).
- Local LLM backend configured in `~/.apollia/apollia.toml`.

## Procedure

```sh
# 1. Fork the baseline into a fresh branch.
git checkout -b sanitize-apollia pre-sanitize-baseline

# 2. Install the agent package against the Apollia daemon.
apollia agent install agents/sanitize-codebase

# 3. Enable the interval trigger declared in manifest.toml.
apollia trigger enable sanitize-interval

# 4. Let it run. The director picks up to 4 files per tick (10 min).
#    A full repo run is volume-dominated, expect multiple hours to
#    multiple days depending on the local LLM throughput. Memory
#    persistence means restarts are safe.
apollia trigger logs sanitize-interval --follow

# 5. Once `apollia agent run sanitize-codebase-director \
#       --skill sanitize.run_batch --input '{"prompt":"status"}'`
#    reports `counts.pending == 0`, stop the trigger.
apollia trigger disable sanitize-interval

# 6. Compare against the Claude pass.
git diff sanitize-claude sanitize-apollia
git diff --stat sanitize-claude sanitize-apollia
```

## Success criteria

- `cargo build --workspace` and `cargo build -p apollia-desktop` both
  green on `sanitize-apollia`.
- `cargo test --workspace --lib`, `cargo test -p apollia-desktop`,
  `pnpm vitest`, `ruff check` all green.
- `cargo audit` shows no new vulnerability.
- SonarQube re-scan: total issue count within +/- 10% of the Claude
  result on the public surface (crates + sdk + ui/src).
- Comment diff: random sample of 50 comment lines reads as
  human-quality English, em-dash free, no `ADR-NNN` / `STORY-NNN` /
  `sprint-N` / `docs/internal/*` references.

## Storytelling artefacts

- Token cost of Phase 1 (Claude) vs zero of Phase 2 (Apollia local).
  Pull the Claude token count from the `.claude/projects/...` cost log.
- Wall clock of Phase 2: take it from the `metrics.wall_clock_secs`
  field aggregated across batch entries in the namespace memory.
- Diff summary: `git diff --stat sanitize-claude sanitize-apollia`
  plus a sentence on the residue count per language.

These three numbers feed the blog post and the launch tweet:
"Apollia OS reproduced a Claude-quality refactor of its own codebase,
locally, for 0 euro."
