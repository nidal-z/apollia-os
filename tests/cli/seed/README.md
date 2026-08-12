# Seed fixture for the CLI end-to-end suite

A deterministic, isolated Apollia data profile built from scratch, so the suite
asserts known content instead of empty states. The real `~/.apollia` profile is
never touched: everything runs under a throwaway `HOME` whose `.apollia` is
fully seeded.

## Run it

```sh
# build the fixture into a throwaway HOME
bash tests/cli/seed/build-seed.sh /path/to/throwaway-home

# check the fixture itself is intact (what CI runs)
bash tests/cli/seed/self-test.sh

# the suite that consumes it
bash tests/cli/cli-e2e.sh
```

`cli-e2e.sh` calls the builder itself, so you rarely invoke it by hand. Only
`HOME` is swapped; `CARGO_HOME` and `RUSTUP_HOME` are preserved so a build still
works underneath. The seed directory is rebuilt from nothing on every run, so it
is always clean.

To load the fixture into your own profile for manual inspection, `load.sh` backs
up what is there and `unload.sh` puts it back. Once the seed is in place,
`load.sh` checks that every absolute path it names exists, and stops rather than
leave you with a seed that looks loaded and shows empty screens; your own
profile is still in `~/.apollia.before-seed` at that point.

## Layout

- `schemas/<db>.sql` : authoritative DDL, dumped from a live migrated app. The
  builder strips the lines SQLite reserves for itself before applying: the
  `sqlite_sequence` table, and the shadow tables of every FTS5 virtual table
  (`<name>_fts_data`, `_idx`, `_content`, `_docsize`, `_config`). A dump names
  both, and both are built by something else. Whether replaying the shadow
  tables is an error depends on the sqlite3 build rather than on how recent it
  is: measured on the unfiltered dump, 3.40.1 and 3.51.0 accept those lines,
  3.45.1 and 3.46.1 refuse them. The builder strips them either way, so no
  caller has to know which one it has.
- `fragments/<db>.sql` : INSERT-only seed rows per DB. A schema with no fragment
  is created empty.
- `files/` : on-disk artifacts copied verbatim into `<SEED_HOME>/.apollia/` :
  - `agents/<name>/` (+ `packages/`) : installed-agent dirs (manifest + agent.py).
  - `memory/<namespace>.db` : one memory DB per namespace (file-scoped store).
    The file stem IS the namespace, and a project namespace carries a colon,
    which no Windows checkout can hold, so those file names store it percent
    encoded (`%3A`) and the builder decodes them into the throwaway HOME.
  - `models/*.gguf` : tiny placeholder GGUFs (the scanner stats name+size only).
  - `mcp-stub-server.py` : a stdlib stdio MCP server the seeded rows spawn (the
    connections sidebar only lists servers whose handshake succeeds at boot).
  - `apollia.toml` : pins the chat file-tool workspace to the repo.
- `build-seed.sh` : assembles the ecosystem (schema then fragment per DB, apply
  the overlay, copy files, rewrite `install_path` / package `root_path` / the
  MCP stub path).
- `self-test.sh` : a few seconds, sqlite3 only. Asserts the base row counts CI
  depends on, that an overlay is applied when asked for and never otherwise,
  that a missing overlay stops the build, that every memory file stem resolves
  to its own rows, and that the paths a seed names exist. That last one is
  checked in both directions: on the seed just built, which must pass, and on a
  seed built without the home alias and then moved, which must fail. Its exit
  code is the count it prints. Runs in the `cli-e2e` CI job.
- `self-test-paths.sh` : resolves every absolute path the seeded databases name.
  Takes the directory to check as a required argument, and exits 2 when called
  without one, so "nothing was measured" never reads as "all is well". Called by
  `self-test.sh` on the seed it builds, and by `load.sh` on the seed it lays
  down.

## Environment

| Variable | Default | Effect |
|---|---|---|
| `APOLLIA_SEED_OVERLAY` | unset | Directory of extra `schemas/`, `fragments/` and `files/` applied on top of the checked-in seed. Set but missing is a hard error. |
| `APOLLIA_SEED_PROJECT_ROOT` | this checkout | What `__APOLLIA_SEED_WORKSPACE__` expands to: the path the seeded project, provider and permission rows display. |
| `APOLLIA_SEED_HOME_ALIAS` | `SEED_HOME` | What `__APOLLIA_SEED_HOME__` expands to. `load.sh` sets it, because it builds into a staging directory and moves the result elsewhere. |

## Overlay

The checked-in seed is a **test fixture**. `tests/cli/cli-e2e.sh` runs on every
pull request and asserts its exact contents (project name, session ids, MCP
server names), and the desktop `-det` suite asserts its row counts. It therefore
stays small, stable, and public.

The **narrative** seed is a different artifact: a coherent usage history whose
job is to make every documentation screenshot show something credible and
consistent with the neighbouring pages. It belongs to whoever shoots the
screenshots, it changes with the story they want to tell, and it has no business
in a public repository. It lives outside the checkout:

```sh
~/.apollia-seed-overlay/          # the default, picked up by load.sh
  schemas/<db>.sql                # databases the checked-in seed has none of
  fragments/<db>.sql              # extra rows, replayed AFTER the base fragment
  files/{agents,memory,models}/   # extra on-disk artifacts, copied over the base
  files/apollia.toml              # replaces the base config wholesale, if present
```

Same placeholders as the checked-in fragments (`__APOLLIA_SEED_WORKSPACE__`,
`__APOLLIA_SEED_HOME__`), expanded identically.

Who turns it on:

- `load.sh`, the human screenshot path, turns it on by default when
  `~/.apollia-seed-overlay` exists, and prints which one it used. It prints just
  as clearly when there is none.
- `build-seed.sh` called directly, by CI and by the `just` recipes, leaves it
  off unless `APOLLIA_SEED_OVERLAY` is set. That is deliberate: an operator with
  an overlay in their home must still be able to run the assertion suite and get
  the same counts CI gets.

**If you cloned this repository, you do not have an overlay, and there is no way
to get one.** Nothing here depends on it: the CLI end-to-end suite runs on the
checked-in seed alone, which is the only configuration it is ever asserted
against. What an overlay adds is a nicer story for documentation screenshots,
which is not a test concern.

## What is seeded (per subsystem, traced to the real read-path)

| DB / files | Rows | Unblocks |
|---|---|---|
| projects.db | 2 projects + 3 providers + 1 doc | project rows, settings fields, provider cards |
| triggers_def.db + triggers.db | 4 triggers (active x2, paused x2) + history | automation rows, filters, delete/history |
| governance.db | 4 permission rules (all scopes) + 5 audit + 1 credential | permissions count/list/audit/revoke-all, brave credential |
| chat.db | 4 sessions + messages + 3 tool auths + 1 plan | session list, session-auths, plan tab |
| agents.db + agents/ | 4 agents (incl apollia-chat) + 1 package | agent rows, detail, package rows |
| user_memory + memory/*.db | 5 namespaces, 3 types each | namespace items, category chips, type tabs, entries |
| system.db | 3 llm backends (1 default) + stt config | llm cards, set-default, stt form |
| llm_calls.db | 5 recent calls (3 backends) | llm-stats-table, observability llm-costs |
| models/*.gguf | 2 (Phi deletable, Qwen in-use) | installed-model rows + delete dialog |
| notifications.db | 2 channels + 3 global events + 6 logs | channel cards, global events, activity |
| hitl.db | 7 tasks (every state) + 2 approvals | task rows, all filter chips, detail tabs |
| mcp.db + stub | 2 servers (live via stub) + 4 transcripts | mcp sidebar, detail tabs, tools; transcript rows |

## All-green pass (2026-07-17)

After the seed landed, every `-det` script was rewritten to assert the deterministic
seeded state (not best-effort empty/populated probes), so a seeded run aims for a
fully-green `report.json`. Supporting changes:

- Two engine primitives added (dev-only runner, tree-shaken from prod):
  `selectOption {value|labelText|index}` drives native `<select>` (38 uses across the
  suite, unblocking the custom-MCP transport chain, notification webhook/throttle,
  llm remote-provider fields, profile/stt selects, permissions filters/add-rule,
  observability filters); `press {key, meta/ctrl/shift/alt}` dispatches keystrokes
  (Escape to close sheets, palette).
- App testids added so previously-unreachable flows are drivable:
  `projects-new-project-btn`, `project-delete-confirm`/`provider-delete-confirm`
  (ConfirmDialog `data-testid`), `automations-new-btn`, and `command-item-{action-id}`
  (which makes the companion/coach reachable via the palette `command-item-companion.toggle`).
- `setChecked` is role-aware (native input OR `<button role=checkbox/switch>`).

Each script asserts populated data where seeded and the correct EMPTY state where a
subsystem is intentionally not seeded (e.g. observability timeline/audit/plan-cache).
True external boundaries (mic, native file/OAuth dialogs, live network test buttons,
HuggingFace search) are reduced to `waitFor(button)+screenshot`, never a failing
click or result-assert.

## Not seedable (documented residuals, not seed defects)

Genuinely non-deterministic or machine-scoped state (reduced to screenshot boundaries,
never a failing assert):

- `automations-filter-error` : the UI derives status from `enabled` only, never "error" (the error chip asserts its empty-filter state instead).
- `inbox-row-primary` (pending) : reads an in-memory approval set, not rehydrated from the DB at boot (the pending tab asserts its empty state).
- `stt-status-banner` / `stt-model-name` / running-agent rows / `agent-detail-start/stop/logs` : need a live engine or a runtime agent uuid (auto-load at boot is not deterministic).
- `catalogue-entry-*` (MCP discover) + model-hub HF search : live HuggingFace/registry fetches.
- OAuth accounts (machine Keychain, not HOME-scoped) and the MCP disclaimer/wizard (gated by `localStorage`, which WKWebView resolves via the real `~/Library`, not the swapped HOME).
- `chat-open-sessions-button` (`lg:hidden`), `tasks-fab` / `settings-mobile-nav-*` (mobile/drawer-only): absent at desktop width, so removed.
- Radios (`profile-radio-*`): the testid is on the group container; individual `RadioItem`s have no testid and there is no radio-option primitive, so the group is asserted present only.

These are the only remaining non-green-able steps, and each is handled so it cannot fail
(screenshot boundary or empty-state assertion). Everything else is deterministic-green.

## Notes

- Deterministic: fixed ids, fixed `2026-07-01` timestamps, except `llm_calls`
  (uses `datetime('now', ...)` because the cost aggregate filters a rolling
  recent window; the seed is rebuilt per run).
- Onboarding is not seeded as completed; every `-det` script begins with an
  `onboarding-skip` step that dismisses the first-launch modal deterministically.
- The MCP stub needs `/usr/bin/python3` (present with Xcode CLT, already a build
  dependency).
