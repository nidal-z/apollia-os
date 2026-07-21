# Desktop E2E → CLI E2E handoff

Written by the desktop-E2E agent for the CLI-E2E agent. We share the same seed
(`scripts/automation/seed`) and the same deterministic/LLM philosophy, so most of
this is "here is what I learned driving the real desktop app that transfers to
your tracks", plus one hard dependency: **I changed the shared seed this session**.

## 1. The shared seed changed under you (action required)

You build `tests/cli` HOME from `scripts/automation/seed`. I own/maintain it and
made four changes this session that affect CLI assertions:

- **Default LLM backend is now `local-llama-server`** (provider `openai`,
  `base_url http://127.0.0.1:8899/v1`), not the embedded `local-qwen`. Reason:
  the `-seeded-llama` desktop recipe routes inference through an external
  llama-server, and the embedded `local-qwen` resolves under the HOME-swap to the
  seed's placeholder GGUF and fails to load (`MODEL_LOAD_FAILED`). `local-qwen`
  is kept as a NON-default row. **Check Track 2/3**: any assertion on "the default
  backend" or `llm list` ordering/default now sees `local-llama-server`. If your
  Track 3 drives the embedded runner, it is no longer the default.
- **Agent stubs are now real `@agent` agents** (`seed/files/agents/*/agent.py`):
  the old `manifest()`/`run()` shape did not load (the loader requires the
  `@agent` decorator's `__apollia_manifest__`). The 4 seed agents (apollia-chat,
  apollia-guide, onboarding-agent, seed-classifier) now load and **auto-start at
  boot** (ProcessState Active). Any `agent list`/status assertion will see them
  Active, and `seed-classifier` exposes the A2A skill `classify_text` (id has NO
  hyphen; the `@skill` grammar forbids it).
- **`install_path` now points at `agent.py`** (not the directory) in
  `seed/fragments/agents.sql` + `build-seed.sh`; the loader validates a `.py`.
- **New `mailbox.db`** seeded (`seed/schemas/mailbox.sql` + `fragments/mailbox.sql`,
  3 inter-agent messages). If you assert the seed DB set, there is one more file.

Re-run your Track 1/2 after pulling; if a seed-dependent assertion drifts, this is
why. The seed README (`seed/README.md`) is the map.

## 2. What the desktop suite is (for context)

A dev-only gestural automaton drives the real Tauri app by `data-testid` (no
WebDriver on WKWebView). Declarative JSON scripts, a machine-readable
`report.json` (per-step `ok`/`detail`), seeded throwaway HOME, deterministic
`<page>-det.json` books + a one-boot `master-det` aggregate, LLM scripts via a
real local model. Docs: `scripts/automation/README.md`; tools:
`scripts/automation/tools/`.

## 3. Lessons that transfer to CLI E2E

1. **Assert the right signal, not a signal.** My worst bug of the session: every
   LLM script "passed" its DOM asserts while the onboarding modal covered the
   whole UI, because the runner clicks by testid and bypasses occlusion. The
   asserts were true but meaningless. CLI parallel: grepping stdout for a
   substring that also appears in an error banner, or a command that prints its
   help and exits 0 when you expected it to act. Prefer the **exit code + `--json`
   structured field** over stdout scraping; when you must scrape, anchor the match
   and also assert the exit code.

2. **Created ≠ Validated.** I kept a ledger separating scripts that are authored
   and static-valid from scripts that actually ran green on the real app. Do the
   same in `report.md`: never mark a track green off "the script exists" or "it
   exited 0 once": a real run with the real assertions is the only green.

3. **Static-validate before the expensive run.** `tools/validate.py` checks every
   step's `testid`/`route`/`kind` resolves against the current UI source before
   any app boot, so a rename is caught in 200ms, not 15 minutes in. CLI parallel:
   introspect `apollia-os --help` / clap to confirm every command+subcommand+flag
   you assert on still exists, before booting the daemon. Cheap, catches drift.

4. **Coverage as a measured number, with justified gaps.** I diff every source
   `data-testid` against what the scripts exercise and bucket the uncovered
   (dead-code, needs-model, fault-injection, viewport-only, ...). CLI parallel:
   enumerate every command leaf from clap and track exercised vs justified-skipped,
   so "we test the CLI" becomes a percentage with named exclusions, not a vibe.

5. **Deterministic gate vs model-dependent best-effort.** Your three-track split
   is exactly right and mirrors mine. One reinforcement from running both a weak
   (Ministral-8B) and strong (Qwen-35B) model: **even a strong model will not
   reliably tool-call/delegate a trivial task** (Qwen classified inline instead of
   calling the A2A tool). So Track 3 must assert STRUCTURE (exit, streaming shape,
   timing, a tool WAS offered) and capture the rest for human review, never the
   exact tool choice or wording. You already do this; hold the line.

6. **The real value is finding blocking bugs, not going green.** These runs
   surfaced genuine product bugs I then fixed: STT history was coupled to a loaded
   model (`fix(stt)`), plan mode left the plan empty in discovery
   (`fix(prompts)`), and the mailbox had no UI at all (`feat`). Treat a failing CLI
   assertion as a candidate product bug first, a script bug second.

7. **Concurrency corrupts the shared build cache.** Your cargo builds and my
   `cargo tauri dev` runs share `target/`; running them at the same time clobbered
   `cfg_if`/`parking_lot_core` `.rmeta` and broke both builds
   (`cargo clean -p cfg-if -p parking_lot_core` recovers it). If CI or a local
   loop runs CLI and desktop builds concurrently, give them separate
   `CARGO_TARGET_DIR`s or serialize them.

8. **Aggregate + per-item.** `master-det` runs all 20 domains in one boot (cheap
   full smoke, one report) and each `<page>-det` isolates one surface. Your
   per-track scripts are the per-item; consider a single "full sweep" entry that
   composes them for a one-command smoke, regenerated from the parts so it never
   drifts (`tools/regen_master.py` is the pattern).

9. **Keep the suite lean.** I had accreted 57 scripts across two generations and
   retired 25 superseded ones. A stale script that no one runs is worse than no
   script: it reads as coverage that is not there. Prune on every generation
   change.

## 4. Concrete suggestions to complete the CLI suite

- A `tools/validate.sh` for CLI: parse each track's asserted commands and confirm
  they exist in `apollia-os --help` output (fail fast on a removed/renamed
  command), the analogue of `validate.py`.
- A command-coverage report: clap leaf list vs exercised, with a justified-skip
  bucket, appended to `report.md`.
- Exit-code contract table (0-5 per `crates/apollia-cli/AGENTS.md`) asserted
  explicitly per command class, not just "non-zero on error".
- Track 3: after the shared-seed backend change, point the LLM backend explicitly
  at your test model (do not rely on the seed default, which is now the external
  llama-server) and assert only structure.
- If you want the desktop `report.json`/analyzer shape for cross-suite tooling,
  `scripts/automation/tools/analyze_report.py` groups failures by section and is
  trivial to adapt.

## 5. Where things live

- Desktop suite + tools + gotchas: `scripts/automation/README.md`,
  `scripts/automation/tools/`.
- Shared seed: `scripts/automation/seed/` (+ its README).
- My run ledger, coverage notes, and green report artifacts: the gitignored
  internal QA area (ask the human for the path if you need it).
- Session commits (desktop): `git log 3505a3d7..HEAD` (14 commits, all under
  `scripts/automation/`, `crates/apollia-desktop/`, `crates/apollia-prompts/`,
  `crates/apollia-runtime/src/stt/`). None touch `tests/cli` or `apollia-cli`.
