#!/usr/bin/env bash
# Smoke test suite for the `apollia-os` binary.
#
# Covers every command that can run without a live daemon (doctor, version,
# config, project, user-memory, chat-config, connector list, ...). Commands
# that need the runtime print a friendly "runtime not started" error and are
# expected to exit 2 — we verify that behaviour.
#
# Usage:
#   tests/cli/cli_integration.sh                # uses ./target/debug/apollia-os
#   APOLLIA_BIN=/path/to/apollia-os tests/cli/cli_integration.sh
#
# Exits 0 on success, 1 on the first failing assertion.
set -euo pipefail

# Resolve the apollia-os binary: explicit override > debug build > release build.
if [[ -n "${APOLLIA_BIN:-}" ]]; then
    BIN=$APOLLIA_BIN
elif [[ -x ./target/debug/apollia-os ]]; then
    BIN=./target/debug/apollia-os
elif [[ -x ./target/release/apollia-os ]]; then
    BIN=./target/release/apollia-os
else
    echo "FAIL: apollia-os binary not found."
    echo "  Tried APOLLIA_BIN, ./target/debug/apollia-os, ./target/release/apollia-os."
    echo "  Run \`cargo build -p apollia-cli\` or \`cargo build --release -p apollia-cli\`,"
    echo "  or set APOLLIA_BIN=/absolute/path/to/apollia-os."
    exit 1
fi

# Isolated environment so we never touch the user's real ~/.apollia.
TMPDIR=$(mktemp -d -t apollia-cli-test.XXXXXX)
trap 'rm -rf "$TMPDIR"' EXIT
export HOME=$TMPDIR
export APOLLIA_BIN=$BIN
mkdir -p "$TMPDIR/.apollia/logs"

PASS=0
FAIL=0
LOG=$TMPDIR/run.log

check() {
    local label=$1
    shift
    if "$@" >>"$LOG" 2>&1; then
        printf '  [PASS] %s\n' "$label"
        PASS=$((PASS + 1))
    else
        printf '  [FAIL] %s (exit %d)\n' "$label" "$?"
        FAIL=$((FAIL + 1))
    fi
}

check_exit() {
    local label=$1
    local expected=$2
    shift 2
    local rc=0
    "$@" >>"$LOG" 2>&1 || rc=$?
    if [[ "$rc" == "$expected" ]]; then
        printf '  [PASS] %s (rc=%d)\n' "$label" "$rc"
        PASS=$((PASS + 1))
    else
        printf '  [FAIL] %s (rc=%d, expected %d)\n' "$label" "$rc" "$expected"
        FAIL=$((FAIL + 1))
    fi
}

echo "=== apollia-os smoke tests ==="
echo "  HOME=$TMPDIR"
echo "  BIN=$BIN"
echo

# ── Always-on commands (no runtime) ──
echo "--- version / help ---"
check "version --json"        "$BIN" version --json
check "help"                  "$BIN" help

echo "--- doctor ---"
# doctor returns 0 even with warnings; only "errors" trigger non-zero.
check "doctor"                "$BIN" doctor
check "doctor --json"         "$BIN" doctor --json

echo "--- config ---"
CFG=$TMPDIR/.apollia/apollia.toml
check "config validate (absent)"  "$BIN" config validate
check "config set llm.default"    "$BIN" config set llm.default anthropic --file "$CFG"
check "config get llm.default"    "$BIN" config get llm.default --file "$CFG"
# A partial [llm] section (default without backends) is intentionally rejected
# by the runtime schema validator, so we expect exit 1 here.
check_exit "config validate rejects partial llm" 1 "$BIN" config validate --file "$CFG"
check "config show --json"        "$BIN" config show --json --file "$CFG"

echo "--- project (local-first) ---"
PDB=$TMPDIR/.apollia/projects.db
check "project list (empty)"      "$BIN" project list --db "$PDB"
PROJECT_OUT=$("$BIN" project create acme --description "demo" --db "$PDB" --json)
PROJECT_ID=$(printf '%s' "$PROJECT_OUT" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
check "project show"              "$BIN" project show "$PROJECT_ID" --db "$PDB"
check "project agents add"        "$BIN" project agents add "$PROJECT_ID" my-agent --db "$PDB"
check "project agents list"       "$BIN" project agents list "$PROJECT_ID" --db "$PDB"
check "project agents remove"     "$BIN" project agents remove "$PROJECT_ID" my-agent --db "$PDB"
check "project delete --confirm"  "$BIN" project delete "$PROJECT_ID" --confirm --db "$PDB"

echo "--- user-memory (local-first) ---"
UDB=$TMPDIR/.apollia/user_memory.db
check "user-memory show empty"    "$BIN" user-memory show --db "$UDB"
check "user-memory set"           "$BIN" user-memory set name "Alice" --db "$UDB"
check "user-memory show with value" "$BIN" user-memory show --db "$UDB" --json
check "user-memory forget"        "$BIN" user-memory forget name --db "$UDB"
check "user-memory reset --confirm" "$BIN" user-memory reset --confirm --db "$UDB"

echo "--- chat-config (local-first) ---"
GDB=$TMPDIR/.apollia/governance.db
check "chat-config get default"   "$BIN" chat-config get --db "$GDB"
check "chat-config set prompt"    "$BIN" chat-config set system-prompt "You are helpful." --db "$GDB"
check "chat-config set tools"     "$BIN" chat-config set allowed-tools "file_read,bash" --db "$GDB"
check "chat-config get updated"   "$BIN" chat-config get --db "$GDB" --json
check "chat-config reset"         "$BIN" chat-config reset --confirm --db "$GDB"

echo "--- connector (multi-account keyring) ---"
# connector list reads the registry only; safe even without runtime.
check "connector list"            "$BIN" connector list
check "connector accounts (empty)" "$BIN" connector accounts

echo "--- runtime-dependent commands (expected: exit 2 when daemon off) ---"
# These call /api/v1/* via the Unix socket. With no daemon we expect rc 2 (RUNTIME_ERROR).
SOCK=$TMPDIR/apollia-test.sock
check_exit "status (daemon off)"  2 "$BIN" --socket "$SOCK" status
# `mcp list` reads ~/.apollia/mcp.toml locally; with no config present it
# returns 0 with an empty list rather than touching the runtime.
check_exit "mcp list (no config)"  0 "$BIN" --socket "$SOCK" mcp list
check_exit "audit list (daemon off)" 2 "$BIN" --socket "$SOCK" audit list

echo
echo "=== Summary ==="
printf '  PASS: %d\n' "$PASS"
printf '  FAIL: %d\n' "$FAIL"
echo "  Log : $LOG"
echo

[[ "$FAIL" -eq 0 ]]
