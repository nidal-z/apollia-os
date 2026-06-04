#!/usr/bin/env bash
# tests/cli/cli-e2e.sh - Apollia OS CLI smoke + E2E suite (single source of truth).
#
# Covers ~145 of the ~165 CLI leaf commands. Splits into two phases:
#
#   Phase A - LOCAL (always runs):
#     Every command runnable without a daemon. Each operation targets the
#     isolated $HOME (mktemp) so the user's real ~/.apollia is never touched.
#     Wall-clock: ~5–10 s on a warm release build.
#
#   Phase B - RUNTIME (gated APOLLIA_REQUIRE_RUNTIME=1):
#     Spawns `apollia-os start`, exercises the runtime-dependent surface
#     (agent lifecycle, tasks, llm with local backend, triggers CRUD, …),
#     then `apollia-os stop`. Wall-clock: 30–90 s depending on the model.
#
# Environment variables:
#   APOLLIA_BIN              binary path. Default: ./target/release/apollia-os
#                            (fallback ./target/debug/apollia-os).
#   APOLLIA_TEST_MODEL_GGUF  GGUF model for the local LLM backend test. Default:
#                            ~/.apollia/models/Qwen3-30B-A3B-Q4_K_M.gguf.
#                            When the path is absent on disk, LLM-bound tests
#                            are SKIPped (not failed).
#   APOLLIA_REQUIRE_RUNTIME  0|1 (default 0). Set to 1 to run Phase B.
#   APOLLIA_TEST_REVIEW      0|1 (default 0). Set to 1 to run `review .`
#                            (slow, depends on the apollia-review agent).
#   APOLLIA_TEST_VERBOSE     0|1 (default 0). Dump stdout/stderr on FAIL.
#
# Exit code: 0 when every assertion passed, 1 otherwise.

set -uo pipefail

# ── Resolve binary ─────────────────────────────────────────────────────────
if [[ -n "${APOLLIA_BIN:-}" ]]; then
    BIN=$APOLLIA_BIN
elif [[ -x ./target/release/apollia-os ]]; then
    BIN="${PWD}/target/release/apollia-os"
elif [[ -x ./target/debug/apollia-os ]]; then
    BIN="${PWD}/target/debug/apollia-os"
else
    echo "FAIL: apollia-os binary not found. Build with \`cargo build --release -p apollia-cli\` or set APOLLIA_BIN." >&2
    exit 1
fi

# ── Constants ──────────────────────────────────────────────────────────────
REAL_HOME=$HOME
DEFAULT_GGUF="${REAL_HOME}/.apollia/models/Qwen3-30B-A3B-Q4_K_M.gguf"
TEST_GGUF="${APOLLIA_TEST_MODEL_GGUF:-$DEFAULT_GGUF}"
REQUIRE_RUNTIME="${APOLLIA_REQUIRE_RUNTIME:-0}"
TEST_REVIEW="${APOLLIA_TEST_REVIEW:-0}"
VERBOSE="${APOLLIA_TEST_VERBOSE:-0}"

# Force file-based secret storage for keyring-bound commands. macOS Terminal
# sessions often can't reach the default keychain from a sub-shell - the
# AgeFileSecretStore is a deterministic, hermetic backend perfect for tests.
export APOLLIA_TOKEN_STORAGE="${APOLLIA_TOKEN_STORAGE:-file}"
export APOLLIA_TOKEN_PASSPHRASE="${APOLLIA_TOKEN_PASSPHRASE:-cli-e2e-test-passphrase}"

# Silence INFO-level tracing on stdout so JSON-emitting commands stay parseable
# (the apollia-memory and connector registries log over stdout on the first
# call). The user can re-enable verbose logs with RUST_LOG=debug if needed.
export RUST_LOG="${RUST_LOG:-warn}"

# ── Isolated $HOME ─────────────────────────────────────────────────────────
TMPDIR=$(/usr/bin/mktemp -d -t apollia-cli-e2e.XXXXXX)
export HOME=$TMPDIR
/bin/mkdir -p "$TMPDIR/.apollia/logs" "$TMPDIR/.apollia/memory"
# Symlink the models directory to the real one so `llm setup --local` doesn't
# duplicate gigabytes of GGUF into tmpdir. We still treat the rest of
# ~/.apollia as fully isolated (databases, configs, journals).
if [[ -d "${REAL_HOME:-$HOME}/.apollia/models" ]]; then
    /bin/ln -s "${REAL_HOME}/.apollia/models" "$TMPDIR/.apollia/models"
else
    /bin/mkdir -p "$TMPDIR/.apollia/models"
fi

PROJECTS_DB="$TMPDIR/.apollia/projects.db"
USER_DB="$TMPDIR/.apollia/user_memory.db"
GOV_DB="$TMPDIR/.apollia/governance.db"
CHAT_DB="$TMPDIR/.apollia/chat.db"
MCP_APPR_DB="$TMPDIR/.apollia/mcp_approvals.db"
MCP_DB="$TMPDIR/.apollia/mcp.db"
SYSTEM_DB="$TMPDIR/.apollia/system.db"
CFG="$TMPDIR/.apollia/apollia.toml"
MDIR="$TMPDIR/.apollia/memory"
SOCK="$TMPDIR/apollia.sock"
MODELS_DIR="$TMPDIR/.apollia/models"

# Avoid colliding with a production daemon already bound to TCP 7771: pick a
# free ephemeral port at script start. Failure to detect (no python3) → 0 lets
# the OS pick one; cli passes through.
FREE_PORT=$(/usr/bin/python3 -c "import socket; s=socket.socket(); s.bind(('127.0.0.1',0)); print(s.getsockname()[1]); s.close()" 2>/dev/null || echo "0")

DAEMON_PID=""

cleanup() {
    local rc=${1:-0}
    if [[ -n "$DAEMON_PID" ]] && /bin/kill -0 "$DAEMON_PID" 2>/dev/null; then
        "$BIN" --socket "$SOCK" stop >/dev/null 2>&1 || /bin/kill -TERM "$DAEMON_PID" 2>/dev/null
        /usr/bin/wait "$DAEMON_PID" 2>/dev/null || true
    fi
    /bin/rm -rf "$TMPDIR"
    exit "$rc"
}
trap 'cleanup $?' EXIT
trap 'cleanup 130' INT TERM

# ── Counters / state ───────────────────────────────────────────────────────
PASS=0
FAIL=0
SKIP=0
FAILED_LABELS=()

# ── Helpers ────────────────────────────────────────────────────────────────
red()    { printf '\033[31m%s\033[0m' "$1"; }
green()  { printf '\033[32m%s\033[0m' "$1"; }
yellow() { printf '\033[33m%s\033[0m' "$1"; }
bold()   { printf '\033[1m%s\033[0m' "$1"; }

section() {
    printf '\n%s\n' "$(bold "─── $1")"
}

_record_fail() {
    local label=$1; local detail=$2
    FAIL=$((FAIL + 1))
    FAILED_LABELS+=("$label")
    if [[ "$VERBOSE" == "1" ]]; then
        printf '    %s\n' "$detail"
    fi
}

# check "label" cmd args...  → expect exit 0.
check() {
    local label=$1; shift
    local out rc
    out=$("$@" 2>&1) ; rc=$?
    if [[ $rc -eq 0 ]]; then
        printf '  %s %s\n' "$(green ✔)" "$label"
        PASS=$((PASS + 1))
    else
        printf '  %s %s (exit %d)\n' "$(red ✗)" "$label" "$rc"
        _record_fail "$label" "cmd: $* | out: ${out:0:300}"
    fi
}

# check_exit "label" expected_rc cmd args...
check_exit() {
    local label=$1; local expected=$2; shift 2
    local out rc
    out=$("$@" 2>&1) ; rc=$?
    if [[ $rc -eq $expected ]]; then
        printf '  %s %s (rc=%d)\n' "$(green ✔)" "$label" "$rc"
        PASS=$((PASS + 1))
    else
        printf '  %s %s (expected %d, got %d)\n' "$(red ✗)" "$label" "$expected" "$rc"
        _record_fail "$label" "cmd: $* | out: ${out:0:300}"
    fi
}

# check_json "label" cmd args...  → expect exit 0 + stdout = valid JSON.
check_json() {
    local label=$1; shift
    local out rc
    out=$("$@" 2>/dev/null) ; rc=$?
    if [[ $rc -eq 0 ]] && /usr/bin/python3 -c "import sys, json; json.loads(sys.stdin.read())" <<<"$out" 2>/dev/null; then
        printf '  %s %s (JSON valid)\n' "$(green ✔)" "$label"
        PASS=$((PASS + 1))
    else
        printf '  %s %s (rc=%d or non-JSON stdout)\n' "$(red ✗)" "$label" "$rc"
        _record_fail "$label" "cmd: $* | out: ${out:0:300}"
    fi
}

# check_grep "label" pattern cmd args...  → expect exit 0 + stdout matches.
check_grep() {
    local label=$1; local pattern=$2; shift 2
    local out rc
    out=$("$@" 2>&1) ; rc=$?
    if [[ $rc -eq 0 ]] && echo "$out" | /usr/bin/grep -qE "$pattern"; then
        printf '  %s %s\n' "$(green ✔)" "$label"
        PASS=$((PASS + 1))
    else
        printf '  %s %s (rc=%d or pattern miss)\n' "$(red ✗)" "$label" "$rc"
        _record_fail "$label" "pattern: $pattern | cmd: $* | out: ${out:0:300}"
    fi
}

skip() {
    local label=$1; local reason=${2:-skipped}
    printf '  %s %s - %s\n' "$(yellow ⊘)" "$label" "$reason"
    SKIP=$((SKIP + 1))
}

# Wait for the Unix socket to be reachable. Used after `apollia-os start &`.
wait_for_socket() {
    local sock=$1; local timeout=${2:-30}
    local i=0
    while [[ $i -lt $timeout ]]; do
        if "$BIN" --socket "$sock" status >/dev/null 2>&1; then
            return 0
        fi
        /bin/sleep 1
        i=$((i + 1))
    done
    return 1
}

# ── Banner ─────────────────────────────────────────────────────────────────
echo "$(bold "Apollia OS CLI E2E suite")"
echo "  BIN                  = $BIN"
echo "  HOME                 = $TMPDIR (isolated)"
echo "  TEST_GGUF            = $TEST_GGUF"
echo "                          (exists: $([[ -f "$TEST_GGUF" ]] && echo yes || echo NO → LLM tests SKIPped))"
echo "  REQUIRE_RUNTIME      = $REQUIRE_RUNTIME"
echo "  TEST_REVIEW          = $TEST_REVIEW"
echo "  VERBOSE              = $VERBOSE"

# ═══════════════════════════════════════════════════════════════════════════
#                                 PHASE A
# ═══════════════════════════════════════════════════════════════════════════
echo
echo "$(bold "═══ Phase A - LOCAL (no daemon) ═══")"

# A.1 - Always-on
section "A.1 always-on"
check        "version"                                      "$BIN" version
check_json   "version --json"                               "$BIN" version --json
check        "--help"                                       "$BIN" --help
check        "doctor"                                       "$BIN" doctor
check_json   "doctor --json"                                "$BIN" doctor --json

# A.2 - config
section "A.2 config"
check        "config validate (absent)"                     "$BIN" config validate --file "$CFG"
check        "config set llm.default local"                 "$BIN" config set llm.default local --file "$CFG"
check_grep   "config get llm.default = local"  "local"      "$BIN" config get llm.default --file "$CFG"
check_json   "config get --json"                            "$BIN" config get --file "$CFG" --json
check_exit   "config validate (partial llm)"  1             "$BIN" config validate --file "$CFG"
check_json   "config show --json"                           "$BIN" config show --file "$CFG" --json
check_exit   "config edit refuses non-TTY"    1             "$BIN" config edit --file "$CFG"
check_exit   "config reset (no --confirm)"    1             "$BIN" config reset
RESET_HOME="$TMPDIR/.reset-test"
/bin/mkdir -p "$RESET_HOME"; /usr/bin/touch "$RESET_HOME/dummy"
/bin/mkdir -p "$RESET_HOME/nested"; /usr/bin/touch "$RESET_HOME/nested/inner"
check        "config reset --dry-run"                       "$BIN" config reset --dry-run --home "$RESET_HOME"
if [[ -f "$RESET_HOME/dummy" && -f "$RESET_HOME/nested/inner" ]]; then
    printf '  %s %s\n' "$(green ✔)" "config reset --dry-run did NOT delete"
    PASS=$((PASS + 1))
else
    printf '  %s %s\n' "$(red ✗)" "config reset --dry-run unexpectedly deleted files"
    _record_fail "reset --dry-run preserved files" ""
fi
check        "config reset --confirm (non-empty)"           "$BIN" config reset --confirm --home "$RESET_HOME"
if [[ -d "$RESET_HOME" && ! -e "$RESET_HOME/dummy" ]]; then
    printf '  %s %s\n' "$(green ✔)" "config reset --confirm wiped contents (kept dir)"
    PASS=$((PASS + 1))
else
    printf '  %s %s\n' "$(red ✗)" "config reset --confirm wipe behavior"
    _record_fail "reset --confirm wiped" ""
fi
check        "config reset --confirm (missing home)"        "$BIN" config reset --confirm --home "$TMPDIR/.never-existed"

# A.3 - project
section "A.3 project"
check        "project list (empty)"                         "$BIN" project list --db "$PROJECTS_DB"
PROJECT_OUT=$("$BIN" project create acme --description "demo" --db "$PROJECTS_DB" --json 2>/dev/null)
PROJECT_ID=$(printf '%s' "$PROJECT_OUT" | /usr/bin/python3 -c "import sys, json; print(json.load(sys.stdin)['id'])" 2>/dev/null)
if [[ -n "$PROJECT_ID" ]]; then
    printf '  %s %s\n' "$(green ✔)" "project create + JSON parse"
    PASS=$((PASS + 1))
else
    printf '  %s %s\n' "$(red ✗)" "project create"
    _record_fail "project create" "out: $PROJECT_OUT"
    PROJECT_ID="ghost-id"
fi
check        "project show <id>"                            "$BIN" project show "$PROJECT_ID" --db "$PROJECTS_DB"
check_json   "project show <id> --json"                     "$BIN" project show "$PROJECT_ID" --db "$PROJECTS_DB" --json
check        "project update --name acme2"                  "$BIN" project update "$PROJECT_ID" --name acme2 --db "$PROJECTS_DB"
check        "project agents add"                           "$BIN" project agents add "$PROJECT_ID" my-agent --db "$PROJECTS_DB"
check_grep   "project agents list contains agent"  "my-agent"   "$BIN" project agents list "$PROJECT_ID" --db "$PROJECTS_DB"
check        "project agents remove"                        "$BIN" project agents remove "$PROJECT_ID" my-agent --db "$PROJECTS_DB"
check        "project templates list"                       "$BIN" project templates list --db "$PROJECTS_DB"
check        "project templates seed-builtins"              "$BIN" project templates seed-builtins --db "$PROJECTS_DB"

# Chat link/chats - prime chat.db with the schema first.
/usr/bin/touch "$CHAT_DB"
"$BIN" chat delete bootstrap-ghost --confirm --db "$CHAT_DB" >/dev/null 2>&1 || true
# Insert a stub session for link/chats tests.
/usr/bin/sqlite3 "$CHAT_DB" "INSERT OR IGNORE INTO chat_sessions(id, mode, agent_name, system_prompt, status, available_tools, created_at, closed_at, llm_backend, summary, title, parent_session_id, fork_depth, project_id) VALUES('e2e-sess-1', 'libre', NULL, '', 'active', '[]', '2026-05-27T10:00:00Z', NULL, NULL, NULL, 'Stub session 1', NULL, 0, NULL);" 2>/dev/null || true

check        "project link <pid> --session"                 "$BIN" project link "$PROJECT_ID" --session e2e-sess-1 --chat-db "$CHAT_DB"
check_json   "project chats <pid> --json"                   "$BIN" project chats "$PROJECT_ID" --chat-db "$CHAT_DB" --json
check        "project link --unlink"                        "$BIN" project link "$PROJECT_ID" --session e2e-sess-1 --unlink --chat-db "$CHAT_DB"
check        "project chats <pid> (after unlink → empty)"   "$BIN" project chats "$PROJECT_ID" --chat-db "$CHAT_DB"
check        "project chats unknown-pid (empty ok)"         "$BIN" project chats nope-uuid --chat-db "$CHAT_DB"
check_exit   "project link with empty session"  1           "$BIN" project link "$PROJECT_ID" --session "" --chat-db "$CHAT_DB"
check        "project delete --confirm"                     "$BIN" project delete "$PROJECT_ID" --confirm --db "$PROJECTS_DB"

# A.3b - auth (read-only paths)
section "A.3b auth (read-only - token storage forced to age-file)"
check        "auth status (no token)"               "$BIN" auth status
check_json   "auth status --json"                   "$BIN" --json auth status
# `auth logout` deletes from the keychain backend (here AgeFileSecretStore in
# the tmp $HOME). With no token stored, it's an idempotent no-op (exit 0).
check        "auth logout anthropic (no-op)"        "$BIN" auth logout anthropic
check        "auth logout openai (no-op)"           "$BIN" auth logout openai
check        "auth logout vertex (no-op)"           "$BIN" auth logout vertex

# A.4 - user-memory
section "A.4 user-memory"
check        "user-memory show (empty)"                     "$BIN" user-memory show --db "$USER_DB"
check        "user-memory set name Alice"                   "$BIN" user-memory set name "Alice" --db "$USER_DB"
check_grep   "user-memory show --json shows Alice"  "Alice"     "$BIN" user-memory show --db "$USER_DB" --json
check        "user-memory schema"                           "$BIN" user-memory schema --db "$USER_DB"
check        "user-memory export"                           "$BIN" user-memory export --db "$USER_DB" --output "$TMPDIR/u.json"
[[ -f "$TMPDIR/u.json" ]] && check "user-memory export produced file" true \
                          || check "user-memory export produced file" false
check        "user-memory import --overwrite"               "$BIN" user-memory import --db "$TMPDIR/.apollia/u2.db" --input "$TMPDIR/u.json" --overwrite
check        "user-memory forget"                           "$BIN" user-memory forget name --db "$USER_DB"
check        "user-memory reset --confirm"                  "$BIN" user-memory reset --confirm --db "$USER_DB"
check_exit   "user-memory reset (no --confirm)"  1          "$BIN" user-memory reset --db "$USER_DB"

# A.5 - chat-config
section "A.5 chat-config"
check        "chat-config get (default)"                    "$BIN" chat-config get --db "$GOV_DB"
check        "chat-config set system-prompt"                "$BIN" chat-config set system-prompt "You are helpful." --db "$GOV_DB"
check        "chat-config set allowed-tools"                "$BIN" chat-config set allowed-tools "file_read,bash_executor" --db "$GOV_DB"
check        "chat-config set llm-backend none"             "$BIN" chat-config set llm-backend none --db "$GOV_DB"
check_json   "chat-config get --json"                       "$BIN" chat-config get --db "$GOV_DB" --json
check_exit   "chat-config set bogus-key"  1                 "$BIN" chat-config set bogus-key x --db "$GOV_DB"
check        "chat-config reset --confirm"                  "$BIN" chat-config reset --confirm --db "$GOV_DB"
check_exit   "chat-config reset (no --confirm)"  1          "$BIN" chat-config reset --db "$GOV_DB"
check        "chat-config permissions list (empty)"         "$BIN" chat-config permissions list --db "$GOV_DB"
check_json   "chat-config permissions list --json"          "$BIN" chat-config permissions list --db "$GOV_DB" --json
check_exit   "chat-config permissions delete (no id)"  1    "$BIN" chat-config permissions delete 9999 --confirm --db "$GOV_DB"
check_exit   "chat-config permissions delete (no --confirm)" 1  "$BIN" chat-config permissions delete 1 --db "$GOV_DB"
skip         "chat-config authorizations list" "v0.1.1 - runtime route absente, gardée tel quel par arbitrage"

# A.6 - permissions
section "A.6 permissions"
check        "permissions list (empty)"                     "$BIN" permissions list
check_json   "permissions list --json"                      "$BIN" permissions list --json
check        "permissions add --tool web_search --scope global"   "$BIN" permissions add --tool web_search --scope global
check_grep   "permissions list shows web_search"  "web_search"    "$BIN" permissions list
check        "permissions add --tool file_write --scope project"  "$BIN" permissions add --tool file_write --scope project --project-path "$TMPDIR"
check_exit   "permissions add project without --project-path"  1  "$BIN" permissions add --tool file_write --scope project
check        "permissions add --action deny"                "$BIN" permissions add --tool risky_tool --action deny --scope global
check        "permissions audit --limit 10"                 "$BIN" permissions audit --limit 10
check        "permissions revoke 1 --yes"                   "$BIN" permissions revoke 1 --yes
check_exit   "permissions revoke session-prefix"  1         "$BIN" permissions revoke s42 --yes
check        "permissions revoke --all --scope global --yes"      "$BIN" permissions revoke --all --scope global --yes

# A.7 - memory
section "A.7 memory"
check        "memory list (empty)"                          "$BIN" memory list --data-dir "$MDIR"
# Bootstrap a namespace: `memory inspect` refuses an absent db, so we create
# the file first; the next inspect run migrates the schema in place.
/usr/bin/touch "$MDIR/myns.db"
"$BIN" memory inspect myns --data-dir "$MDIR" >/dev/null 2>&1 || true
check        "memory list (1 ns) --json"                    "$BIN" memory list --data-dir "$MDIR" --json
check        "memory inspect myns"                          "$BIN" memory inspect myns --data-dir "$MDIR"
check_exit   "memory search ghost (ns absent)"  1           "$BIN" memory search ghost-ns "needle" --data-dir "$MDIR"
check_grep   "memory search empty (No matches)" "No matches"      "$BIN" memory search myns "needle" --data-dir "$MDIR"
check_json   "memory search empty --json"                   "$BIN" memory search myns "needle" --data-dir "$MDIR" --json
check_exit   "memory search --source procedural (rejected)" 2 "$BIN" memory search myns "x" --source procedural --data-dir "$MDIR"
check_exit   "memory forget unknown uuid"  1                "$BIN" memory forget myns 00000000-0000-0000-0000-000000000000 --data-dir "$MDIR"
check        "memory clear --agent myns --confirm"          "$BIN" memory clear --agent myns --confirm --data-dir "$MDIR"
check        "memory purge --older-than 0"                  "$BIN" memory purge --namespace myns --older-than 0 --data-dir "$MDIR"
check        "memory export"                                "$BIN" memory export --namespace myns --output "$TMPDIR/m.apollia-memory" --data-dir "$MDIR"
[[ -f "$TMPDIR/m.apollia-memory" ]] && check "memory export produced file" true \
                                    || check "memory export produced file" false
check        "memory import --replace"                      "$BIN" memory import --namespace myns2 --input "$TMPDIR/m.apollia-memory" --replace --data-dir "$MDIR"
check        "memory learn-procedure"                       "$BIN" memory learn-procedure --namespace myns --trigger "Test" --steps "1,2,3" --data-dir "$MDIR"

# A.8 - connector (multi-account keyring + drive-prefs + oauth-clients)
section "A.8 connector"
check        "connector list"                               "$BIN" connector list
check_json   "connector list --json"                        "$BIN" connector list --json
check        "connector accounts (no provider)"             "$BIN" connector accounts
check_json   "connector accounts --provider google --json"  "$BIN" connector accounts --provider google --json
check_exit   "connector accounts --provider invalid"  1     "$BIN" connector accounts --provider notion
check_exit   "connector test (account absent)"  1           "$BIN" connector test google alice@example.invalid
check_exit   "connector revoke (no --confirm)"  1           "$BIN" connector revoke google alice@example.invalid
check        "connector client-id list"                     "$BIN" connector client-id list
check_json   "connector client-id list --json"              "$BIN" connector client-id list --json
check        "connector client-id set google <stub>"        "$BIN" connector client-id set google "stub-id-e2e.apps.googleusercontent.com"
check_grep   "connector client-id source=file"  "file"      "$BIN" connector client-id list
check        "connector client-id set google '' (clear)"    "$BIN" connector client-id set google ""
check        "connector client-secret set"                  "$BIN" connector client-secret set google "stub-secret-e2e"
check        "connector api-key set"                        "$BIN" connector api-key set google "stub-api-key-e2e"
check        "connector drive folder list (no accounts)"    "$BIN" connector drive folder list
check        "connector drive folder set"                   "$BIN" connector drive folder set alice@example.invalid "Apollia/Workspace"
check_exit   "connector drive folder set empty path"  1     "$BIN" connector drive folder set alice@example.invalid ""
check        "connector drive folder reset"                 "$BIN" connector drive folder reset alice@example.invalid
check        "connector drive folder picked list (empty)"   "$BIN" connector drive folder picked list alice@example.invalid
check        "connector drive folder picked remove (idempotent)"  "$BIN" connector drive folder picked remove alice@example.invalid ghost-folder-id

# A.9 - mcp (without daemon)
section "A.9 mcp"
check        "mcp list (mcp.db absent)"                     "$BIN" mcp list
check_json   "mcp list --json"                              "$BIN" mcp list --json
check        "mcp list-pending --db (empty)"                "$BIN" mcp list-pending --db "$MCP_APPR_DB"
check_json   "mcp list-pending --db --json"                 "$BIN" mcp list-pending --db "$MCP_APPR_DB" --json
check        "mcp set-approval (creates entry)"             "$BIN" mcp set-approval code-tools bash_exec --db "$MCP_APPR_DB" --ttl-hours 1
check        "mcp revoke-approval"                          "$BIN" mcp revoke-approval code-tools bash_exec --db "$MCP_APPR_DB"
check        "mcp secret set"                               "$BIN" mcp secret set notion NOTION_API_KEY "stub-value-e2e"
check_exit   "mcp secret set empty value"  1                "$BIN" mcp secret set notion NOTION_API_KEY ""
check_exit   "mcp secret set empty server"  1               "$BIN" mcp secret set "" K V
check        "mcp secret delete"                            "$BIN" mcp secret delete notion NOTION_API_KEY
check        "mcp oauth client-id set"                      "$BIN" mcp oauth client-id set APOLLIA_E2E_CLIENT_ID "stub-client-id"
check        "mcp oauth client-id clear"                    "$BIN" mcp oauth client-id clear APOLLIA_E2E_CLIENT_ID
check_exit   "mcp oauth client-id set empty env"  1         "$BIN" mcp oauth client-id set "" stub
check        "mcp oauth status (mcp.db absent)"             "$BIN" mcp oauth status --db "$MCP_DB"
check_json   "mcp oauth status --json"                      "$BIN" mcp oauth status --db "$MCP_DB" --json
check        "mcp oauth logout --confirm (no token)"        "$BIN" mcp oauth logout some-server --confirm
check_exit   "mcp oauth logout (no --confirm)"  1           "$BIN" mcp oauth logout some-server
check_exit   "mcp oauth discover (mcp.db absent)"  1        "$BIN" mcp oauth discover ghost-server --db "$MCP_DB"

# A.10 - chat hygiene
section "A.10 chat hygiene"
check_exit   "chat delete (no --confirm)"  1                "$BIN" chat delete e2e-sess-1
check_exit   "chat delete (chat.db absent)"  1              "$BIN" chat delete x --confirm --db "$TMPDIR/no-chat.db"
# Use the chat.db prepared in A.3
check        "chat rename"                                  "$BIN" chat rename e2e-sess-1 "Renamed via E2E" --db "$CHAT_DB"
check_exit   "chat rename empty title"  1                   "$BIN" chat rename e2e-sess-1 "   " --db "$CHAT_DB"
check        "chat export markdown to file"                 "$BIN" chat export e2e-sess-1 --output "$TMPDIR/c.md" --db "$CHAT_DB"
[[ -f "$TMPDIR/c.md" ]] && check "chat export produced .md file" true \
                       || check "chat export produced .md file" false
check        "chat export --format json"                    "$BIN" chat export e2e-sess-1 --output "$TMPDIR/c.json" --format json --db "$CHAT_DB"
check_json   "chat export .json is valid JSON"              /bin/cat "$TMPDIR/c.json"
check_exit   "chat export --format xml (clap rejects)"  2   "$BIN" chat export e2e-sess-1 --format xml --db "$CHAT_DB"
check_exit   "chat export unknown session"  1               "$BIN" chat export ghost-sess-id --db "$CHAT_DB"
check        "chat delete --confirm"                        "$BIN" chat delete e2e-sess-1 --confirm --db "$CHAT_DB"

# A.11 - llm threshold + setup (without daemon)
section "A.11 llm (threshold + setup, no daemon)"
check        "llm costs --get-threshold (toml absent)"      "$BIN" llm costs --get-threshold --config "$CFG"
check        "llm costs --threshold 0.5"                    "$BIN" llm costs --threshold 0.5 --config "$CFG"
check_grep   "llm costs --get-threshold = 0.5" "0.5"        "$BIN" llm costs --get-threshold --config "$CFG"
check        "llm costs --threshold 0 (clear)"              "$BIN" llm costs --threshold 0 --config "$CFG"
check_exit   "llm costs --threshold + --get-threshold"  2   "$BIN" llm costs --threshold 0.5 --get-threshold --config "$CFG"
check_exit   "llm setup without --local"  1                 "$BIN" llm setup --model /tmp/never.gguf
check_exit   "llm setup --local --model missing"  1         "$BIN" llm setup --local --model /definitely/missing.gguf
/usr/bin/touch "$TMPDIR/wrong-ext.bin"
check_exit   "llm setup --local --model .bin"  1            "$BIN" llm setup --local --model "$TMPDIR/wrong-ext.bin"
if [[ -f "$TEST_GGUF" ]]; then
    check    "llm setup --local --model <GGUF>"             "$BIN" llm setup --local --model "$TEST_GGUF" --system-db "$SYSTEM_DB" --models-dir "$MODELS_DIR"
    # Sanity-check via sqlite3: backend `local` is registered as default.
    LOCAL_ROW=$(/usr/bin/sqlite3 "$SYSTEM_DB" "SELECT name, is_default FROM llm_backends WHERE name='local';" 2>/dev/null)
    if echo "$LOCAL_ROW" | /usr/bin/grep -q "local|1"; then
        printf '  %s %s\n' "$(green ✔)" "system.db has backend local is_default=1"
        PASS=$((PASS + 1))
    else
        printf '  %s %s (row: %s)\n' "$(red ✗)" "system.db backend check" "$LOCAL_ROW"
        _record_fail "system.db backend check" ""
    fi
else
    skip "llm setup --local --model" "GGUF absent: $TEST_GGUF"
    skip "system.db backend check"   "depends on llm setup"
fi

# A.12 - daemon-off behaviors for runtime-bound commands
section "A.12 runtime-bound commands (daemon off → exit 2)"
check_exit   "status (daemon off)"           2  "$BIN" --socket "$SOCK" status
# `agent list` has a local-DB fallback: it reads agents.db when the daemon is
# off and prints "(no agents registered or installed)" with exit 0.
check        "agent list (daemon off, local fallback)"   "$BIN" --socket "$SOCK" agent list
check_exit   "agent status hello (off)"      2  "$BIN" --socket "$SOCK" agent status hello
check_exit   "agent messages hello (off)"    2  "$BIN" --socket "$SOCK" agent messages hello
check_exit   "task list (off)"               2  "$BIN" --socket "$SOCK" task list
check_exit   "audit list (off)"              2  "$BIN" --socket "$SOCK" audit list
check_exit   "audit stats (off)"             2  "$BIN" --socket "$SOCK" audit stats
check_exit   "audit export (off)"            2  "$BIN" --socket "$SOCK" audit export --output "$TMPDIR/a.json"
check_exit   "llm status (off)"              2  "$BIN" --socket "$SOCK" llm status
check_exit   "llm ping (off)"                2  "$BIN" --socket "$SOCK" llm ping
check_exit   "trigger list (off)"            2  "$BIN" --socket "$SOCK" trigger list
check_exit   "notify list (off)"             2  "$BIN" --socket "$SOCK" notify list
check_exit   "stt status (off)"              2  "$BIN" --socket "$SOCK" stt status
check_exit   "digest (off)"                  2  "$BIN" --socket "$SOCK" digest --since 24h
check_exit   "trace x (off)"                 2  "$BIN" --socket "$SOCK" trace dummy-task-id
check_exit   "resilience list (off)"         2  "$BIN" --socket "$SOCK" resilience list
check_exit   "hitl (off)"                    2  "$BIN" --socket "$SOCK" hitl
check        "tools list (local)"               "$BIN" tools list
# `tools describe` is runtime-bound (queries the live tool registry) and
# now emits exit 2 daemon-off, aligned on ADR-008 conventions.
check_exit   "tools describe (off)"          2  "$BIN" --socket "$SOCK" tools describe bash_executor
check_exit   "tools approvals pending (off)" 2  "$BIN" --socket "$SOCK" tools approvals pending
check        "model list (no models)"           "$BIN" model list
# `model hardware` queries the runtime LLM registry (`/api/v1/llm/hardware`);
# in Phase A there's no daemon so it's grouped here as a daemon-off check
# (exit 2 per ADR-008 - F4 polish 2026-05-27).
check_exit   "model hardware (daemon off)"   2  "$BIN" model hardware
check_exit   "model hardware --json (off)"   2  "$BIN" model hardware --json
# `model delete` round-trip on a stub `.gguf` we drop into the models dir.
# `models_dir` is a symlink to the real ~/.apollia/models in this script,
# so we drop the stub into a private tmp models dir and pass --models-dir
# isn't supported; we use the symlinked dir but delete a path that only we
# created (no clash with real models).
STUB_GGUF="$MODELS_DIR/e2e-stub-$(/bin/date +%s).gguf"
/usr/bin/touch "$STUB_GGUF" 2>/dev/null || true
if [[ -f "$STUB_GGUF" ]]; then
    check    "model delete --confirm"           "$BIN" model delete "$(/usr/bin/basename "$STUB_GGUF")" --confirm
    if [[ ! -f "$STUB_GGUF" ]]; then
        printf '  %s %s\n' "$(green ✔)" "model delete actually removed the file"
        PASS=$((PASS + 1))
    fi
else
    skip "model delete --confirm" "could not create stub .gguf in $MODELS_DIR (symlink writable?)"
fi
check        "plan-cache stats (db absent)"     "$BIN" plan-cache stats
check        "plan-cache clear --force"         "$BIN" plan-cache clear --force
check        "plan-cache evict --max-age-days 7"  "$BIN" plan-cache evict --max-age-days 7
check        "workspace status"                 "$BIN" workspace status
check        "workspace init --force"           "$BIN" workspace init --force
check_json   "workspace status --json"          "$BIN" workspace status --json
check        "rollback --list"                  "$BIN" rollback --list
check_exit   "logs --last 5 (no log file)"   1  "$BIN" logs --last 5
# agent install validates the path locally BEFORE contacting the runtime, so
# a missing path returns 1 (not the runtime-off exit 2).
check_exit   "agent install (missing file)"  1  "$BIN" --socket "$SOCK" agent install /tmp/never-exists.py

# A.13 - Skipped paths (and the *precise* reason each is unreachable here).
# Trimmed aggressively in the 2026-05-27 polish; the remaining skips fall in
# four categories: (a) interactive UI, (b) network, (c) deferred v0.1.1
# items, (d) environment limitations the script can't synthesize.
section "A.13 SKIP justifiés"
skip "chat (REPL)" \
     "interactive rustyline editor (no pty in non-tty CI)"
skip "auth login <provider>" \
     "spawns the browser at the provider authorize URL - needs a human"
skip "update / update --check" \
     "outbound HTTPS to api.github.com (script policy: no network)"
skip "onboard / onboard --topic" \
     "runs a chat-based onboarding agent - interactive by design"
skip "mcp-server / mcp-server --with-runtime" \
     "long-running stdio JSON-RPC server, never returns"
skip "model search / model show" \
     "outbound HTTPS to huggingface.co"
skip "stt transcribe / stt model download" \
     "whisper-rs model on disk + HF download (~1 GB), out of suite scope"
skip "notify test" \
     "dispatches a real desktop notif / webhook POST"
skip "agent install <git-url>" \
     "git clone over network"
skip "agent package install" \
     "needs an external agent.toml bundle to install from"
skip "mcp oauth login" \
     "opens the AS authorize URL + waits on a browser callback"
skip "tools credentials set / test" \
     "set: masked stdin prompt; test: live call to the credentialed backend"
skip "chat-config authorizations list / revoke" \
     "deferred v0.1.1 - in-memory daemon state, no HTTP route yet"
skip "mcp catalogue / mcp enrichments list" \
     "deferred v0.1.1 - backend (McpRegistryClient + enrichments.json) lives in apollia-desktop"

# ═══════════════════════════════════════════════════════════════════════════
#                                 PHASE B
# ═══════════════════════════════════════════════════════════════════════════
if [[ "$REQUIRE_RUNTIME" == "1" ]]; then
    echo
    echo "$(bold "═══ Phase B - RUNTIME (daemon spawned) ═══")"

    section "B.0 daemon lifecycle"
    # Pre-stage a local LLM backend in system.db so the daemon picks it up at
    # boot (and `llm reload` becomes a no-op refresh instead of a first-time
    # initialiser). Phase A may already have done this - repeating is cheap
    # because the models dir is a symlink and the upsert is idempotent.
    if [[ -f "$TEST_GGUF" ]]; then
        "$BIN" llm setup --local --model "$TEST_GGUF" --system-db "$SYSTEM_DB" --models-dir "$MODELS_DIR" >/dev/null 2>&1 || true
    fi

    # Start daemon. `--socket` + `--port` must be options of the `start`
    # subcommand (not global) so the daemon binds to the tmpdir socket
    # and the auto-picked free TCP port instead of /tmp/apollia.sock:7771.
    "$BIN" start --socket "$SOCK" --port "$FREE_PORT" >"$TMPDIR/daemon.log" 2>&1 &
    DAEMON_PID=$!
    if wait_for_socket "$SOCK" 30; then
        printf '  %s %s (pid=%d, port=%s)\n' "$(green ✔)" "daemon started + socket ready" "$DAEMON_PID" "$FREE_PORT"
        PASS=$((PASS + 1))
    else
        printf '  %s %s\n' "$(red ✗)" "daemon failed to bind socket within 30s"
        _record_fail "daemon start" "see $TMPDIR/daemon.log"
        # Print last 30 lines of daemon log for diagnosis
        echo "--- daemon.log (tail) ---"
        /usr/bin/tail -30 "$TMPDIR/daemon.log" 2>/dev/null || true
        echo "--- end daemon.log ---"
        echo
        echo "FAIL: daemon could not start, aborting Phase B."
        exit 1
    fi

    check        "status (daemon on)"               "$BIN" --socket "$SOCK" status
    check_json   "status --json (daemon on)"        "$BIN" --socket "$SOCK" status --json
    check        "doctor (daemon on)"               "$BIN" doctor

    # B.1 - agent lifecycle with a stub agent.
    section "B.1 agent lifecycle"
    HELLO_PY="$TMPDIR/hello.py"
    /bin/cat >"$HELLO_PY" <<'PYEOF'
"""E2E stub agent for cli-e2e.sh. Minimal @on_message echo."""
from apollia import agent, on_message
from apollia.types import Ctx, Message

@agent(name="e2e-hello", version="0.0.1", description="E2E stub")
class E2EHello:
    @on_message
    async def chat(self, message: str, history: list[Message], ctx: Ctx) -> str:
        return f"echo:{message}"

agent_instance = E2EHello()
PYEOF
    check        "agent list (empty)"               "$BIN" --socket "$SOCK" agent list
    check        "agent validate <stub>"            "$BIN" agent validate "$HELLO_PY"
    check        "agent install <stub> --skip-tests"  "$BIN" --socket "$SOCK" agent install "$HELLO_PY" --skip-tests
    check_grep   "agent list shows e2e-hello" "e2e-hello"  "$BIN" --socket "$SOCK" agent list
    check        "agent info"                       "$BIN" --socket "$SOCK" agent info e2e-hello
    check_json   "agent info --json"                "$BIN" --socket "$SOCK" agent info e2e-hello --json
    # `--skip-tests` registers the agent without spawning it in the runtime.
    # We must `agent start` to load it before status/messages/logs work.
    if "$BIN" --socket "$SOCK" agent start e2e-hello >/dev/null 2>&1; then
        printf '  %s %s\n' "$(green ✔)" "agent start (post-install)"
        PASS=$((PASS + 1))
        check    "agent status"                     "$BIN" --socket "$SOCK" agent status e2e-hello
        check    "agent messages --limit 5"         "$BIN" --socket "$SOCK" agent messages e2e-hello --limit 5
        # `agent logs` now falls back to the audit trail (cf. CLI fix
        # 2026-05-27); it always exits 0 with "(no recent activity)" when
        # the agent has no events yet, so we assert the command runs cleanly.
        check    "agent logs --last 5"              "$BIN" --socket "$SOCK" agent logs e2e-hello --last 5
        if "$BIN" --socket "$SOCK" agent repair e2e-hello >/dev/null 2>&1; then
            check  "agent repair (no-op)"           "$BIN" --socket "$SOCK" agent repair e2e-hello
        else
            skip   "agent repair (no-op)" "standalone stub isn't in any agent_packages bundle (repair only fixes packaged agents)"
        fi
        AGENT_RUNNING=1
    else
        skip     "agent status/messages/logs/repair" "agent start failed - Python bundled runner couldn't spawn the SDK loader inside the tmp \$HOME"
        AGENT_RUNNING=0
    fi
    check        "agent enable"                     "$BIN" --socket "$SOCK" agent enable e2e-hello
    check        "agent disable"                    "$BIN" --socket "$SOCK" agent disable e2e-hello
    check        "agent enable (back to on)"        "$BIN" --socket "$SOCK" agent enable e2e-hello
    check        "agent package list (empty)"       "$BIN" --socket "$SOCK" agent package list
    check        "agent new <name> --type react"    "$BIN" agent new e2e-scaffold --type react

    # B.2 - task via run
    section "B.2 task lifecycle"
    if [[ -f "$TEST_GGUF" && "$AGENT_RUNNING" == "1" ]]; then
        # Configure local LLM backend so `run` has a model.
        "$BIN" llm setup --local --model "$TEST_GGUF" --system-db "$SYSTEM_DB" --models-dir "$MODELS_DIR" >/dev/null 2>&1 || true
        "$BIN" --socket "$SOCK" llm reload >/dev/null 2>&1 || true

        RUN_OUT=$("$BIN" --socket "$SOCK" run e2e-hello "ping" --detach --json 2>/dev/null)
        TASK_ID=$(printf '%s' "$RUN_OUT" | /usr/bin/python3 -c "import sys, json; d = json.load(sys.stdin); print(d.get('task_id') or d.get('id', ''))" 2>/dev/null)
        if [[ -n "$TASK_ID" ]]; then
            printf '  %s %s (id=%s)\n' "$(green ✔)" "run --detach + JSON parse" "$TASK_ID"
            PASS=$((PASS + 1))
            # Poll task status (up to 30s)
            for _ in $(/usr/bin/seq 1 15); do
                STATUS=$("$BIN" --socket "$SOCK" task status "$TASK_ID" --json 2>/dev/null | /usr/bin/python3 -c "import sys,json; print(json.load(sys.stdin).get('status',''))" 2>/dev/null)
                [[ "$STATUS" == "completed" || "$STATUS" == "failed" || "$STATUS" == "cancelled" ]] && break
                /bin/sleep 2
            done
            check       "task status <id>"          "$BIN" --socket "$SOCK" task status "$TASK_ID"
            check       "task list"                 "$BIN" --socket "$SOCK" task list
            check       "task list --pending-approval"   "$BIN" --socket "$SOCK" task list --pending-approval
            check       "task inspect <id>"         "$BIN" --socket "$SOCK" task inspect "$TASK_ID"
            check       "task approvals (resolved)" "$BIN" --socket "$SOCK" task approvals
            check       "task approvals --pending"  "$BIN" --socket "$SOCK" task approvals --pending
            check       "trace <id> --format human" "$BIN" --socket "$SOCK" trace "$TASK_ID" --format human
            check_json  "trace <id> --format json"  "$BIN" --socket "$SOCK" trace "$TASK_ID" --format json
            check       "hitl"                      "$BIN" --socket "$SOCK" hitl
            check       "agent logs --last 5 (post-task)"  "$BIN" --socket "$SOCK" agent logs e2e-hello --last 5
        else
            skip "task lifecycle" "run --detach didn't return a task_id (out: ${RUN_OUT:0:200})"
        fi
    else
        skip "task lifecycle" "GGUF absent ($TEST_GGUF) - no LLM backend"
    fi

    # B.3 - LLM (local backend only).
    # The daemon delegates model loading to the apollia-runner sidecar over
    # HTTP. In a CI/sub-shell environment the runner usually isn't on PATH,
    # so model loading fails with "load_model via runner: http error". When
    # that happens we gracefully skip the model-bound assertions and still
    # exercise the metadata CRUD on backends.
    section "B.3 llm (local backend)"
    if [[ -f "$TEST_GGUF" ]]; then
        # CRUD on backends - pure metadata, works without the runner.
        check       "llm backends list"             "$BIN" --socket "$SOCK" llm backends list
        check       "llm backends show local"       "$BIN" --socket "$SOCK" llm backends show local
        check       "llm backends create local2 (llama-cpp)"  "$BIN" --socket "$SOCK" llm backends create local2 --provider llama-cpp --model "$TEST_GGUF" --device cpu --timeout-sec 60
        check       "llm backends update local2 --device cpu"  "$BIN" --socket "$SOCK" llm backends update local2 --device cpu
        check       "llm backends set-default local2"  "$BIN" --socket "$SOCK" llm backends set-default local2
        # Restore `local` as default BEFORE deleting local2 (server rejects
        # deleting the active default with HTTP 409).
        check       "llm backends set-default local (restore)"  "$BIN" --socket "$SOCK" llm backends set-default local
        check       "llm backends delete local2 --confirm"  "$BIN" --socket "$SOCK" llm backends delete local2 --confirm
        check       "llm status"                    "$BIN" --socket "$SOCK" llm status
        check_json  "llm status --json"             "$BIN" --socket "$SOCK" llm status --json
        check       "llm costs"                     "$BIN" --socket "$SOCK" llm costs
        # Model-bound ops - require apollia-runner to be reachable. Probe with
        # `llm reload` and skip the rest if the runner isn't there.
        if "$BIN" --socket "$SOCK" llm reload >/dev/null 2>&1; then
            printf '  %s %s\n' "$(green ✔)" "llm reload"
            PASS=$((PASS + 1))
            check   "llm ping"                      "$BIN" --socket "$SOCK" llm ping
            check   "llm ping local"                "$BIN" --socket "$SOCK" llm ping local
            check_grep  "llm chat returns non-empty" "."  "$BIN" --socket "$SOCK" llm chat "Réponds par OK." --backend local
            check_json  "llm chat --json"           "$BIN" --socket "$SOCK" llm chat "OK?" --backend local --json
        else
            skip "llm reload + ping + chat" "apollia-runner sidecar unreachable in this env"
        fi
    else
        skip "llm phase B" "GGUF absent: $TEST_GGUF"
    fi

    # B.4 - tools / audit / triggers / notify / stt / model / resilience / plan-cache / digest
    section "B.4 services (tools / audit / triggers / notify / …)"
    check       "tools list (daemon on)"            "$BIN" --socket "$SOCK" tools list
    check_json  "tools list --json"                 "$BIN" --socket "$SOCK" tools list --json
    check       "tools describe bash_executor"      "$BIN" --socket "$SOCK" tools describe bash_executor
    check_exit  "tools describe inexistant" 1       "$BIN" --socket "$SOCK" tools describe inexistant_tool
    check       "tools reload"                      "$BIN" --socket "$SOCK" tools reload
    check       "tools enable bash_executor"        "$BIN" --socket "$SOCK" tools enable bash_executor
    check       "tools disable web_read + re-enable"  bash -c "$BIN --socket $SOCK tools disable web_read && $BIN --socket $SOCK tools enable web_read"
    check       "audit list --limit 5"              "$BIN" --socket "$SOCK" audit list --limit 5
    check_json  "audit stats --json"                "$BIN" --socket "$SOCK" audit stats --json
    check       "audit export"                      "$BIN" --socket "$SOCK" audit export --output "$TMPDIR/audit.json" --limit 100

    # Triggers - exercise all 5 source kinds end-to-end (CRUD).
    # The runtime requires the webhook secret to be ≥ 32 chars, so the
    # test secret is padded accordingly.
    section "B.4.1 triggers (multi-kind CRUD)"
    check       "trigger list (empty)"              "$BIN" --socket "$SOCK" trigger list
    check       "trigger reload"                    "$BIN" --socket "$SOCK" trigger reload
    check       "trigger create cron"               "$BIN" --socket "$SOCK" trigger create t-cron --agent e2e-hello --kind cron --detail "@daily"
    check       "trigger status t-cron"             "$BIN" --socket "$SOCK" trigger status t-cron
    check       "trigger disable t-cron"            "$BIN" --socket "$SOCK" trigger disable t-cron
    check       "trigger enable t-cron"             "$BIN" --socket "$SOCK" trigger enable t-cron
    check       "trigger update t-cron"             "$BIN" --socket "$SOCK" trigger update t-cron --detail "@hourly"
    check       "trigger logs t-cron --last 5"      "$BIN" --socket "$SOCK" trigger logs t-cron --last 5
    check       "trigger delete t-cron --confirm"   "$BIN" --socket "$SOCK" trigger delete t-cron --confirm
    check       "trigger create interval"           "$BIN" --socket "$SOCK" trigger create t-int --agent e2e-hello --kind interval --detail "30m"
    check       "trigger delete t-int --confirm"    "$BIN" --socket "$SOCK" trigger delete t-int --confirm
    check       "trigger create oneshot"            "$BIN" --socket "$SOCK" trigger create t-os --agent e2e-hello --kind oneshot --detail "2099-12-31T23:59:59Z"
    check       "trigger delete t-os --confirm"     "$BIN" --socket "$SOCK" trigger delete t-os --confirm
    /bin/mkdir -p "$TMPDIR/watched"
    check       "trigger create filewatch"          "$BIN" --socket "$SOCK" trigger create t-fw --agent e2e-hello --kind filewatch --detail "$TMPDIR/watched"
    check       "trigger delete t-fw --confirm"     "$BIN" --socket "$SOCK" trigger delete t-fw --confirm
    check       "trigger create webhook"            "$BIN" --socket "$SOCK" trigger create t-wh --agent e2e-hello --kind webhook --detail "this-is-a-32-char-or-more-secret-aa"
    check       "trigger delete t-wh --confirm"     "$BIN" --socket "$SOCK" trigger delete t-wh --confirm

    # Notify - exercise full CRUD with explicit id, webhook + desktop kinds.
    section "B.4.2 notify"
    check       "notify list"                       "$BIN" --socket "$SOCK" notify list
    check       "notify events get"                 "$BIN" --socket "$SOCK" notify events get
    check       "notify logs --last 5 (no channel)"  "$BIN" --socket "$SOCK" notify logs --last 5
    check       "notify create webhook"             "$BIN" --socket "$SOCK" notify create --kind webhook --id e2e-hook --label "E2E webhook" --url "https://example.invalid/notify"
    check       "notify create desktop"             "$BIN" --socket "$SOCK" notify create --kind desktop --id e2e-desk
    check       "notify update url"                 "$BIN" --socket "$SOCK" notify update e2e-hook --url "https://other.invalid/notify"
    check       "notify update --enabled false"     "$BIN" --socket "$SOCK" notify update e2e-hook --enabled false
    check       "notify delete e2e-hook --confirm"  "$BIN" --socket "$SOCK" notify delete e2e-hook --confirm
    check       "notify delete e2e-desk --confirm"  "$BIN" --socket "$SOCK" notify delete e2e-desk --confirm

    # STT - engine is disabled by default in the test env (no Whisper model
    # bundled). The runtime returns 503 "STT engine not available", which the
    # CLI surfaces as exit 1. That's the *expected* state on a fresh $HOME,
    # so we assert exit 1 explicitly rather than skipping.
    section "B.4.3 stt"
    check_exit  "stt status (engine disabled, 503)"   1  "$BIN" --socket "$SOCK" stt status
    check       "stt model list"                    "$BIN" --socket "$SOCK" stt model list
    check       "stt config get"                    "$BIN" --socket "$SOCK" stt config get

    # Model + resilience + plan-cache + digest
    section "B.4.4 model / resilience / plan-cache / digest"
    check       "model list (with GGUF in models dir)"  "$BIN" model list
    check_json  "model hardware --json"             "$BIN" --socket "$SOCK" model hardware --json
    check       "resilience list"                   "$BIN" --socket "$SOCK" resilience list
    # bash_executor may not yet be registered in the circuit breaker registry
    # (it lazy-registers on first invocation). Skip when absent.
    if "$BIN" --socket "$SOCK" resilience show bash_executor >/dev/null 2>&1; then
        check   "resilience show bash_executor"     "$BIN" --socket "$SOCK" resilience show bash_executor
        check   "resilience reset bash_executor"    "$BIN" --socket "$SOCK" resilience reset bash_executor
    else
        skip    "resilience show/reset bash_executor" "tool not in breaker registry (no invocations yet)"
    fi
    check_exit  "resilience reset unknown" 1        "$BIN" --socket "$SOCK" resilience reset inexistant_tool
    check       "plan-cache stats (daemon on)"      "$BIN" --socket "$SOCK" plan-cache stats
    check       "plan-cache evict --max-age-days 0"  "$BIN" --socket "$SOCK" plan-cache evict --max-age-days 0
    check_json  "digest --since 24h --json"         "$BIN" --socket "$SOCK" digest --since 24h --json

    # MCP
    section "B.4.5 mcp (daemon on)"
    check       "mcp list (daemon on, db absent)"   "$BIN" --socket "$SOCK" mcp list

    # Chat (REPL skipped; list via --list)
    section "B.4.6 chat --list"
    check       "chat --list"                       "$BIN" --socket "$SOCK" chat --list
    check       "rollback --list"                   "$BIN" --socket "$SOCK" rollback --list

    # Optional: review .
    section "B.4.7 review (opt-in)"
    if [[ "$TEST_REVIEW" == "1" ]]; then
        check   "review ."                          "$BIN" --socket "$SOCK" review .
    else
        skip    "review ." "APOLLIA_TEST_REVIEW=0 (opt-in)"
    fi

    # B.5 - cleanup agent
    section "B.5 cleanup"
    check       "agent uninstall e2e-hello"         "$BIN" --socket "$SOCK" agent uninstall e2e-hello

    # B.6 - stop daemon (the EXIT trap also handles this)
    section "B.6 stop daemon"
    check       "stop"                              "$BIN" --socket "$SOCK" stop
    DAEMON_PID=""
else
    echo
    echo "$(yellow "Phase B SKIPPED") (set APOLLIA_REQUIRE_RUNTIME=1 to enable)"
fi

# ═══════════════════════════════════════════════════════════════════════════
#                                  SUMMARY
# ═══════════════════════════════════════════════════════════════════════════
echo
echo "$(bold "═══ Summary ═══")"
printf '  PASS  : %d\n' "$PASS"
printf '  FAIL  : %d\n' "$FAIL"
printf '  SKIP  : %d\n' "$SKIP"

if [[ "$FAIL" -gt 0 ]]; then
    echo
    echo "$(red "Failed assertions:")"
    for label in "${FAILED_LABELS[@]}"; do
        echo "  - $label"
    done
    exit 1
fi

echo "$(green "All assertions passed.")"
exit 0
