# `tests/cli/` — Apollia OS CLI end-to-end suite

Single file: [`cli-e2e.sh`](./cli-e2e.sh). One source of truth for smoke +
E2E coverage of the `apollia-os` binary.

## Quick start

```sh
# Phase A only (LOCAL, no daemon — ~5–10 s):
bash tests/cli/cli-e2e.sh

# Phase A + Phase B (RUNTIME, spawns the daemon — ~60–90 s):
APOLLIA_REQUIRE_RUNTIME=1 bash tests/cli/cli-e2e.sh
```

The suite exits 0 on full pass, 1 on the first failed assertion. Skips are
documented inline (interactive REPL, OAuth browser flow, network calls,
deferred v0.1.1 items).

## Coverage at a glance

| Domain | Phase A | Phase B | Skipped |
|---|---|---|---|
| start/stop/status, doctor, version, --help | 5 | 4 | — |
| config (get/set/validate/edit/show/reset) | 13 | — | — |
| project (CRUD + agents + templates + link/chats) | 16 | — | — |
| user-memory (show/set/forget/reset/schema/export/import) | 10 | — | — |
| chat-config (get/set/reset/permissions/authorizations) | 12 | — | 1 (deferred) |
| permissions (list/add/audit/revoke) | 11 | — | — |
| memory (inspect/list/clear/purge/learn/export/import/forget/search) | 14 | — | — |
| connector (list/accounts/test/revoke + client-id/secret/api-key + Drive) | 19 | — | — |
| mcp (list/approvals + secret + oauth + discover) | 18 | 1 | 2 (deferred + browser) |
| chat hygiene (delete/rename/export) | 11 | — | 1 (REPL) |
| llm (--threshold + setup --local + CRUD on backends) | 10 | ~15 | — |
| daemon-off exit codes (37 cmds) | 32 | — | — |
| agent lifecycle | — | ~14 | — |
| task lifecycle (run + trace + approvals) | — | ~9 | — |
| triggers (cron, interval, filewatch, webhook) | — | ~12 | — |
| notify CRUD | — | ~5 | 1 (notify test) |
| stt / model / resilience / plan-cache / digest / rollback | — | ~14 | — |
| auth (login/status/logout), update, onboard, mcp-server | — | — | 5 (UI/network) |

Totals (latest run on dev machine):

| Mode | PASS | FAIL | SKIP |
|---|---|---|---|
| Phase A only | 173 | 0 | 17 |
| Phase A + B | 239 | 0 | 28 |

## Environment variables

| Var | Default | Effect |
|---|---|---|
| `APOLLIA_BIN` | `./target/release/apollia-os` (fallback `./target/debug/apollia-os`) | Binary to test. |
| `APOLLIA_TEST_MODEL_GGUF` | `~/.apollia/models/Qwen3-30B-A3B-Q4_K_M.gguf` | Local LLM model. The dev machine usually has this; override to another `.gguf` if needed. If the file is absent on disk, LLM-bound tests are SKIPped — never failed. |
| `APOLLIA_REQUIRE_RUNTIME` | `0` | Set to `1` to run Phase B (daemon spawn). |
| `APOLLIA_TEST_REVIEW` | `0` | Set to `1` to run `apollia-os review .` in Phase B (slow, opt-in). |
| `APOLLIA_TEST_VERBOSE` | `0` | Set to `1` to dump stdout/stderr (truncated to 300 chars) on each FAILed assertion. |

The script also forces a hermetic secret-storage backend so it can run inside
sub-shells that lack keychain access:

```sh
export APOLLIA_TOKEN_STORAGE=file
export APOLLIA_TOKEN_PASSPHRASE=cli-e2e-test-passphrase
export RUST_LOG=warn  # silence INFO tracing leaking on stdout
```

These can be overridden by setting them in your shell before running the
script — the defaults inside the script use the `:-` operator.

## Isolation

* `$HOME` is replaced with `mktemp -d -t apollia-cli-e2e.XXXXXX` for the
  duration of the script. The user's real `~/.apollia` is never touched.
* The `~/.apollia/models/` directory is **symlinked** to the real one (read
  paths) to avoid copying GGUF files into tmp. Databases (`projects.db`,
  `chat.db`, `governance.db`, `system.db`, `mcp.db`, `mcp_approvals.db`,
  per-namespace `memory/*.db`) live inside the tmp `$HOME` and are wiped on
  exit.
* The daemon in Phase B binds an auto-picked free TCP port and a
  tmp-located Unix socket — never `/tmp/apollia.sock:7771`.
* `trap` ensures the daemon is stopped (graceful → SIGTERM fallback) and
  the tmp dir is removed on EXIT / INT / TERM.

## What is **not** tested (and why)

These are intentional skips, documented inline in the script (`A.13 SKIP
justifiés`):

| Cmd | Reason |
|---|---|
| `chat` (REPL) | rustyline — interactive, non-scriptable. |
| `auth login` / `status` / `logout` | OAuth2 PKCE browser flow + OS keyring side-effects. |
| `update`, `update --check` | network (GitHub Releases). |
| `onboard`, `onboard --topic` | spawns an interactive chat agent. |
| `mcp-server`, `mcp-server --with-runtime` | long-running stdio server. |
| `model search`, `model show` | network (HF API). |
| `model delete --confirm` | destructive on a real `.gguf` — covered by unit tests. |
| `stt transcribe`, `stt model download` | network + whisper model. |
| `notify test` | sends a real desktop notification. |
| `agent install <git-url>` | network (Git clone). |
| `agent package install` | requires an external `agent.toml` package. |
| `mcp oauth login` | browser + callback. |
| `tools credentials test` / `set` | live calls + masked stdin prompt. |
| `chat-config authorizations list` / `revoke` | deferred v0.1.1 — runtime route absent (cf. `docs/internal/release/CLI-STATE.md` §3). |
| `mcp catalogue`, `mcp enrichments list` | deferred v0.1.1 — backend lives in `apollia-desktop`, exposing to CLI requires cross-crate refactor. |

## When a test fails

1. Re-run with `APOLLIA_TEST_VERBOSE=1` to dump stdout/stderr for the failing
   command:
   ```sh
   APOLLIA_TEST_VERBOSE=1 APOLLIA_REQUIRE_RUNTIME=1 bash tests/cli/cli-e2e.sh
   ```
2. The script prints the resolved tmp `$HOME` at the top — the daemon's
   stderr log lives at `$TMPDIR/daemon.log` for the duration of the run.
3. Phase B failures often boil down to environment: the `apollia-runner`
   sidecar not in `PATH`, no Python+SDK for the stub agent, the local LLM
   model file missing or unreadable. The script auto-skips those branches
   so a missing piece never blocks the suite; if you see a real FAIL it
   likely points at a regression in the CLI itself.

## Extending the suite

* Use the inline helpers — `check`, `check_exit`, `check_json`,
  `check_grep`, `skip` — for new assertions; they keep the PASS/FAIL/SKIP
  counters consistent.
* Add a new section under `Phase A` (LOCAL) for every new local-first
  surface; place runtime-dependent tests under `Phase B`.
* Update the coverage table above when adding domains.
* Phase B sections should gracefully `skip` rather than `check` when their
  hard dependencies (`AGENT_RUNNING`, `apollia-runner`, the model file) are
  not satisfied — never let an environment gap fail the suite.
