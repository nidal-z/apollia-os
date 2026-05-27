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

Totals (latest run on dev machine, 2026-05-27 post bug fixes):

| Mode | PASS | FAIL | SKIP | Wall-clock |
|---|---|---|---|---|
| Phase A only | **180** | 0 | 15 | ~6 s |
| Phase A + B | **271** | 0 | 19 | ~18 s |

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

These are intentional skips documented inline in the script (`A.13` for
Phase A skips; in-place reasons for Phase B environment skips). Each
remaining skip falls in one of four categories: (a) interactive UI,
(b) outbound network, (c) deferred v0.1.1 items, (d) environment limits
the script can't synthesize.

### Phase A SKIPs (15)

| Cmd | Category | Reason |
|---|---|---|
| `chat` (REPL) | UI | rustyline editor needs a tty / pty |
| `auth login <provider>` | UI | spawns the browser at the AS authorize URL |
| `update` / `update --check` | Network | outbound HTTPS to api.github.com |
| `onboard` / `onboard --topic` | UI | chat-based onboarding agent |
| `mcp-server` / `--with-runtime` | Long-running | stdio JSON-RPC server, never returns |
| `model search` / `model show` | Network | HTTPS to huggingface.co |
| `stt transcribe` / `stt model download` | Network + model | needs Whisper model on disk (~1 GB HF download) |
| `notify test` | Side-effect | dispatches a real desktop notif / webhook POST |
| `agent install <git-url>` | Network | git clone |
| `agent package install` | External | needs an `agent.toml` bundle to install from |
| `mcp oauth login` | UI | AS authorize URL + browser callback |
| `tools credentials set` / `test` | UI + side-effect | masked stdin prompt + live backend call |
| `chat-config authorizations list` / `revoke` | Deferred v0.1.1 | in-memory daemon state, no HTTP route yet |
| `mcp catalogue` / `mcp enrichments list` | Deferred v0.1.1 | backend (`McpRegistryClient` + `enrichments.json`) lives in `apollia-desktop`; exposing to CLI requires moving the modules into `apollia-mcp` |

### Phase B SKIPs (4 additional, with daemon)

| Cmd | Reason |
|---|---|
| `agent repair e2e-hello` | the E2E stub agent is standalone — `agent repair` only fixes agents installed as part of an `agent_packages` bundle |
| `llm reload` + `llm ping` + `llm chat` | the `apollia-runner` sidecar isn't reachable inside the script's $HOME — the daemon falls back to UNAVAILABLE for model-bound ops. Pure metadata CRUD on backends still runs (created, updated, set-default, deleted in Phase B). |
| `resilience show bash_executor` + `reset bash_executor` | circuit-breaker registry is empty — bash_executor isn't registered until a real agent invokes it; the stub agent never does |
| `review .` | opt-in (`APOLLIA_TEST_REVIEW=1`) — spawns the heavy apollia-review agent, several minutes wall-clock |

The Phase B skips that **were** issues before the 2026-05-27 bug fixes
(trigger CRUD across kinds, notify CRUD, agent logs, auth status/logout)
are now executed and asserted — see commits `bfe1ab0f` and `6bfcc854`.

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
