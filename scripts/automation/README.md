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
just automation-seeded scripts/automation/master-det.json

# With local inference (llama-server, --jinja), pass a real model:
just automation-seeded-llama scripts/automation/chat-llm.json ~/.apollia/models/<model>.gguf

# Destructive suite (disposable seed HOME) needs the opt-in flag:
APOLLIA_AUTOMATION_ALLOW_DESTRUCTIVE=1 just automation-seeded scripts/automation/destructive.json

# The first launch, timed, on a blank home (see "The first launch, timed"):
just automation-first-run scripts/automation/first-run-llm.json ~/.apollia/models/<model>.gguf
```

The recipes swap `HOME` to a throwaway `.apollia-seed-home` (the real `~/.apollia`
is never touched) and build it from `tests/cli/seed/` (see
`tests/cli/seed/README.md`). Each run writes `.apollia-automation/report.json`
(the machine-readable verdict: `ok`, per-step `ok`/`detail`) plus per-step
screenshots. The app does not exit on its own; kill `cargo-tauri tauri dev`
once the report lands.

Network: both seeded recipes export `APOLLIA_MCP_REGISTRY_URL=http://127.0.0.1:1`,
so the MCP catalogue answers from the seeded registry cache instead of the
public registry. Without it a run longer than fifteen minutes (the cache TTL)
swaps the two seeded servers for the twenty thousand public ones mid-book, and
the connections section fails on network timing rather than on the product.

Model: `-seeded-llama` starts llama-server with `--jinja`, a 131072-token
context and one slot, and samples greedily on a fixed seed (`--temp 0 --seed
42`). Each is a setting of the justfile (`APOLLIA_LLAMA_CTX`,
`APOLLIA_LLAMA_SLOTS`, `APOLLIA_LLAMA_TEMP`, `APOLLIA_LLAMA_SEED`, see
`.env.example`):
a model book asserts what the model does next, so its answer has to be the
same from one run to the next; the seed's `apollia.toml` pins
`tool_turn_temperature = 0.0` for the same reason. For a dense model a
full 131072 KV cache can be heavy; override with `APOLLIA_LLAMA_CTX=32768`. The free-chat
prompt is ~13.5k tokens (the loaded tool surface), so keep a comfortable margin.

## The first launch, timed

`first-run-llm.json` holds the first launch to its promise: from the window to
the first result of a task in at most five minutes, downloads excluded. It
plays the seven steps of the first launch (language, AI, embeddings model, who
I am, first agent, connection, first task) with the example that qualifies
requests, launches the batch on the demonstration workbook, waits for the
first row read as succeeded, and fails past 300 seconds.

It does not run under the seeded recipes. The seed holds a backend already
declared and placeholder model files, so nothing of a first launch would be
measured there. Its own recipe builds a home with nothing in it but the two
model files, and removes that home at the start of every run:

```sh
just automation-first-run scripts/automation/first-run-llm.json \
  ~/.apollia/models/<model>.gguf [~/.apollia/models/multilingual-e5-small-q8_0.gguf]
```

- **The home** is `.apollia-first-run-home` at the root of the checkout,
  ignored by git. No `onboarding.completed_at`, no `apollia.toml`, no agent.
- **The two model files are cloned into its models folder**, not linked: the
  scan of that folder skips a symbolic link, and a hard link would let a write
  through to the real file. A copy-on-write clone (`cp -c` on APFS,
  `cp --reflink=auto` on Linux) costs no bytes; on a file system with neither
  the files are copied, and a large model then takes its time before the run.
  None of that is counted: the application is not started yet.
- **The embeddings file must be `multilingual-e5-small-q8_0.gguf`**, under
  that name. The embeddings step recognises a model by the file name the
  registry gives it, and the recipe imports that one through a stubbed picker.
  The recipe refuses before building anything when the file is missing.
- **The application loads the language model itself**, with the engine it
  finds, where the `-seeded-llama` recipe starts a `llama-server` beside it.
  That loading is part of a first launch and is counted. The engine takes a
  free port of its own, so only 5173 and 7771 have to be free.
- **The network is in reach.** The recipe does not point the registry at a
  dead address, and the agent of the example reads public pages while it
  qualifies. A run without the network measures a failure, not a first launch.

Read the verdict in `.apollia-automation/report.json`: `ok`, and the measure
under `captures.first_run_ms`, in milliseconds, recorded whether the budget
held or not. An overrun is a finding about the product, to report with its
number; the recipe is not where it gets fixed.

What the clock counts is said by its step, `expectElapsed` (see the script
contract below): the time since the webview started loading the interface.
What the process does before its window exists is not counted, and neither is
the compilation `cargo tauri dev` does first.

New conversation: the seed's `apollia.toml` sets `[desktop] chat_single_empty =
false`. With the default, on, "New conversation" opens the empty conversation
that already exists; the chat books leave conversations without a message
behind them and count on a fresh one at every press. The press opens a
conversation directly, and the agent or the model is chosen from the selector
of its header; the picker shows only when the conversation cannot be created,
which the chat book reaches by stubbing that command to refuse once.

## Running it on Linux and Windows

The harness is not macOS-bound: the runner is TypeScript inside the app, the
recipes are JSON and the seed is bash over sqlite3. One thing is macOS-only,
the window capture, and a `screenshot` step says so and carries on rather than
failing a book whose gestures all passed. What a clone on another machine
needs, measured rather than assumed:

- **A binary built against that bundle**: `just build-cli`. A plain
  `cargo build` picks the machine's interpreter, which PyO3 refuses above 3.13,
  and on Linux the recipe also bakes the path to the bundle's libpython into
  the binary so it starts outside the shipped launcher.
- **The Python bundle for that machine's triple.** The recipes look for
  `target/python-bundle/<triple>/python`, any triple. Without it the embedded
  interpreter is absent and every agent fails to load at boot, which reds a
  large share of the books for a reason that is not the product. Build it with
  `bash packaging/build-python-bundle.sh <triple> target/python-bundle/<triple>`;
  the script covers
  `*-unknown-linux-gnu` and `*-pc-windows-msvc` as well as Darwin.
- **The Tauri CLI**, which every desktop recipe drives:
  `cargo install tauri-cli --version '^2'`, the version the release uses. The
  recipes refuse without it before building anything.
- **Linux**: the webkit2gtk and audio development packages the CI job installs
  (see `cli-e2e` in `.github/workflows/ci.yml`), plus `sqlite3`.
- **Windows**: a bash (git-bash), `sqlite3` on PATH, and the WebView2 runtime.
  The recipes export `USERPROFILE` next to `HOME`, because `std::env::home_dir`
  reads the first on Windows and the second on Unix: without it the seeded
  throwaway home redirects nothing and the run writes into the real profile.

Then the same commands as on macOS:

```
just automation-seeded scripts/automation/nav-det.json      # boots in a minute
just automation-seeded scripts/automation/master-det.json   # the whole deterministic suite
```

The deterministic books never reach a model: the seed's GGUF and whisper files
are kilobyte placeholders, the recording commands are stubbed, and nothing
spawns llama-server. What a run of them proves on a new system is the whole
interface, the runtime, the agents and the seeded data; what it does not touch
is inference and dictation, which is where a platform usually breaks. One
model book covers the first:

```
just automation-seeded-llama scripts/automation/chat-llm.json /path/to/model.gguf
```

It needs `llama-server` on PATH (or `APOLLIA_LLAMA_BIN=/path/to/llama-server`) built
for that machine. Dictation has no automated path with a real engine on any
system: loading a whisper model and recording once is a manual check.

Read `.apollia-automation/report.json` for the verdict. A run on a machine with
no window capture reports its `screenshot` steps as skipped, so compare the
failed count, not the step count, with a macOS run.

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

The File column and the image names the published pages reference must be the
same set, because a misnamed file is invisible (the stale image simply stays)
and an unreferenced one ships to every visitor unserved. Nothing in CI compares
them; the publisher run without `--apply` is what names both halves of a
mismatch:

```sh
python3 scripts/automation/tools/publish_screenshots.py
```

`tests/cli/seed/load.sh` and `tests/cli/seed/unload.sh` put the seed into the
real profile and take it back out again. `load.sh` also picks up the narrative
overlay (`~/.apollia-seed-overlay`) when there is one, which is what fills the
timeline, the plans and the agentic conversation the images show. See
`tests/cli/seed/README.md`, section Overlay.

One English set is published into both locale directories:

```sh
python3 scripts/automation/tools/publish_screenshots.py --locale both --apply
```

## The canonical suite

- **Deterministic** (`<page>-det.json`, no model): one exhaustive book per surface
  (the walk with developer mode off and on, empty/error states, dialogs opened
  then cancelled, mutating controls driven to the boundary). `master-det.json`
  runs the 18 books of `regen_master.py`'s `ORDER` in one boot and is the
  release gate (4953 steps); `just automation-verdict` reads its report.
- **Standalone deterministic**: `onboarding-full`, `mailbox-det`, `destructive`,
  and four books written against the coverage report below, none of them in
  `regen_master.py`'s `ORDER` yet because none has been through a runtime round:
  `danger-guard-det` (the cancel half of the four destructive guards: step-1
  cancel, mismatch, match, lock hint, step-2 cancel, and no `-confirm` ever
  clicked), `profile-facts-det` (the off-schema memory facts of the profile
  page, added, edited, deleted by the script itself), `llm-advanced-det` (the
  developer fields of the server form, the timeout and the extra JSON
  parameters, each refusal played, cancelled without saving), `library-det` (the Library: the packages the
  application ships read from the real store, then a stubbed package of every
  kind opened, its update through a cancelled file chooser, its uninstall
  dialog cancelled, refused and accepted, the empty state and the unreadable
  list). Fold one into `master-det` by adding it to `ORDER`
  and re-running `regen_master.py --write`, after its first green solo run.
  Eight more books sit outside `ORDER` as well: `agent-create-det`,
  `agent-examples-det`, `example-trial-det`, `batch-launch-det`,
  `batch-view-det`, `workflows-det`, `developer-det` and `design-det`.
- **LLM** (need `-seeded-llama`): `chat-llm` (the flagship: tools, HITL, memory,
  config, plan mode, ask_user), `hitl-critical`, `coach-llm`,
  `chat-sanity`, `a2a`, `agents-a2a-llm`, `onboarding-llm`, `hitl-always-llm`,
  `plan-gate-llm`, and `master-llm` (an
  aggregate of chat/hitl/coach, 191 steps).
- **First launch** (needs `automation-first-run`, a blank home and
  two model files): `first-run-llm`, the timed first launch described above.
  Written against the source and statically validated; its first runtime
  round is still to come.

The whole corpus was last run green on 2026-07-28 against the redesigned UI:
14 suites, 3056 steps, 0 failures. The LLM ones ran on
`Qwen3.6-35B-A3B-MXFP4_MOE.gguf` with a 32768-token context and one slot.

The deterministic books went through a runtime round on 2026-09-06, page by
page and then as `master-det`, after a pass that turned observed anchors into
played ones wherever the seeded, throwaway HOME made the action safe. What a
book still leaves unplayed is written in its top-level `notes`, with the
reason measured on a run (a native picker, a network boundary, a narrow-layout
control, a state the seed cannot reach). `uncovered.py --list` names the rest.

## Script contract

A script is `{ name, stopOnError?, destructive?, steps: [...] }`. Step kinds
(see `crates/apollia-desktop/ui/src/lib/automation/types.ts` for the full typed
contract): `goto`, `waitFor`, `waitGone`, `click`, `fill`, `sendChat`, `expect`,
`captureText`, `screenshot`, `sleep`, `awaitTurn`, `setChecked`, `selectOption`,
`press`, `drag`, `stubInvoke`, `clearStubs`, `clearToasts`, `resizeWindow`, `fault`, `emitEvent`,
`expectElapsed`. Targets are an exact
`testid` or a `testidPrefix` (+ optional `nth`, negative counts from the end).
`awaitTurn` drives a chat/agent turn to completion and auto-accepts HITL cards;
`sendChat` targets the chat composer (`chat-input`).
`waitFor` takes an optional `contains`: the target must then be visible AND
read that fragment before the step passes, which is how a recipe waits for a
count or a status to change (the topbar's agents-at-work chip going back to
`1 agent`) instead of sleeping for a guessed duration.
`drag` presses on its target, moves the pointer to the centre of the element
named by `toTestid` (`toNth` picks one of several), and releases there. It
plays pointer events, which is how the application moves a conversation into a
folder: the drag and drop of HTML never reaches the page, because the window
keeps the drops of the system for itself.

Six kinds act on the harness rather than on the DOM:

- **`stubInvoke { command, resolve? | reject? | patch?, once?, argsMatch? }`**
  answers one Tauri command from the script. Every webview call, the
  plugin-dialog pickers included (`plugin:dialog|open`, `plugin:dialog|save`),
  goes through the `invoke` of `@tauri-apps/api/core` (aliased in the dev server to `tauriCoreShim.ts`, since the webview's own invoke is read-only), and the runner replaces that
  one function on the first `stubInvoke` of a run. Exactly one of `resolve`
  (the value returned; JSON `null` is a cancelled picker), `reject` (the payload
  the UI classifies, by `.kind` or by message text) or `patch` (the real call
  runs, these fields are merged over its result) is given, by key presence.
  `once` drops the stub after its first hit; `argsMatch` restricts it to calls
  whose argument object carries each listed key with a deep-equal value, so
  `get_chat_session` can be broken for one seeded session and left alone for
  the others. What the table does not answer reaches the backend unchanged,
  the runner's own `automation_*` calls included. The original `invoke` comes
  back when the run ends, whatever ended it. Two cautions: a stub proves the
  UI's handling of a shape the backend never produced, so a `resolve` that
  imitates a backend type (`PendingApproval`, `McpResourceSummary`) drifts
  silently when the type changes and should name the type in the script's
  `notes`; and a `patch` on `get_chat_session` also reaches `awaitTurn`'s own
  status poll, so never leave one armed across a turn.
- **`clearStubs { command? }`** drops the stubs of one command, or all of them.
  The wrapper stays until the run ends; a cleared command passes through again.
- **`resizeWindow { width, height }`** resizes the app window to a logical size
  through the dev-only `automation_resize` command. `tauri.conf.json` pins a
  minimum of 900 x 600 that the OS enforces, so the command lifts the minimum
  first and puts it back, from the same config, once the requested size
  satisfies it again. The runner waits 400 ms for WKWebView to reflow and the
  `matchMedia` listeners to fire, and restores 1280 x 800 when the run ends.
  Two bands matter: under 1024 px the chat panes become drawers, under 768 px
  the sidebar does. A narrow block must contain no boundary click (a native
  picker, a `window.confirm`): a hung step keeps the window narrow for the
  rest of the boot, and every later section assumes 1280 x 800.
- **`fault { name: "heartbeat-lost" }`** injects the one fault no IPC can ask
  for: the runtime heartbeat is an event the backend emits every five seconds,
  so the dev-only `simulateHeartbeatLoss` of `stores/runtimeHealth.ts` mutes
  the liveness listeners and fires the watchdog, which shows the disconnected
  banner. The banner's retry unmutes them and the next real heartbeat closes
  it, which is what the recipe asserts.
- **`emitEvent { event, payload? }`** emits a Tauri event from the webview, as
  the backend would; Tauri's JS `emit` reaches every listener, this webview's
  `listen` included. It is how a recipe delivers an event-borne request the
  product cannot produce on demand (a consent card on a seeded session, which
  only a live tool call raises). The payload imitates a backend type and
  the script's `notes` name it, for the same drift reason as a `stubInvoke`.

- **`expectElapsed { maxMs, as }`** holds a duration to a budget. It reads the
  time elapsed since the webview started loading the interface
  (`performance.now()`), fails when it exceeds `maxMs`, and records the
  measure, in milliseconds, under `captures[as]` of the report whichever way
  the verdict falls. Place it right after the wait that closes what is being
  timed, and keep screenshots after it: a window capture costs about a second
  on macOS and would be counted. The clock starts with the window, so the
  boot of the process before it and the compilation are outside the measure,
  and a reload of the webview starts it again.

Two more substitutions concern file inputs, whose chooser is served by wry as
a modal `NSOpenPanel` that would freeze the boot: a `fill` on an
`input[type=file]` builds one `text/plain` file named
`automation-attachment.txt` in the webview from the step's text (nothing is
read from disk), assigns it to `.files` and fires `change`, taking the input
without the visibility gate since such inputs are `display:none` by design;
and for the whole run the runner replaces `HTMLInputElement.prototype.click`
on file inputs by the same immediate pick, so the button that proxies the
input (the composer paperclip) plays end to end, then restores it.

Two substitutions ride on the boot: `goto` takes an optional `sessionId`, set
on `pendingChatSessionId` before the navigation (the order the command palette
uses), which is how a recipe reaches the session-not-found screen with an id no
table holds; and `${HOME}` in `fill.text` and in every string of a `stubInvoke`
value and of an `emitEvent` payload expands to the boot's `homeDir`, the seeded throwaway home the recipe
swapped in, so a picker stub can name `${HOME}/.apollia/models/<file>` without
knowing where the recipe put it. A boot without a home refuses the token rather
than expanding it to nothing.

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
  An exact `testid` resolves by equality only. A step that targets an instance
  id built from data (`automation-row-<seed id>`) is declared in the script's
  top-level `dynamicTestids` list; the declaration must sit under a dynamic
  prefix of the source and is refused when the id also resolves exactly. Step
  keys are validated per kind against `types.ts` (a `captureText` without `as`,
  or a `timeoutMs` on a kind the runner never reads it from, is an error).
  Exit codes: 0 clean, 1 problems, 2 nothing measured (empty corpus or no
  script, e.g. launched outside the repository root).
  What it cannot see is FLOW: an anchor that exists on another route, a panel
  that needs an extra click to open, a tab that kept its previous selection. A
  runtime round is the only way to catch those.
- **Regenerate `master-det`** after editing any `<page>-det.json`. `master-det`
  is a concatenation (a head, then per page `[section marker, goto chat,
  waitFor app-main]` + the page steps minus their 2-step onboarding preamble):
  ```sh
  python3 scripts/automation/tools/regen_master.py           # dry-run (checks it still matches)
  python3 scripts/automation/tools/regen_master.py --write   # rewrite the file
  ```
- **What is NOT covered**: the mirror of `validate.py`. It reads the same
  corpus (it imports `build_corpus`), sorts the anchors into gestures and
  markers, and reports, per page, the gestures no step ever acts on:
  ```sh
  python3 scripts/automation/tools/uncovered.py            # table by page
  python3 scripts/automation/tools/uncovered.py --list --page chat
  python3 scripts/automation/tools/uncovered.py --families # the id-built-from-data anchors
  python3 scripts/automation/tools/uncovered.py --max-uncovered 197   # guard mode
  ```
  A gesture is covered only by an acting step (`click`, `fill`, `setChecked`,
  `selectOption`, `press`, plus the implicit `chat-input` / `chat-send-button`
  of `sendChat`); a `waitFor` on a button proves the button exists, not that
  anyone can press it. Exit codes: 0 measured and within budget, 1 a defect
  (over `--max-uncovered`, or an anchor it resolves that the validation corpus
  does not), 2 nothing measured.
  Read the `--origin` flag before quoting a number. The default counts only
  ids written literally in the source. `--origin all` adds the ids
  reconstructed from a file's string literals, and that bucket is noisy by
  construction: the danger zone appears to hold a hundred anchors, of which
  about ninety are the confirm-dialog suffixes applied to class names caught in
  the same literal pool (`rounded-xl-confirm-input`). The page really holds
  four actions.
- **Analyse a run**: group the failed steps of a report by section:
  ```sh
  python3 scripts/automation/tools/analyze_report.py [report.json] [script.json]
  ```
- **Scan variant** after a large UI refactor: a fail-fast copy of a suite whose
  wait timeouts are capped, so one boot enumerates every broken anchor in
  minutes instead of hours. Data only, no runner change:
  ```sh
  python3 scripts/automation/tools/make_scan.py            # master-det -> master-det-scan, cap 3000ms
  just automation-seeded scripts/automation/master-det-scan.json
  ```

## The language a run speaks

Every book asserts English sentences, and the run states English rather than
inheriting it: the runner pins the locale before the first step. Left alone,
the application takes its language from a stored preference and failing that
from the machine, so the same book read English on macOS and French under
Ubuntu, and two journal assertions failed on a build that was correct. Assert
an English string freely; a book that needs another language drives the
language buttons itself.

## What a run writes into this repository

The seeded run redirects `HOME`, not the chat workspace: the seed's database
fragments point the seeded project and its governance rows at this checkout
(`__APOLLIA_SEED_WORKSPACE__`, expanded by `tests/cli/seed/build-seed.sh`), so
a file the model is
asked to create lands here rather than in the throwaway home. The model books
write under `tmp/` and `notes/`, both ignored by git. Four of those files were
once committed and read as leftovers in a public tree; nothing creates them but
a run, and nothing reads them back.

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
- **A native file dialog is answered by a stub, or not at all.**
  `memory-export-button`, `memory-import-pick` and `agents-install-apo-button` open an OS picker the runner cannot drive: an unstubbed
  click never returns and every later step of the boot fails. Arm
  `stubInvoke { command: "plugin:dialog|open", resolve: "${HOME}/...", once: true }`
  (`plugin:dialog|save` for the export) right before the click, or `resolve:
  null` for a cancelled picker. The chat composer is the exception: its attach
  button clicks a hidden `<input type=file>`, which is not an IPC and stays a
  boundary. Never point an update picker at the agent's own installed module:
  `std::fs::copy` onto the same path truncates the file before copying it.
- **An inline confirm may have no cancel testid.** Arming the agent uninstall
  swaps the header for a confirm panel whose Cancel button carries none, so the
  only scripted way out is to select another agent, which the route uses to
  retire the armed state. Same shape wherever a confirm is rendered inline
  rather than as a dialog: check that the way back is reachable before writing
  the arming click.
- **One consent card**: file tools ask on the same card as every other tool,
  with three answers and an optional reason. Developer mode off (the seed
  default), it is `OperatorApprovalCard` (`operator-approval-*`, then
  `operator-approval-accept-*`, `-always-*`, `-reason-*` and `-refuse-*`); on,
  the raw `ApprovalCard` (`approval-card-*`, `approval-accept-*`,
  `approval-always-*`, `approval-reason-*`, `approval-refuse-*`). There is no
  separate file prompt. `bash_executor` is auto-refused with no card.
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
- **The plan gate has two cards.** `ChatPlanHost` renders `ChatPlanReview` with
  developer mode off (`chat-plan-review`, request-changes plus an adjust
  textarea) and `ChatPlanReviewBuilder` with developer mode on
  (`chat-plan-review-builder`, approve or reject only). A script that asserts
  one must pin developer mode first, and it should
  end on an approval: leaving the plan pending never exercises the gate.
- **`ask_user` cannot be answered by a fixed step.** The model picks the question
  type (open, single, multi) and `ask-user-submit` stays disabled until every
  question is answered. Use `ask-user-skip`, which is type-independent. Leaving the
  card pending keeps the session `processing`, which silently hides every
  control a busy conversation disables downstream.
- **Sizing `awaitTurn`.** `maxApprovals` defaults to 25. The plan-review steps use 6
  on purpose, but the turn that FOLLOWS an approval executes the whole plan and needs
  the full budget, otherwise it aborts as a runaway turn.
- **Never run the app through a symlinked `target/`.** Tauri caches
  `current_exe` at load and refuses a path that traverses a symlink, so
  `resource_dir()` errors for the whole life of the process. The only visible
  symptom is the settings CLI card quietly not rendering (`bundled` is derived
  from `resource_dir`), which reads as a product regression. In a worktree
  whose `target` is a symlink to the main tree's, export
  `CARGO_TARGET_DIR=<real target path>` so cargo launches the binary through
  the real path.
- **One recipe invocation can run the script twice.** When the app exits after
  writing the report, the `tauri dev` watcher may relaunch it (a vite temp
  file counts as a change), and the second pass replays the script on the
  already-mutated seed HOME, then overwrites `report.json` with its verdict.
  Read or copy the report as soon as it lands, and kill the recipe's process
  tree before starting the next run.
