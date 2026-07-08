#!/usr/bin/env bash
# Master demo: a host application drives a real Apollia daemon end to end through
# the generated Python SDK, over TCP + bearer token.
#
# Steps: build the daemon, install the no-LLM echo agent, start the daemon
# (which auto-loads the agent), then run demo_driver.py which submits a task and
# streams the result through the generated client. Tears the daemon down on exit.
#
# Usage: bash clients/examples/demo_python.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$ROOT/target/debug/apollia-os"
SOCK="/tmp/apollia.sock"
PORT="${APOLLIA_TCP_PORT:-7771}"
VENV="$ROOT/clients/.venv"
DAEMON_LOG="$(mktemp -t apollia-demo.XXXXXX)"
DAEMON_PID=""

cleanup() {
  [ -n "$DAEMON_PID" ] && kill "$DAEMON_PID" 2>/dev/null || true
  "$BIN" stop >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "== build daemon =="
[ -x "$BIN" ] || cargo build -p apollia-cli

echo "== ensure generated Python client is present =="
if [ ! -d "$ROOT/clients/python/apollia_runtime_client" ]; then
  echo "generated client missing; run 'bash clients/regen.sh --from-daemon' first" >&2
  exit 1
fi

echo "== python venv + client runtime deps =="
[ -d "$VENV" ] || python3 -m venv "$VENV"
"$VENV/bin/python" -c "import httpx, attr, dateutil" 2>/dev/null || \
  "$VENV/bin/pip" install --quiet httpx attrs python-dateutil

echo "== install echo agent =="
"$BIN" agent install "$ROOT/clients/examples/echo_agent.py" --skip-tests

echo "== start daemon on 127.0.0.1:${PORT} (TCP token auth) + ${SOCK} =="
"$BIN" stop >/dev/null 2>&1 || true
rm -f "$SOCK"
"$BIN" start --port "$PORT" >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!
for _ in $(seq 1 120); do
  if [ -S "$SOCK" ] && "$BIN" --socket "$SOCK" status >/dev/null 2>&1; then break; fi
  kill -0 "$DAEMON_PID" 2>/dev/null || { echo "daemon died"; tail -20 "$DAEMON_LOG"; exit 1; }
  sleep 1
done

echo "== drive the daemon through the generated SDK =="
"$VENV/bin/python" "$ROOT/clients/examples/demo_driver.py"
