#!/usr/bin/env bash
# Reset all agent caches for clean testing.
#
# Usage:
#   ./scripts/reset-agent-cache.sh [agent-name] [--keep-chat]
#
# Without agent-name: resets ALL agent memories + ALL chat sessions.
# With agent-name:   resets only that agent's memory (chat sessions are still
#                    cleared unless --keep-chat is passed, because the UI's
#                    "previous response" lives in chat.db).
#
# The script STOPS the running apollia-os daemon before clearing anything.
# Two caches would otherwise survive the reset and make new code look stale:
#
#   1. PyO3 keeps every Python agent module in the running interpreter's
#      memory. Editing agents/assistants/<name>.py on disk has no effect
#      until the process restarts — the interpreter never re-reads the file.
#
#   2. chat.db (+ -wal/-shm) stores every chat session. On restart the
#      ChatSessionManager restores active sessions, and the UI replays the
#      stored message history, so an "old" assistant response is still
#      visible and can be mistaken for a re-generated one.
#
# You must run `apollia-os start` yourself afterwards.

set -euo pipefail

APOLLIA_DIR="${HOME}/.apollia"
AGENTS_SOURCE="$(cd "$(dirname "$0")/.." && pwd)/agents"
SOCKET="/tmp/apollia.sock"

agent_name=""
keep_chat=false

for arg in "$@"; do
    case "$arg" in
        --keep-chat) keep_chat=true ;;
        --help|-h)
            sed -n '2,27p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        --*)
            echo "Unknown flag: $arg" >&2
            exit 1
            ;;
        *)
            if [ -n "$agent_name" ]; then
                echo "Unexpected extra argument: $arg" >&2
                exit 1
            fi
            agent_name="$arg"
            ;;
    esac
done

echo "=== Apollia Agent Cache Reset ==="

# ── [0/4] Stop the running daemon ─────────────────────────────────────────────
# PyO3 caches Python modules in the running interpreter. .py file edits only
# take effect after a full process restart.
stop_daemon() {
    if [ ! -S "$SOCKET" ]; then
        echo "[0/4] No running daemon detected (socket $SOCKET absent)"
        return
    fi

    echo "[0/4] Stopping running apollia-os daemon..."
    if command -v apollia-os >/dev/null 2>&1 \
        && apollia-os stop >/dev/null 2>&1; then
        :
    else
        # Fall back to SIGTERM on whoever owns the Unix socket.
        pid="$(lsof -t -U -- "$SOCKET" 2>/dev/null | head -n1 || true)"
        if [ -n "$pid" ]; then
            echo "       'apollia-os stop' unavailable — SIGTERM to PID $pid"
            kill "$pid" 2>/dev/null || true
        fi
    fi

    # Wait up to 10 s for the socket to disappear.
    for _ in 1 2 3 4 5 6 7 8 9 10; do
        [ -S "$SOCKET" ] || break
        sleep 1
    done
    if [ -S "$SOCKET" ]; then
        echo "       WARNING: daemon socket still present after 10 s" >&2
    else
        echo "       stopped"
    fi
}

stop_daemon

# ── [1/4] Clear Python bytecode cache ─────────────────────────────────────────
# Safety net — the running interpreter is already gone, so bytecode on disk
# would be reloaded next start. Clearing avoids stale .pyc shadowing .py.
echo "[1/4] Clearing __pycache__..."
find "${APOLLIA_DIR}/agents" -type d -name "__pycache__" \
    -exec rm -rf {} + 2>/dev/null || true
find "${AGENTS_SOURCE}" -type d -name "__pycache__" \
    -exec rm -rf {} + 2>/dev/null || true

# ── [2/4] Clear agent memory databases ────────────────────────────────────────
# Agent memory lives at ~/.apollia/memory/<namespace>.db — clear the WAL and
# SHM companion files too or SQLite WAL recovery re-materialises old data.
remove_sqlite() {
    # $1 = base path (e.g. ~/.apollia/chat.db)
    local base="$1"
    rm -f -- "$base" "${base}-wal" "${base}-shm" "${base}-journal"
}

if [ -n "$agent_name" ]; then
    echo "[2/4] Clearing memory for: ${agent_name}"
    remove_sqlite "${APOLLIA_DIR}/memory/${agent_name}.db"
else
    echo "[2/4] Clearing ALL agent memories..."
    for db in "${APOLLIA_DIR}/memory/"*.db; do
        [ -f "$db" ] || continue
        remove_sqlite "$db"
    done
fi

# ── [3/4] Clear chat sessions (unless --keep-chat) ────────────────────────────
if $keep_chat; then
    echo "[3/4] Keeping chat sessions (--keep-chat)"
else
    echo "[3/4] Clearing chat sessions (chat.db)..."
    remove_sqlite "${APOLLIA_DIR}/chat.db"
fi

# ── [4/4] Re-sync agent .py from source ───────────────────────────────────────
sync_agent_py() {
    # $1 = agent name
    local name="$1"
    local dst="${APOLLIA_DIR}/agents/${name}/agent.py"
    if [ ! -d "$(dirname "$dst")" ]; then
        echo "  skip ${name}: not installed"
        return
    fi
    for src_dir in assistants workers system; do
        local src="${AGENTS_SOURCE}/${src_dir}/${name}.py"
        if [ -f "$src" ]; then
            cp "$src" "$dst"
            echo "  synced: ${name} (from ${src_dir})"
            return
        fi
    done
    echo "  skip ${name}: source .py not found under agents/{assistants,workers,system}"
}

echo "[4/4] Re-syncing agent sources..."
if [ -n "$agent_name" ]; then
    sync_agent_py "$agent_name"
else
    for agent_dir in "${APOLLIA_DIR}/agents"/*/; do
        sync_agent_py "$(basename "$agent_dir")"
    done
fi

echo ""
echo "Done. Run 'apollia-os start' to bring the runtime back."
