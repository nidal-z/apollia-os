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
lsof -ti :5173 -ti :7771 -ti :8899 | xargs kill -9 2>/dev/null   # vite, runtime API, llama-server

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

## Taking the documentation screenshots

`SCREENSHOTS.md` next to this file is the shooting script: one row per image,
with its route, its gesture, the state the seed puts on screen, the exact values
to type where a value is needed, and the destination filename. Read it before
anything else on a shooting day.

The 85 images are shot two ways, and the script's **How** column says which:

- **64 by the automaton**, two of them only once a pending item exists.
  `screenshots-en.json` (61 capture labels) and `screenshots-en-llm.json` (5)
  drive the real app by testid. The runner frames the whole window, which is
  looser than the crops the script describes, so these are a baseline to re-crop
  rather than a finished set.
- **21 by hand.** Twenty for a named reason: a native folder dialog, a real
  Google consent screen, a download whose useful instant lasts seconds, a live
  model turn, and the inbox pending list, which `list_pending_approvals` reads
  from an in-memory set no seed can reach. The twenty-first is deterministic and
  by hand only because its crop is tighter than the whole window.
  `SCREENSHOTS.md` names all the reasons and says how to provoke what can be
  provoked.

The 66 capture labels and the 64 `auto` rows answer different questions and are
not meant to match: three labelled rows still need their state provoked first,
and one unlabelled row is captured as a side effect. `SCREENSHOTS.md` reconciles
the two counts row by row.

The File column and the image names the published pages reference are checked
against each other in CI, because a misnamed file is invisible (the stale image
simply stays) and an unreferenced one ships to every visitor unserved:

```sh
python3 scripts/check_screenshot_script.py             # the two sets agree
python3 scripts/check_screenshot_script.py --self-test # and the check can fail
```

`seed/load.sh` and `seed/unload.sh` put the seed into the real profile and take
it back out again. `load.sh` also picks up the narrative overlay
(`~/.apollia-seed-overlay`) when there is one, which is what fills the timeline,
the plans and the agentic conversation the images show. See `seed/README.md`,
section Overlay.

One English set is published into both locale directories:

```sh
python3 scripts/automation/tools/publish_screenshots.py --locale both --apply
```

## The canonical suite

- **Deterministic** (`<page>-det.json`, no model): one exhaustive book per surface
  (operator + builder walk, empty/error states, dialogs opened then cancelled,
  mutating controls driven to the boundary). `master-det.json` runs all 21 in one
  boot and is the release gate (2226 steps). `tour-det` is the newest section:
  the Getting started band and the guided tour (entry points, step navigation,
  the anchorless fallback, the exit confirmation, finishing).
- **Standalone deterministic**: `onboarding-full`, `mailbox-det`, `destructive`.
- **LLM** (need `-seeded-llama`): `chat-llm` (the flagship: tools, HITL, memory,
  config, plan mode, ask_user, step budget), `hitl-critical`, `coach-llm`,
  `chat-sanity`, `a2a`, `agents-a2a-llm`, `onboarding-llm`, `tour-llm` (act 1 of
  the guided tour, annotated live on a real conversation), and `master-llm` (an
  aggregate of chat/hitl/coach/tour, 234 steps). The tour section sits last so it
  cannot perturb the three proven ones, and it forces operator mode because the
  approval card is persona-picked.

The whole corpus was last run green on 2026-07-28 against the redesigned UI:
14 suites, 3056 steps, 0 failures. The LLM ones ran on
`Qwen3.6-35B-A3B-MXFP4_MOE.gguf` with `CTX=32768 NP=1`.

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
  It reproduces what the UI renders, including ids a shared component composes
  off the one it is given (`${dataTestId}-input`) and ids a route builds from
  its own literals (`` `${action.id}-btn` ``, `{testid}-{opt.value}`). A
  `testidPrefix` step is checked strictly: something in the corpus must really
  start with it, because that is what `[data-testid^="..."]` does at runtime.
  What it cannot see is FLOW: an anchor that exists on another route, a panel
  that needs an extra click to open, a tab that kept its previous selection. A
  runtime round is the only way to catch those.
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
- **Scan variant** after a large UI refactor: a fail-fast copy of a suite whose
  wait timeouts are capped, so one boot enumerates every broken anchor in
  minutes instead of hours. Data only, no runner change:
  ```sh
  python3 scripts/automation/tools/make_scan.py            # master-det -> master-det-scan, cap 3000ms
  just desktop-dev-automation-seeded scripts/automation/master-det-scan.json
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
- **Leave a settings sub-page clean.** The routes wired with an explicit save
  (`profile`, `stt`, `observability`) arm the nav guard: leaving one dirty opens
  `settings-unsaved-dialog`, a modal that no later step dismisses, and every
  remaining section of `master-det` then fails. Save or discard before moving on.
- **A save button only enables on a real change.** Picking the value a field
  already holds leaves the form clean, so `settings-subpage-save` stays disabled
  and the click silently does nothing. Choose values that differ from the seed
  AND from what an earlier section persisted.
- **Tabs keep their selection across a change of subject.** The connection
  detail and the catalogue sheet reopen on whatever tab was last used, so
  re-select the one you assert on instead of assuming the default.
- **Free three ports for real, and check that they are free.** Two runs out of
  five failed to start on this alone. `pkill` on the tauri and app processes
  returns before the sockets are released, so a `pkill` followed by an immediate
  relaunch dies on `beforeDevCommand` (5173) or on the embedded runtime
  (`failed to bind TCP on port 7771: Address already in use`). The ports are:

  | Port | Held by | Symptom when still held |
  |---|---|---|
  | 5173 | the vite dev server | `Port 5173 is already in use`, the run never starts |
  | 7771 | the embedded runtime's API server | the app launches, the runtime does not, and every step fails against a dead backend while still writing screenshots |
  | 8899 | the local llama-server, `-llama` runs only | the model backend is unreachable |

  7771 is the one that costs the most, because the run *looks* like it worked:
  it produces a full set of `fail-*` captures rather than refusing to start.

  ```sh
  # Kill, then WAIT and verify. The verification is the part people skip.
  pkill -9 -f 'tauri|vite|apollia-desktop'
  lsof -ti :5173 -ti :7771 -ti :8899 | xargs kill -9 2>/dev/null
  sleep 3
  lsof -ti :5173 -ti :7771 -ti :8899   # must print nothing before relaunching
  ```

  Note the repeated `-ti`. `lsof -ti :5173 :7771` reads everything after the
  first port as a file name, errors on it, and prints nothing at all, so it
  reports every port as free. That form is worse than useless: it is a check
  that always passes.
- **The plan gate has two cards.** `ChatPlanHost` renders `ChatPlanReview` for the
  operator (`chat-plan-review`, request-changes plus an adjust textarea) and
  `ChatPlanReviewBuilder` for the builder (`chat-plan-review-builder`, approve or
  reject only). A script that asserts one must pin the mode first, and it should
  end on an approval: leaving the plan pending never exercises the gate.
- **`ask_user` cannot be answered by a fixed step.** The model picks the question
  type (open, single, multi) and `ask-user-submit` stays disabled until every
  question is answered. Use `ask-user-skip`, which is type-independent. Leaving the
  card pending keeps the session `processing`, which silently hides every
  read-only-gated control downstream (`config-save` and friends).
- **Sizing `awaitTurn`.** `maxApprovals` defaults to 25. The plan-review steps use 6
  on purpose, but the turn that FOLLOWS an approval executes the whole plan and needs
  the full budget, otherwise it aborts as a runaway turn.
