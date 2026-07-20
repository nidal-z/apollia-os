# Automation seed ecosystem

A deterministic, isolated Apollia data profile built from scratch so the `-det`
verification scripts find data instead of asserting empty states. The real
`~/.apollia` profile is never touched: the app runs under a throwaway `HOME`
whose `.apollia` is fully seeded.

## Run it

From the main checkout (the app builds there; testids match):

```sh
# deterministic suite, no model
just desktop-dev-automation-seeded .claude/worktrees/chantier-automation-cov/scripts/automation/master-det.json

# a single page
just desktop-dev-automation-seeded .claude/worktrees/chantier-automation-cov/scripts/automation/permissions-det.json

# inference scripts (real model from the real home, app under the seed home)
just desktop-dev-automation-seeded-llama .claude/worktrees/chantier-automation-cov/scripts/automation/chat-llm.json
```

The recipe runs `seed/build-seed.sh` (deriving it from the script path, so it
works from the worktree while the app runs from main), then launches the app
with `HOME=$PWD/.apollia-seed-home` (override via `APOLLIA_SEED_HOME`). Only
`HOME` is swapped; `CARGO_HOME` / `RUSTUP_HOME` are preserved so the build still
works. The seed dir is rebuilt (`rm -rf`) on every run, so it is always clean.

## Layout

- `schemas/<db>.sql` : authoritative DDL, dumped from a live migrated app. The
  builder strips the reserved `sqlite_sequence` line before applying.
- `fragments/<db>.sql` : INSERT-only seed rows per DB. A schema with no fragment
  is created empty.
- `files/` : on-disk artifacts copied verbatim into `<SEED_HOME>/.apollia/` :
  - `agents/<name>/` (+ `packages/`) : installed-agent dirs (manifest + agent.py).
  - `memory/<namespace>.db` : one memory DB per namespace (file-scoped store).
  - `models/*.gguf` : tiny placeholder GGUFs (the scanner stats name+size only).
  - `mcp-stub-server.py` : a stdlib stdio MCP server the seeded rows spawn (the
    connections sidebar only lists servers whose handshake succeeds at boot).
  - `apollia.toml` : pins the chat file-tool workspace to the repo.
- `build-seed.sh` : assembles the ecosystem (schema then fragment per DB, copy
  files, rewrite `install_path` / package `root_path` / the MCP stub path).

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
