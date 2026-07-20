# Desktop E2E automation

Dev-only gestural end-to-end tests for the Tauri desktop app. macOS has no
WebDriver for WKWebView, so the harness drives the **real** app by injecting a
declarative JSON script that acts on the DOM through stable `data-testid`
selectors, against a real backend (and, for the `-llama` recipes, real local
inference). The runner, the Rust capture command, and this whole surface are
gated behind `debug_assertions` / `import.meta.env.DEV` and tree-shaken out of
any release build.

## Run it

Everything runs from the repo root, from `main` (the app must match the scripts'
testids). Always free the ports first, an editor or a stale run holds them:

```sh
lsof -ti :5173 :8899 | xargs kill -9 2>/dev/null   # vite + llama-server

# Deterministic (no model), seeded ecosystem:
just desktop-dev-automation-seeded scripts/automation/master-det.json

# With local inference (llama-server, --jinja), pass a real model:
just desktop-dev-automation-seeded-llama scripts/automation/chat-llm.json ~/.apollia/models/<model>.gguf

# Destructive suite (disposable seed HOME) needs the opt-in flag:
APOLLIA_AUTOMATION_ALLOW_DESTRUCTIVE=1 just desktop-dev-automation-seeded scripts/automation/destructive.json
```

The recipes swap `HOME` to a throwaway `.apollia-seed-home` (the real `~/.apollia`
is never touched) and build it from `seed/` (see `seed/README.md`). Each run
writes `.apollia-automation/report.json` (the machine-readable verdict:
`ok`, per-step `ok`/`detail`) plus per-step screenshots. The app does not exit
on its own; kill `cargo-tauri tauri dev` once the report lands.

Model: `-seeded-llama` defaults `CTX=131072 NP=1 --jinja`. For a dense model a
full 131072 KV cache can be heavy; override with `CTX=32768`. The free-chat
prompt is ~13.5k tokens (the loaded tool surface), so keep a comfortable margin.

## The canonical suite

- **Deterministic** (`<page>-det.json`, no model): one exhaustive book per surface
  (operator + builder walk, empty/error states, dialogs opened then cancelled,
  mutating controls driven to the boundary). `master-det.json` runs all 20 in one
  boot and is the release gate (currently 2116/2116).
- **Standalone deterministic**: `onboarding-full`, `mailbox-det`, `destructive`.
- **LLM** (need `-seeded-llama`): `chat-llm` (the flagship: tools, HITL, memory,
  config, plan mode, ask_user, step budget), `hitl-critical`, `coach-llm`,
  `chat-sanity`, `a2a`, `agents-a2a-llm`, `onboarding-llm`, and `master-llm` (an
  aggregate of chat/hitl/coach).

## Script contract

A script is `{ name, stopOnError?, destructive?, steps: [...] }`. Step kinds
(see `crates/apollia-desktop/ui/src/lib/automation/types.ts` for the full typed
contract): `goto`, `waitFor`, `waitGone`, `click`, `fill`, `sendChat`, `expect`,
`captureText`, `screenshot`, `sleep`, `awaitTurn`, `setChecked`, `selectOption`,
`press`. Targets are an exact `testid` or a `testidPrefix` (+ optional `nth`,
negative counts from the end). `awaitTurn` drives a chat/agent turn to completion
and auto-accepts HITL cards; `sendChat` targets the chat composer (`chat-input`).

## Maintenance

- **Validate** every script parses and every kind / route / testid resolves
  against the current UI source (run after any script or UI change):
  ```sh
  python3 scripts/automation/tools/validate.py
  ```
- **Regenerate `master-det`** after editing any `<page>-det.json`. `master-det`
  is a concatenation (a head, then per page `[section marker, goto dashboard,
  waitFor app-main]` + the page steps minus their 2-step onboarding preamble):
  ```sh
  python3 scripts/automation/tools/regen_master.py           # dry-run (checks it still matches)
  python3 scripts/automation/tools/regen_master.py --write   # rewrite the file
  ```
- **Analyse a run**: group the failed steps of a report by section:
  ```sh
  python3 scripts/automation/tools/analyze_report.py [report.json] [script.json]
  ```

## Gotchas (learned the hard way)

- **Dismiss onboarding first.** The seed sets no `onboarding.completed_at`, so
  the onboarding modal opens at boot and sits on top of everything. The runner
  clicks by testid and bypasses pointer occlusion, so DOM asserts pass while the
  modal masks the UI. Every script starts with a `click onboarding-skip` +
  `waitGone` preamble (except `onboarding-*`, which drive the modal). `validate.py`
  does not enforce this, keep it in mind when adding a script.
- **Screenshots capture the app window by id** (`screencapture -l`), so an editor
  or launcher on top no longer pollutes them. If a capture is ever wrong, check
  the window is a normal (non-minimized) window.
- **Two HITL surfaces**: file tools show the generic `ApprovalCard`
  (`approval-card-*`, `approval-accept-*`, ...), not the `hitl-fs-modal`
  fast-path. `bash_executor` is auto-refused with no card.
- **The seed default backend** is `local-llama-server` (an OpenAI-compatible
  backend pointing at the llama-server on :8899). Deterministic runs never
  connect to it; the `-llama` runs route real inference through it.
- **captureText** races the final render, so `captures.*` is often empty even
  when the reply is correct on screen. Verify by screenshot.
- **Destructive**: factory reset must be cancelled after typing its confirm word,
  or it quits the app before the report is written.
