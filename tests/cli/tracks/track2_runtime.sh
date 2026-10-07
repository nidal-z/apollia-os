# shellcheck shell=bash
# tests/cli/tracks/track2_runtime.sh - deterministic RUNTIME track.
#
# The daemon boots on a freshly seeded $HOME, so runtime read commands return
# seeded state (4 agents, 4 triggers, 2 notify channels, 3 LLM backends, 2 live
# MCP servers via the bundled stub). Covers the runtime-only leaves and CRUD
# lifecycle. Model-bound / non-deterministic commands (run, chat, llm chat,
# task cancel/resume, a2a invoke) live in Track 3.
#
# Sourced by cli-e2e.sh with the daemon already up on $SOCK; uses $BIN, $SOCK,
# the seeded $HOME, $RUN_TMP and the assert helpers. Environment gaps (Python
# runner cannot spawn the stub agent) degrade to `skip`, never a hard failure.

Q=("$BIN" --socket "$SOCK")   # convenience prefix

# ── seeded runtime reads (content assertions) ───────────────────────────────
section "seeded runtime reads"
check         "status (daemon on)"                 "${Q[@]}" status
check_json    "status --json"                      "${Q[@]}" status --json
# The orchestrator owns the daemon lifecycle, so the track exercises `start`
# through its other deterministic path: a second start on the held socket
# refuses fast, before binding anything.
check_exit    "start (already running) → 1"  1     "$BIN" start --socket "$SOCK" --port 0
check_content "status shows active seeded agents" "active"  "${Q[@]}" status
check_content "agent list has apollia-chat"  "apollia-chat"  "${Q[@]}" agent list
check_content "agent list has seed-classifier" "seed-classifier" "${Q[@]}" agent list
check         "agent show apollia-chat"            "${Q[@]}" agent show apollia-chat
check_json    "agent show --json"                  "${Q[@]}" agent show apollia-chat --json
check_content "trigger list has daily-digest" "seed-trigger-daily-digest" "${Q[@]}" trigger list
check_content "notify list has seed-channel-desktop" "seed-channel-desktop" "${Q[@]}" notify list
# The seed writes exactly three backends (seed/fragments/system.sql):
# local-llama-server (the default), vllm-qwen and gpu-server.
# Assert the default and one non-default, so the list is proven to carry more
# than the row Track 3 repoints.
check_content "llm backends lists local-llama-server" "local-llama-server" "${Q[@]}" llm backends list
check_content "llm backends lists gpu-server" "gpu-server"  "${Q[@]}" llm backends list
check_content "mcp list has filesystem connected" "filesystem"  "${Q[@]}" mcp list
check_content "notify events get has task.completed" "task.completed" "${Q[@]}" notify events get
# A seeded agent is on the card under its identifier, read from the store with
# nothing started.
check_content "a2a skills exposes agent:seed-classifier" "agent:seed-classifier" "${Q[@]}" a2a skills
skip          "a2a invoke agent:seed-classifier" "an agent answers through a model; the invocation of a packaged workflow, which needs none, is asserted in the packaged phase"

# Empty-but-valid runtime reads (seeded elsewhere, not surfaced by these paths).
check         "task list (empty ok)"               "${Q[@]}" task list
check         "audit list (empty ok)"              "${Q[@]}" audit list --limit 5
check_json    "audit stats --json"                 "${Q[@]}" audit stats --json
check_exit    "audit show (unknown id) → 1"  1     "${Q[@]}" audit show nonexistent-run
check         "resilience list"                    "${Q[@]}" resilience list
check         "model hardware (daemon on)"         "${Q[@]}" model hardware
check_json    "model hardware --json"              "${Q[@]}" model hardware --json
check_content "model embeddings show (none set)" "keywords alone" "${Q[@]}" model embeddings show
check_exit    "model embeddings set-default (chat model) → 1" 1 "${Q[@]}" model embeddings set-default qwen3-8b
check_content "model embeddings set-default" "multilingual-e5-small" "${Q[@]}" model embeddings set-default multilingual-e5-small
check_json    "model embeddings show --json"       "${Q[@]}" model embeddings show --json
# The registry of sources: a folder is added, listed and removed by its
# identifier; a mailbox without a reference is refused with 1.
check_exit    "source create mail without reference → 1" 1 "${Q[@]}" source create --kind mail --label Inbox
SRCID=$("${Q[@]}" -q source create --kind folder --label E2E-Contracts --reference conn-e2e 2>/dev/null)
check_content "source list shows the folder" "E2E-Contracts" "${Q[@]}" source list
check_json    "source list --json"                 "${Q[@]}" source list --json
check_exit    "source delete the folder → 0" 0     "${Q[@]}" source delete "$SRCID" --confirm
check_exit    "source delete again → 1"      1     "${Q[@]}" source delete "$SRCID" --confirm
check         "digest --since 24h"                 "${Q[@]}" digest --since 24h
check_json    "digest --json"                      "${Q[@]}" digest --since 24h --json

# ── mcp (seeded live servers) ───────────────────────────────────────────
section "mcp (runtime)"
# The seed writes exactly two MCP servers (seed/fragments/mcp.sql): `filesystem`
# and `notes`, both backed by the bundled stdio stub.
check         "mcp show filesystem"                "${Q[@]}" mcp show filesystem
check_content "mcp show reports connected" "connected|healthy|yes" "${Q[@]}" mcp show filesystem
check         "mcp test filesystem"                "${Q[@]}" mcp test filesystem
check         "mcp raw-config filesystem"          "${Q[@]}" mcp raw-config filesystem
check         "mcp update filesystem"              "${Q[@]}" mcp update filesystem --require-approval true
check_content "mcp update persisted requires_approval" "requires_approval.*true|true" "${Q[@]}" mcp raw-config filesystem
check         "mcp restart filesystem"             "${Q[@]}" mcp restart filesystem
check         "mcp remove notes --confirm"         "${Q[@]}" mcp remove notes --confirm
check_exit    "mcp show removed server → 1"  1     "${Q[@]}" mcp show notes
# `mcp add` on a command whose binary does not exist refuses immediately with a
# spawn error (ENOENT), well before the 30s init timeout a bad-but-spawnable
# command would hit; nothing is persisted on that path.
check_exit    "mcp add (unspawnable command) → 1"  1 "${Q[@]}" mcp add e2e-ghost --command /nonexistent-mcp-server-binary
# The seeded apollia.toml leaves `mcp.server_enabled` off, so the daemon
# refuses the MCP bridge with a reason that names the setting.
check_exit    "mcp server (bridge off) → 2"  2    bash -c "'${Q[0]}' ${Q[*]:1} mcp server </dev/null"
skip          "mcp add (live server)" "needs a single-binary MCP server; a bad-but-spawnable command blocks 30s on init timeout (seeded servers cover show/test/restart/update/remove)"

# ── tools / audit ───────────────────────────────────────────────────────────
section "tools / audit"
check         "tools list (daemon on)"             "${Q[@]}" tools list
check_json    "tools list --json"                  "${Q[@]}" tools list --json
check         "tools show bash_executor"           "${Q[@]}" tools show bash_executor
check_exit    "tools show inexistant → 1"  1       "${Q[@]}" tools show inexistant_tool
check         "tools config get bash_executor"     "${Q[@]}" tools config get bash_executor
check_exit    "tools config set (unknown key) → 1" 1 "${Q[@]}" tools config set bash_executor.timeout_secs 30
check         "tools disable web_read + re-enable"  bash -c "'$BIN' --socket '$SOCK' tools disable web_read && '$BIN' --socket '$SOCK' tools enable web_read"
# The seed ships a deliberately-fake (non-decryptable) tool credential blob to
# exercise the desktop "test" button; it makes `tools reload` fail to decrypt.
# Drop it from THIS run's throwaway seed HOME (never the shared fixture) so the
# command is exercised for real.
e2e_sqlite "$HOME/.apollia/governance.db" "DELETE FROM tool_credentials;" >/dev/null 2>&1 || true
check         "tools reload"                        "${Q[@]}" tools reload
check         "tools enable bash_executor"          "${Q[@]}" tools enable bash_executor
check         "tools approvals pending"             "${Q[@]}" tools approvals pending
check         "audit verify (empty journal ok)"    "${Q[@]}" audit verify
check         "audit journal (empty journal ok)"   "${Q[@]}" audit journal
check_exit    "audit anchor (empty journal) → 1" 1 "${Q[@]}" audit anchor
check_exit    "audit replay (unknown run) → 1"   1 "${Q[@]}" audit replay ghost-run-id
check         "audit export --format json"          "${Q[@]}" audit export --format json --output "$RUN_TMP/journal.json"
check_content "audit export --format json: a summary, the detail left out" '"detail": false' cat "$RUN_TMP/journal.json"
check         "audit export --format csv --detail"  "${Q[@]}" audit export --format csv --detail --output "$RUN_TMP/journal.csv"
check_content "audit export --format csv --detail: the summary columns, then the detail" '^ts,agent,nature,family,what,tool,target,outcome,run_id,seq,kind,hash,payload,arguments,output,status' cat "$RUN_TMP/journal.csv"
check_json_field "audit export is itself journaled, with its format and the detail" "[(e['payload']['format'], e['payload']['detail']) for e in d['entries'] if e['kind'] == 'journal_exported']" "[('csv', True), ('json', False)]" "${Q[@]}" audit journal --limit 50 --json
check_exit    "audit export (unknown nature) → 1" 1 "${Q[@]}" audit export --format csv --nature everything --output "$RUN_TMP/journal-refused.csv"

# ── triggers CRUD (target: seeded apollia-chat) ─────────────────────────────
section "triggers CRUD"
check         "trigger create cron"                "${Q[@]}" trigger create t-e2e --agent apollia-chat --kind cron --detail "@daily"
check         "trigger status t-e2e"               "${Q[@]}" trigger status t-e2e
check         "trigger disable t-e2e"              "${Q[@]}" trigger disable t-e2e
check         "trigger enable t-e2e"               "${Q[@]}" trigger enable t-e2e
check         "trigger update t-e2e"               "${Q[@]}" trigger update t-e2e --detail "@hourly"
check         "trigger logs t-e2e --last 5"        "${Q[@]}" trigger logs t-e2e --last 5
check         "trigger reload"                      "${Q[@]}" trigger reload
check         "trigger create interval"            "${Q[@]}" trigger create t-int --agent apollia-chat --kind interval --detail "30m"
check         "trigger create filewatch"           "${Q[@]}" trigger create t-fw --agent apollia-chat --kind filewatch --detail "$RUN_TMP"
check         "trigger create webhook"             "${Q[@]}" trigger create t-wh --agent apollia-chat --kind webhook --detail "this-is-a-32-char-or-more-secret-aa"
check         "trigger delete t-e2e"               "${Q[@]}" trigger delete t-e2e --confirm
check         "trigger delete t-int"               "${Q[@]}" trigger delete t-int --confirm
check         "trigger delete t-fw"                "${Q[@]}" trigger delete t-fw --confirm
check         "trigger delete t-wh"                "${Q[@]}" trigger delete t-wh --confirm

# ── notify CRUD ─────────────────────────────────────────────────────────────
section "notify CRUD"
check         "notify events get"                  "${Q[@]}" notify events get
check         "notify events set"                  "${Q[@]}" notify events set task.completed task.failed
check         "notify create webhook"              "${Q[@]}" notify create --kind webhook --id e2e-hook --label "E2E" --url "https://example.invalid/notify"
check         "notify create desktop"              "${Q[@]}" notify create --kind desktop --id e2e-desk
check         "notify update url"                  "${Q[@]}" notify update e2e-hook --url "https://other.invalid/notify"
check         "notify update --enabled false"      "${Q[@]}" notify update e2e-hook --enabled false
check         "notify logs --last 5"               "${Q[@]}" notify logs --last 5
check         "notify delete e2e-hook"             "${Q[@]}" notify delete e2e-hook --confirm
check         "notify delete e2e-desk"             "${Q[@]}" notify delete e2e-desk --confirm
# `notify test` dispatches to every ACTIVE channel and exits 0 when none is
# active. The seeded webhook channel is born disabled; the desktop channel is
# disabled for the call so nothing real is dispatched, then restored.
check         "notify test setup: disable seeded desktop channel" "${Q[@]}" notify update seed-channel-desktop --enabled false
check         "notify test (no active channel, dispatches nothing)" "${Q[@]}" notify test
check         "notify test teardown: re-enable seeded desktop channel" "${Q[@]}" notify update seed-channel-desktop --enabled true

# ── llm backends CRUD (metadata; no model needed) ───────────────────────────
section "llm backends CRUD"
check         "llm backends list"                  "${Q[@]}" llm backends list
check         "llm backends show local-llama-server" "${Q[@]}" llm backends show local-llama-server
check         "llm backends create e2e2"           "${Q[@]}" llm backends create e2e2 --provider openai --model qwen3-8b --base-url http://127.0.0.1:9/v1 --timeout-sec 60
check         "llm backends update e2e2"           "${Q[@]}" llm backends update e2e2 --timeout-sec 90
check         "llm backends set-default e2e2"      "${Q[@]}" llm backends set-default e2e2
check         "llm backends set-default local-llama-server (restore)" "${Q[@]}" llm backends set-default local-llama-server
check         "llm backends delete e2e2"           "${Q[@]}" llm backends delete e2e2 --confirm
check         "llm status"                          "${Q[@]}" llm status
check_json    "llm status --json"                  "${Q[@]}" llm status --json
check         "llm costs"                           "${Q[@]}" llm costs
check         "llm reload"                          "${Q[@]}" llm reload

# ── stt / resilience / plan cache / chat --list ─────────────────────────
section "stt / resilience / plan cache"
# The seed writes a configured stt_config row (fragments/system.sql:82-93) and a
# ggml-base.bin placeholder no whisper backend could load. This assertion used to
# expect a refusal, on the premise that such a file leaves the daemon without an
# STT engine. It does not: `stt/builder.rs` checks that the model file EXISTS and
# hands the runner sidecar its path, and the sidecar loads on the first
# transcription. The engine starts, the status answers, and the exit is 0.
#
# What the status must not do is claim the model is loaded, which it did until
# `model_loaded` stopped being a literal `true` in the actor's reply. On this
# fixture nothing has been transcribed, so the honest answer is false, and the
# text rendering says `loading` rather than `ready`.
check_json_field "stt status reports the placeholder as not loaded" \
                 "d['model_loaded']" "False" "${Q[@]}" --json stt status
check         "stt model list"                      "${Q[@]}" stt model list
check         "stt config get"                      "${Q[@]}" stt config get
check         "stt config update --language en"    "${Q[@]}" stt config update --language en
# STT history is available without a loaded model, and the seed carries rows.
check_content "stt transcriptions has seeded rows" "seed-transcript" "${Q[@]}" stt transcriptions list
check         "stt transcriptions delete (seeded row)" "${Q[@]}" stt transcriptions delete seed-transcript-charlie --confirm
check_exit    "resilience reset unknown → 1"  1    "${Q[@]}" resilience reset inexistant_tool --confirm
# resilience show/reset only work once a tool is in the breaker registry (lazy).
if "${Q[@]}" resilience show bash_executor >/dev/null 2>&1; then
    check     "resilience show bash_executor"      "${Q[@]}" resilience show bash_executor
    check     "resilience reset bash_executor"     "${Q[@]}" resilience reset bash_executor --confirm
else
    skip      "resilience show/reset bash_executor" "tool not in the breaker registry (no invocations yet)"
fi
check         "plan cache stats (daemon on)"       "${Q[@]}" plan cache stats
check         "plan cache evict --max-age-days 0"  "${Q[@]}" plan cache evict --max-age-days 0 --confirm
check         "chat --list (daemon on)"            "${Q[@]}" chat --list
# seed-session-4 opens on "Which MCP tools do I have on this machine?". Listed
# first, then sent to the trash the way the desktop does (`deleted_at` set):
# `chat --list` no longer names it.
check_content "chat --list names seed-session-4"   "Which MCP tools" "${Q[@]}" chat --list
e2e_sqlite "$HOME/.apollia/chat.db" "UPDATE chat_sessions SET deleted_at = '2026-07-02T00:00:00Z' WHERE id = 'seed-session-4';"
check         "chat --list leaves out a trashed session" bash -c "out=\$(\"\$@\" 2>&1) || { echo \"\$out\"; exit 1; }; [[ \$out != *'Which MCP tools'* ]] || { echo \"\$out\"; exit 1; }" _ "${Q[@]}" chat --list
e2e_sqlite "$HOME/.apollia/chat.db" "UPDATE chat_sessions SET deleted_at = NULL WHERE id = 'seed-session-4';"

# ── task lifecycle (submit to a seeded auto-started agent, no model) ─────────
# apollia-chat auto-starts; a submitted task exercises the read commands even
# while it sits in `working` (no model wired in Track 2), then we cancel it.
section "task lifecycle"
TRUN=$("${Q[@]}" run apollia-chat "ping" --detach --json 2>/dev/null)
TID=$(printf '%s' "$TRUN" | "${E2E_PYTHON:-python3}" -c "import sys,json;d=json.load(sys.stdin);print(d.get('task_id') or d.get('id',''))" 2>/dev/null)
if [[ -n "$TID" ]]; then
    _pass "run --detach + task_id parse" "$TID"
    check   "task status <id>"                     "${Q[@]}" task status "$TID"
    check   "task list"                            "${Q[@]}" task list
    check   "task list --pending-approval"         "${Q[@]}" task list --pending-approval
    check   "task inspect <id>"                     "${Q[@]}" task inspect "$TID"
    check   "task approvals"                        "${Q[@]}" task approvals
    check   "task approvals --pending"             "${Q[@]}" task approvals --pending
    check   "trace <id> --format human"            "${Q[@]}" trace "$TID" --format human
    check_json "trace <id> --format json"          "${Q[@]}" trace "$TID" --format json
    check   "task cancel <id>"                     "${Q[@]}" task cancel "$TID" --confirm
else
    skip    "task lifecycle" "run --detach did not return a task_id (out: ${TRUN:0:120})"
fi
# ── an agent without code (no model wired) ──────────────────────────────────
# The seed carries no agent without code, and its row counts are asserted
# elsewhere, so one is written for this section and removed after it. With no
# model wired the run cannot answer; what is asserted is the path: the task is
# accepted by `run`, followed by the router, and recorded under that agent.
section "agent without code"
LEDGER_DEF='{"id":"seed-ledger","name":"Ledger clerk","mission":"Reconcile the ledger."}'
if e2e_sqlite "$HOME/.apollia/agents.db" "INSERT INTO agents (name, origin, version, definition_json, enabled, installed_at, updated_at) VALUES ('seed-ledger', 'interface', '1.0.0', '$LEDGER_DEF', 1, '2026-07-16T09:00:00Z', '2026-07-16T09:00:00Z');" >/dev/null 2>&1; then
    DRUN=$("${Q[@]}" run seed-ledger "Reconcile March." --detach --json 2>/dev/null)
    DTID=$(printf '%s' "$DRUN" | "${E2E_PYTHON:-python3}" -c "import sys,json;d=json.load(sys.stdin);print(d.get('task_id') or d.get('id',''))" 2>/dev/null)
    if [[ -n "$DTID" ]]; then
        _pass "run agent without code --detach" "$DTID"
        check         "task status <id> (agent without code)" "${Q[@]}" task status "$DTID"
        check_content "task list --json names the agent without code" "seed-ledger" "${Q[@]}" task list --json
        DROW=$(e2e_sqlite "$HOME/.apollia/hitl.db" "SELECT agent_name || '|' || (input_json IS NOT NULL) FROM tasks WHERE task_id = '$DTID';" 2>/dev/null)
        [[ "$DROW" == "seed-ledger|1" ]] \
            && _pass "hitl.db records the agent and the whole input" "$DROW" \
            || _fail "hitl.db records the agent and the whole input" "row: ${DROW:-none}"
    else
        _fail "run agent without code --detach" "no task_id (out: ${DRUN:0:200})"
    fi
    e2e_sqlite "$HOME/.apollia/agents.db" "DELETE FROM agents WHERE name = 'seed-ledger';" >/dev/null 2>&1 || true
else
    skip "agent without code" "agents.db has no agents table to write the agent into"
fi

# ── decisions waiting for the user ──────────────────────────────────────────
# Nothing in the seed waits for a decision: no paused task, no batch, no
# conversation running. The list is empty and the command succeeds.
section "approvals"
check_json_field "approvals pending --json holds no item" "len(d['items'])" 0 "${Q[@]}" approvals pending --json
check         "approvals pending (empty)"                    "${Q[@]}" approvals pending
# No workflow of the seed waits on a question: answering one that does not
# wait is refused by the runtime, in human and in JSON form.
check_exit    "approvals decide unknown question → 1"        1 "${Q[@]}" approvals decide workflow-question:00000000-0000-0000-0000-000000000000 answer --value 12
check_exit    "approvals decide unknown question --json → 1" 1 "${Q[@]}" approvals decide workflow-question:00000000-0000-0000-0000-000000000000 refuse --reason later --json

# ── batches ─────────────────────────────────────────────────────────────────
# The batch engine runs in the daemon. The seed holds no batch. It reads
# workbooks (`spreadsheet`) and CSV files (`csv`): a launch on another source
# type, or on a file that is not there, is refused with 1 before anything is
# recorded, and the list stays empty.
section "batches"
check_json_field "batch list --json holds no batch" "len(d['batches'])" 0 "${Q[@]}" batch list --json
check_exit    "batch run unknown source type → 1" 1  "${Q[@]}" batch run apollia-chat --source-type mailbox --source /data/inbox
check_exit    "batch run missing workbook → 1"    1  "${Q[@]}" batch run apollia-chat --source-type spreadsheet --source /data/requests.xlsx
check_exit    "batch status unknown → 1"          1  "${Q[@]}" batch status 00000000-0000-0000-0000-000000000000
check_exit    "batch stop unknown → 1"            1  "${Q[@]}" batch stop 00000000-0000-0000-0000-000000000000
check_exit    "batch retry unknown → 1"           1  "${Q[@]}" batch retry 00000000-0000-0000-0000-000000000000
check_exit    "batch export unknown → 1"          1  "${Q[@]}" batch export 00000000-0000-0000-0000-000000000000 --format json -o -

# ── workflows ───────────────────────────────────────────────────────────────
# The workflow engine runs in the daemon. The seed installs no package with a
# workflow: a launch is refused with 1 before anything is recorded, and the
# history of any workflow is empty.
section "workflows"
check_json_field "workflow logs --json holds no run" "len(d['runs'])" 0 "${Q[@]}" workflow logs invoices/close-month --json
check_exit    "workflow show unknown → 1"         1  "${Q[@]}" workflow show invoices/close-month
check_exit    "workflow run unknown → 1"          1  "${Q[@]}" workflow run invoices/close-month --input folder=/data/march
check_exit    "workflow status unknown → 1"       1  "${Q[@]}" workflow status 00000000-0000-0000-0000-000000000000
check_exit    "workflow cancel unknown → 1"       1  "${Q[@]}" workflow cancel 00000000-0000-0000-0000-000000000000 --confirm

# `task resume` resolves the task id before touching its state, so an unknown
# id is a deterministic refusal; the approve path on a real suspension stays a
# variant below.
check_exit    "task resume (unknown task) → 1"  1  "${Q[@]}" task resume ghost-task-id --approve
skip          "task resume (suspended task)" "needs a task suspended in input_required; the model-free stub never suspends"

# ── agent lifecycle (a seeded agent: data, no Python behind it) ──────────────
section "agent lifecycle (seeded agent)"
# The seeded agents are available as soon as the daemon is up: nobody starts
# them. The lifecycle verbs are played on onboarding-agent and leave it as
# they found it, enabled and available.
check_content "agent list shows onboarding-agent"  "onboarding-agent" "${Q[@]}" agent list
check         "agent status onboarding-agent"      "${Q[@]}" agent status onboarding-agent
check         "agent messages --limit 5"           "${Q[@]}" agent messages onboarding-agent --limit 5
check         "agent logs --last 5"                "${Q[@]}" agent logs onboarding-agent --last 5
check         "trigger fire (seeded agent)"        bash -c "'$BIN' --socket '$SOCK' trigger create t-fire --agent onboarding-agent --kind cron --detail '@daily' && '$BIN' --socket '$SOCK' trigger fire t-fire; rc=\$?; '$BIN' --socket '$SOCK' trigger delete t-fire --confirm >/dev/null 2>&1; exit \$rc"
check_exit    "agent start (already available) → 1" 1 "${Q[@]}" agent start onboarding-agent
check_exit    "agent start (unknown agent) → 1"    1 "${Q[@]}" agent start e2e-ghost-agent
check_exit    "agent start apollia:chat (built in) → 1" 1 "${Q[@]}" agent start apollia:chat
check         "agent stop onboarding-agent"        "${Q[@]}" agent stop onboarding-agent
check         "agent start onboarding-agent (after a stop)" "${Q[@]}" agent start onboarding-agent
check         "agent stop onboarding-agent (again)" "${Q[@]}" agent stop onboarding-agent
check         "agent disable onboarding-agent"     "${Q[@]}" agent disable onboarding-agent
check_exit    "agent start (disabled agent) → 1"   1 "${Q[@]}" agent start onboarding-agent
check_json_field "agent enable makes it available again" "d['runtime_load']" "loaded" "${Q[@]}" agent enable onboarding-agent --json
# Overrides of a seeded agent: set, read back resolved, then back to defaults.
# Placed after the empty-journal audit checks: every write is journaled.
check_content "agent list --definitions has apollia-chat" "apollia-chat" "${Q[@]}" agent list --definitions
check_content "agent set mission"                  "Answer in one line." "${Q[@]}" agent set apollia-chat mission "Answer in one line."
check_content "agent show carries the override"    "Answer in one line." "${Q[@]}" agent show apollia-chat
check_exit    "agent reset without --confirm → 1"  1 "${Q[@]}" agent reset apollia-chat
check         "agent reset --field mission"        "${Q[@]}" agent reset apollia-chat --field mission --confirm
check_content "agent set sources profile"          "profile" "${Q[@]}" agent set apollia-chat sources profile
check_content "agent show lists the profile source" "profile" "${Q[@]}" agent show apollia-chat
check         "agent reset --field sources"        "${Q[@]}" agent reset apollia-chat --field sources --confirm
check         "agent reset --confirm"              "${Q[@]}" agent reset apollia-chat --confirm
check_exit    "agent set unknown agent → 1"        1 "${Q[@]}" agent set e2e-ghost-agent mission "x"
# The settings of the chat are the overrides and the model of the Apollia
# agent, written through the same routes and journaled the same way.
check         "chat config set system-prompt"      "${Q[@]}" chat config set system-prompt "Answer in one line."
check         "chat config set allowed-tools"      "${Q[@]}" chat config set allowed-tools "web_search,file_read"
check         "chat config set llm-backend"        "${Q[@]}" chat config set llm-backend local-llama-server
check_content "chat config get carries the prompt" "Answer in one line." "${Q[@]}" chat config get
check_content "chat config get --json carries the model" "local-llama-server" "${Q[@]}" chat config get --json
check_content "agent list --definitions has apollia:chat" "apollia:chat" "${Q[@]}" agent list --definitions
check_exit    "chat config reset without --confirm → 1" 1 "${Q[@]}" chat config reset
check         "chat config reset --confirm"        "${Q[@]}" chat config reset --confirm
check_content "chat config get after reset"        "(runtime default)" "${Q[@]}" chat config get
check_exit    "agent uninstall apollia:chat (built in) → 1" 1 "${Q[@]}" agent uninstall apollia:chat --confirm
check         "audit verify (overrides journaled)"  "${Q[@]}" audit verify
# The overrides above are access changes: each entry of the page carries its
# summary, and the filters keep that nature and nothing else.
check_json_field "audit journal --nature access_change --json: summarised, that nature only" "d['count'] > 0 and all(e['summary']['nature'] == 'access_change' for e in d['entries'])" "True" "${Q[@]}" audit journal --nature access_change --limit 50 --json
check_json_field "audit journal --nature policy --result refused --json: nothing matches" "d['count']" "0" "${Q[@]}" audit journal --nature policy --result refused --json
check_content "audit journal --nature access_change: a line per entry, in words" "Agent setting (changed|reset)" "${Q[@]}" audit journal --nature access_change --limit 50
check_exit    "audit journal (unknown nature) → 1" 1 "${Q[@]}" audit journal --nature everything
check_exit    "audit journal (unreadable date) → 1" 1 "${Q[@]}" audit journal --since last-week
check         "agent package list"                 "${Q[@]}" agent package list
check_content "agent package list has seed-office-pack" "seed-office-pack" "${Q[@]}" agent package show seed-office-pack
# The leaf resolves its target before acting, so an unknown name is a
# deterministic refusal; the success path is played further down, on a
# package this track installs.
check_exit    "agent package uninstall (unknown pack) → 1" 1 "${Q[@]}" agent package uninstall e2e-ghost-pack --confirm

# ── eval (report offline; run needs a model → Track 3) ──────────────────────
section "eval"
check         "eval report (empty jsonl)"          bash -c "printf '' > '$RUN_TMP/eval.jsonl'; '$BIN' --socket '$SOCK' eval report '$RUN_TMP/eval.jsonl'"
# `eval run` reads the suite file before involving any agent or model, so a
# missing suite is a deterministic refusal.
check_exit    "eval run (missing suite) → 1"  1    "${Q[@]}" eval run "$RUN_TMP/never-written-suite.yaml"
skip          "eval run <suite> (real model)" "runs the agent against a real model; covered as a capture in Track 3 when a model is wired"

# ── .apo packages (preview, install, update, keyring) ───────────────────────
# A tool-only package is built here, signed by a key generated for the run, and
# driven through the runtime. The seed's row counts are asserted above; the
# package this section installs is removed at its end.
section "packages"
RPKG="$RUN_TMP/runtime-pkg"; /bin/mkdir -p "$RPKG/tools"
write_runtime_pkg() {
    cat > "$RPKG/apollia.toml" <<TOML
[package]
name = "e2e-runtime-pkg"
version = "$1"
publisher = "E2E Publisher"
sdk = ">=0.3"

[[tools]]
name = "ping"
module = "tools/ping.py"
description = "Answer pong."
action = "read"
inputs = { type = "object" }
TOML
    printf "@tool(name='ping')\nasync def ping(ctx, inputs):\n    return 'pong'\n" > "$RPKG/tools/ping.py"
}
write_runtime_pkg "1.1.0"
"$BIN" package build "$RPKG" --sign --key e2e-runtime -o "$RUN_TMP/rpkg-1.1.0.apo" >/dev/null 2>&1
write_runtime_pkg "1.0.0"
"$BIN" package build "$RPKG" -o "$RUN_TMP/rpkg-1.0.0.apo" >/dev/null 2>&1
check_json_field "package preview: name"            "d['name']"                  "e2e-runtime-pkg" "${Q[@]}" package preview "$RUN_TMP/rpkg-1.1.0.apo" --json
check_json_field "package preview: signed, untrusted" "d['signature']['status']+'/'+str(d['signature']['trusted'])" "signed/False" "${Q[@]}" package preview "$RUN_TMP/rpkg-1.1.0.apo" --json
check_json_field "package preview: the tool"        "d['tools'][0]['name']"      "ping"            "${Q[@]}" package preview "$RUN_TMP/rpkg-1.1.0.apo" --json
check_content    "package preview (human) names the publisher" "E2E Publisher" "${Q[@]}" package preview "$RUN_TMP/rpkg-1.1.0.apo"
check_json_field "package install: version"         "d['version']"               "1.1.0"           "${Q[@]}" package install "$RUN_TMP/rpkg-1.1.0.apo" --json
check_json_field "package preview: installed version" "d['installedVersion']"    "1.1.0"           "${Q[@]}" package preview "$RUN_TMP/rpkg-1.0.0.apo" --json
check_exit       "package install (older version) → 1" 1 "${Q[@]}" package install "$RUN_TMP/rpkg-1.0.0.apo"
check_json_field "package install --allow-downgrade" "d['previousVersion']"      "1.1.0"           "${Q[@]}" package install "$RUN_TMP/rpkg-1.0.0.apo" --allow-downgrade --json
# The author key generated by the signed build above is the one trusted here.
RFP=$("${Q[@]}" package inspect "$RUN_TMP/rpkg-1.1.0.apo" --json 2>/dev/null | "${E2E_PYTHON:-python3}" -c "import sys,json;print(json.load(sys.stdin)['signature']['key_fingerprint'])" 2>/dev/null)
RPUB=$("${E2E_PYTHON:-python3}" - "$RUN_TMP/rpkg-1.1.0.apo" <<'PY' 2>/dev/null
import sys, zipfile
print(zipfile.ZipFile(sys.argv[1]).read("SIGNATURE.key").decode().strip())
PY
)
check_json_field "package trust import"             "d['fingerprint']"           "$RFP"            "${Q[@]}" package trust import "$RPUB" --label "E2E Corp" --json
check_content    "package trust list names the label" "E2E Corp"                 "${Q[@]}" package trust list
check_json_field "package preview: trusted publisher" "d['signature']['publisher']" "E2E Corp"      "${Q[@]}" package preview "$RUN_TMP/rpkg-1.1.0.apo" --json
check            "package trust remove --confirm"   "${Q[@]}" package trust remove "$RFP" --confirm
check_exit       "package trust remove (unknown) → 1" 1 "${Q[@]}" package trust remove "$RFP" --confirm
check_exit       "package trust import (not a key) → 1" 1 "${Q[@]}" package trust import "not-a-key" --label "X"
check            "audit verify (installation journaled)" "${Q[@]}" audit verify
# The uninstall goes through the runtime too: the package and its history go,
# the journal records it, and a second uninstall finds nothing.
check_json_field "agent package uninstall: history erased" "d['name']+'/'+str(d['historyCleared'])" "e2e-runtime-pkg/True" "${Q[@]}" agent package uninstall e2e-runtime-pkg --clear-history --confirm --json
check_exit       "agent package uninstall (already gone) → 1" 1 "${Q[@]}" agent package uninstall e2e-runtime-pkg --confirm
check            "audit verify (uninstallation journaled)" "${Q[@]}" audit verify
e2e_sqlite "$HOME/.apollia/agents.db" "DELETE FROM package_items WHERE package_name = 'e2e-runtime-pkg'; DELETE FROM installed_packages WHERE name = 'e2e-runtime-pkg';" >/dev/null 2>&1 || true
/bin/rm -rf "$HOME/.apollia/agents/packages/e2e-runtime-pkg"
