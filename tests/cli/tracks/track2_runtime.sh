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
# local-llama-server (the default), openai-gpt4o-mini and anthropic-claude.
# Assert the default and one non-default, so the list is proven to carry more
# than the row Track 3 repoints.
check_content "llm backends lists local-llama-server" "local-llama-server" "${Q[@]}" llm backends list
check_content "llm backends lists anthropic-claude" "anthropic-claude"  "${Q[@]}" llm backends list
check_content "mcp list has filesystem connected" "filesystem"  "${Q[@]}" mcp list
check_content "notify events get has task.completed" "task.completed" "${Q[@]}" notify events get
# Seeded agents auto-start at boot, so seed-classifier exposes its A2A skill.
check_content "a2a skills exposes classify_text" "classify_text" "${Q[@]}" a2a skills
check         "a2a invoke classify_text completes" "${Q[@]}" a2a invoke classify_text --args '{"text":"the app crashes on login","labels":["bug","feature"]}' --timeout 60

# Empty-but-valid runtime reads (seeded elsewhere, not surfaced by these paths).
check         "task list (empty ok)"               "${Q[@]}" task list
check         "audit list (empty ok)"              "${Q[@]}" audit list --limit 5
check_json    "audit stats --json"                 "${Q[@]}" audit stats --json
check_exit    "audit show (unknown id) → 1"  1     "${Q[@]}" audit show nonexistent-run
check         "resilience list"                    "${Q[@]}" resilience list
check         "model hardware (daemon on)"         "${Q[@]}" model hardware
check_json    "model hardware --json"              "${Q[@]}" model hardware --json
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
check         "tools approvals resolved"           "${Q[@]}" tools approvals resolved
# The seed ships a deliberately-fake (non-decryptable) tool credential blob to
# exercise the desktop "test" button; it makes `tools reload` fail to decrypt.
# Drop it from THIS run's throwaway seed HOME (never the shared fixture) so the
# command is exercised for real.
/usr/bin/sqlite3 "$HOME/.apollia/governance.db" "DELETE FROM tool_credentials;" >/dev/null 2>&1 || true
check         "tools reload"                        "${Q[@]}" tools reload
check         "tools enable bash_executor"          "${Q[@]}" tools enable bash_executor
check         "tools approvals pending"             "${Q[@]}" tools approvals pending
check         "audit verify (empty journal ok)"    "${Q[@]}" audit verify
check         "audit journal (empty journal ok)"   "${Q[@]}" audit journal
check_exit    "audit anchor (empty journal) → 1" 1 "${Q[@]}" audit anchor
check_exit    "audit replay (unknown run) → 1"   1 "${Q[@]}" audit replay ghost-run-id
check         "audit export"                        "${Q[@]}" audit export --output "$RUN_TMP/audit.json" --limit 100

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
check         "llm backends create e2e2"           "${Q[@]}" llm backends create e2e2 --provider openai --model gpt-4o-mini --timeout-sec 60
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
# The seed writes a configured stt_config row (fragments/system.sql:82-93),
# but its ggml-base.bin is a placeholder no whisper backend can load, so the
# daemon holds no STT engine and `GET /stt/status` answers 503 by contract
# (routes_stt.rs, stt_unavailable). The deterministic path on this fixture is
# therefore the refusal; Track 1 already asserts the daemon-off exit 2.
check_exit    "stt status (placeholder model, engine unavailable) → 1" 1 "${Q[@]}" stt status
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

# ── task lifecycle (submit to a seeded auto-started agent, no model) ─────────
# apollia-chat auto-starts; a submitted task exercises the read commands even
# while it sits in `working` (no model wired in Track 2), then we cancel it.
section "task lifecycle"
TRUN=$("${Q[@]}" run apollia-chat "ping" --detach --json 2>/dev/null)
TID=$(printf '%s' "$TRUN" | /usr/bin/python3 -c "import sys,json;d=json.load(sys.stdin);print(d.get('task_id') or d.get('id',''))" 2>/dev/null)
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
# `task resume` resolves the task id before touching its state, so an unknown
# id is a deterministic refusal; the approve path on a real suspension stays a
# variant below.
check_exit    "task resume (unknown task) → 1"  1  "${Q[@]}" task resume ghost-task-id --approve
skip          "task resume (suspended task)" "needs a task suspended in input_required; the model-free stub never suspends"

# ── agent lifecycle (stub agent; degrades to skip if runner unavailable) ─────
section "agent lifecycle (stub)"
HELLO_PY="$RUN_TMP/hello.py"
/bin/cat >"$HELLO_PY" <<'PYEOF'
"""E2E stub agent for cli-e2e.sh."""
from apollia import agent, on_message
from apollia.types import Ctx, Message

@agent(name="e2e-hello", version="0.0.1", description="E2E stub")
class E2EHello:
    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        return f"echo:{message}"

agent_instance = E2EHello()
PYEOF
check         "agent validate <stub>"              "$BIN" agent validate "$HELLO_PY"
check         "agent install <stub> --skip-tests"  "${Q[@]}" agent install "$HELLO_PY" --skip-tests
check_content "agent list shows e2e-hello" "e2e-hello" "${Q[@]}" agent list
if "${Q[@]}" agent start e2e-hello >/dev/null 2>&1; then
    _pass "agent start (post-install)"
    check    "agent status e2e-hello"              "${Q[@]}" agent status e2e-hello
    check    "agent messages --limit 5"            "${Q[@]}" agent messages e2e-hello --limit 5
    check    "agent logs --last 5"                 "${Q[@]}" agent logs e2e-hello --last 5
    check    "trigger fire (started stub)"         bash -c "'$BIN' --socket '$SOCK' trigger create t-fire --agent e2e-hello --kind cron --detail '@daily' && '$BIN' --socket '$SOCK' trigger fire t-fire; rc=\$?; '$BIN' --socket '$SOCK' trigger delete t-fire --confirm >/dev/null 2>&1; exit \$rc"
    AGENT_RUNNING=1
else
    skip     "agent status/messages/logs/fire" "Python runner could not spawn the SDK loader in the tmp \$HOME"
    AGENT_RUNNING=0
fi
check         "agent update e2e-hello <path>"      "${Q[@]}" agent update e2e-hello "$HELLO_PY"
check         "agent stop e2e-hello"               "${Q[@]}" agent stop e2e-hello
check         "agent disable e2e-hello"            "${Q[@]}" agent disable e2e-hello
check         "agent enable e2e-hello"             "${Q[@]}" agent enable e2e-hello
check         "agent package list"                 "${Q[@]}" agent package list
check_content "agent package list has seed-office-pack" "seed-office-pack" "${Q[@]}" agent package show seed-office-pack
check         "agent create scaffold --type react" "$BIN" agent create e2e-scaffold --type react
check         "agent uninstall e2e-hello"          "${Q[@]}" agent uninstall e2e-hello --confirm
# Both leaves resolve their target before acting, so an unknown name is a
# deterministic refusal; the success paths need an installed bundle and stay
# variants below.
check_exit    "agent repair (unknown agent) → 1"           1 "${Q[@]}" agent repair e2e-ghost-agent
check_exit    "agent package uninstall (unknown pack) → 1" 1 "${Q[@]}" agent package uninstall e2e-ghost-pack --confirm
skip          "agent repair / agent package uninstall (installed bundle)" "repair re-provisions a packaged agent's venv; uninstall needs the bundle (seed-office-pack is show-only here)"

# ── eval (report offline; run needs a model → Track 3) ──────────────────────
section "eval"
check         "eval report (empty jsonl)"          bash -c "printf '' > '$RUN_TMP/eval.jsonl'; '$BIN' --socket '$SOCK' eval report '$RUN_TMP/eval.jsonl'"
# `eval run` reads the suite file before involving any agent or model, so a
# missing suite is a deterministic refusal.
check_exit    "eval run (missing suite) → 1"  1    "${Q[@]}" eval run "$RUN_TMP/never-written-suite.yaml"
skip          "eval run <suite> (real model)" "runs the agent against a real model; covered as a capture in Track 3 when a model is wired"
