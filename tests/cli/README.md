# `tests/cli/` - Apollia OS CLI end-to-end suite

Exercises the whole `apollia-os` CLI surface against a **fixed, isolated,
deterministically-seeded** data profile, so read commands assert KNOWN content
(not empty states), and produces a structured report (`report.json` +
`report.md`). Regressions on any command path are caught by a single run.

## Quick start

```sh
# Track 1 only (OFFLINE, no daemon, ~a few seconds):
bash tests/cli/cli-e2e.sh

# Tracks 1 + 2 (+ 3 if a model is wired), spawns the daemon (~90 s on a debug build):
APOLLIA_REQUIRE_RUNTIME=1 bash tests/cli/cli-e2e.sh
```

Exit 0 on a full pass, 1 on the first failed assertion. The run always writes
`tests/cli/report/report.md` (human) and `report.json` (machine).

## Architecture

```
tests/cli/
  cli-e2e.sh            orchestrator: env, seed, tracks, report
  lib/
    seed.sh             build a fresh throwaway seeded HOME (per phase)
    assert.sh           check / check_exit / check_json / check_grep /
                        check_content / check_json_field / skip + capture_*
    report.sh           TSV accumulator → render_report.py
    render_report.py    assemble report.json + report.md
    run_capture.py      run a command, capture I/O + timing + stream bursts
    pty_run.py          drive an interactive REPL under a pty, capture the stream
  tracks/
    track1_offline.sh   OFFLINE deterministic (seeded content + exit contract)
    track2_runtime.sh   RUNTIME deterministic (daemon on seeded HOME + CRUD)
    track3_llm.sh       LLM capture (non-deterministic: structure only + capture)
  report/               report.json + report.md (git-ignored output)
```

### The seed (fixed dataset)

The suite does not build state from scratch; it loads the shared, committed seed
builder at `tests/cli/seed/` (one source of truth with the desktop
automation suite, never a fork). `lib/seed.sh` rebuilds a throwaway `HOME` per
phase, so the reference fixtures a later assertion depends on are never mutated,
and the real `~/.apollia` is never touched. Isolation is a `HOME` swap: the CLI
resolves everything from `$HOME/.apollia` and `$HOME/.config/apollia`.

The builder also accepts a narrative overlay (`APOLLIA_SEED_OVERLAY`), used to
give the documentation screenshots a coherent usage history. This suite never
sets it, and the builder never applies one unless asked, so the content asserted
below is the committed fixture and nothing else. See
`tests/cli/seed/README.md`, section Overlay.

Seeded content asserted by the tracks (fixed ids, fixed `2026-07-01` data):
2 projects, 4 permission rules, 5 memory namespaces, 4 chat sessions, 4 agents
(+ 1 package), 3 LLM backends (`local-qwen` default), 4 triggers, 2 notify
channels, 2 live MCP servers (via a bundled stdio stub).

### The three tracks

| Track | Gate | What it does |
|---|---|---|
| 1 OFFLINE | always | Every command runnable without the daemon, against the seeded HOME. Content assertions (`project list` shows the 2 seeded projects, `permissions list` the 4 rules, `memory search` a known hit, …) plus the exit-code contract (daemon-off → 2, validation → 1, clap → 2). |
| 2 RUNTIME | `APOLLIA_REQUIRE_RUNTIME=1` | Daemon booted on the seeded HOME, so `agent/trigger/notify/mcp/llm-backends list` return seeded state; full CRUD lifecycle; the runtime-only leaves (`a2a`, `audit verify/anchor`, `tools config`, `stt config`, `mcp show/test/restart`, `trigger fire`, `notify events set`, …). |
| 3 LLM CAPTURE | `APOLLIA_REQUIRE_RUNTIME=1` + a real model | Non-deterministic commands (`run --stream`, `chat` REPL via pty, `llm chat`, `do`, `explain`). Asserts STRUCTURE ONLY (exit code, streaming happened, terminated in time); the full input/output is captured into `report.md` for human review. The content is never asserted, matching the "prove the stream, not the answer" intent. |

Track 3 gracefully skips when no model is wired or the `apollia-runner` sidecar
is unreachable; the skip and its reason are recorded in the report.

## Environment variables

| Var | Default | Effect |
|---|---|---|
| `APOLLIA_BIN` | `target/release/apollia-os` (fallback `target/debug`) | binary under test. A relative path is resolved against the calling directory; a path that is not executable makes the suite exit 2 without running an assertion |
| `APOLLIA_REQUIRE_RUNTIME` | `0` | `1` runs Tracks 2 and 3 |
| `APOLLIA_TEST_MODEL_GGUF` | `~/.apollia/models/Qwen3-30B-A3B-Q4_K_M.gguf` | real model for Track 3; absent → Track 3 SKIP, never FAIL |
| `APOLLIA_TEST_REVIEW` | `0` | `1` captures `review .` in Track 3 (slow) |
| `APOLLIA_TEST_VERBOSE` | `0` | `1` dumps stdout/stderr on FAIL |
| `APOLLIA_E2E_REPORT_DIR` | `tests/cli/report` | report output directory |
| `APOLLIA_TOKEN_STORAGE` / `APOLLIA_TOKEN_PASSPHRASE` / `RUST_LOG` | file / test passphrase / error | forced for hermeticity |

## The report

`report.json` carries every assertion (`track`, `label`, `verdict`, `exit`,
`duration_ms`) and the Track 3 captures (`input`, `output`, `stream_chunks`,
`first_chunk_ms`, `duration_ms`). `report.md` renders a coverage-by-track table,
the failures, the justified skips, and a **Non-deterministic captures** section
that surfaces each LLM command's input/output/streaming for human review.

A failing assertion carries one field more, `detail`: the expectation, the
command that was run, and the head of the observed output. A passing or skipped
row has no `detail` key at all, so a green report keeps the shape it always had.
The same string appears under its bullet in the **Failures** section of
`report.md`, which is the file the last line of a run points you at.

Three machine-specific roots are replaced in the detail before it is written,
`$RUN_TMP`, `$REPO` and `$HOME`. The report directory is git-ignored, so the
prose guard never scans it, and CI uploads it as an artifact. Two runs on two
machines therefore produce comparable details.

`APOLLIA_TEST_VERBOSE` keeps its own use: it prints the detail as the run goes,
which is the only trace left when a run is interrupted before it finalizes its
report, since the working directory holding the buffered rows goes with it.

## Justified skips (never automated)

The only remaining skips are genuinely non-automatable: browser OAuth
(`auth login`, `mcp oauth login`), large HuggingFace downloads (`model search`,
`stt model download`), git clones (`agent install <git-url>`), and masked-stdin
credential prompts. Each is recorded in the report with its reason. The
interactive `chat` REPL is NOT skipped: it is driven under a pty in Track 3.

## CI

The offline track runs on every PR (`cli-e2e` job in `.github/workflows/ci.yml`),
which builds `apollia-cli`, installs `sqlite3`, runs `bash tests/cli/cli-e2e.sh`,
and uploads `report/` as an artifact. Tracks 2 and 3 stay opt-in (they need the
daemon and a model) and are run before releases.

## Extending the suite

Add assertions with the `lib/assert.sh` helpers so PASS/FAIL/SKIP and the report
stay consistent. Put daemon-free commands in `track1_offline.sh`, runtime
commands in `track2_runtime.sh`, and model-backed / streaming commands in
`track3_llm.sh` via `capture_run` / `capture_stream` / `capture_pty`. When a
read command asserts seeded content, cite the seeded id/value it depends on.
```
