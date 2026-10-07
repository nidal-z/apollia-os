#!/usr/bin/env bash
# Master demo: a host application drives a real Apollia daemon end to end through
# the generated Python SDK, over TCP + bearer token, with no model.
#
# Steps: build the daemon, build the echo package (one workflow, no agent),
# start the daemon, install the package, restart the daemon (it reads the
# workflows of the installed packages when it starts), then run demo_driver.py,
# which launches the workflow and reads its result through the generated
# client. Tears the daemon down on exit.
#
# The demo starts its own daemon and stops it. It refuses to run while another
# runtime answers for this user, so stop that one first.
#
# Usage: bash clients/examples/demo_python.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$ROOT/target/debug/apollia-os"
PACKAGE="$ROOT/clients/examples/echo"
PORT="${APOLLIA_TCP_PORT:-7771}"
VENV="$ROOT/clients/.venv"
WORK="$(mktemp -d -t apollia-demo.XXXXXX)"
ARCHIVE="$WORK/echo-0.1.0.apo"
DAEMON_LOG="$WORK/daemon.log"
DAEMON_PID=""

cleanup() {
  if [ -n "$DAEMON_PID" ]; then
    "$BIN" stop >/dev/null 2>&1 || true
    kill "$DAEMON_PID" 2>/dev/null || true
  fi
  rm -rf "$WORK"
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

echo "== build the echo package =="
"$BIN" package build "$PACKAGE" --output "$ARCHIVE"

start_daemon() {
  "$BIN" start --port "$PORT" >"$DAEMON_LOG" 2>&1 &
  DAEMON_PID=$!
  for _ in $(seq 1 120); do
    if "$BIN" status >/dev/null 2>&1; then return 0; fi
    kill -0 "$DAEMON_PID" 2>/dev/null || { echo "daemon died" >&2; tail -20 "$DAEMON_LOG" >&2; exit 1; }
    sleep 1
  done
  echo "daemon did not answer in 120 s" >&2
  tail -20 "$DAEMON_LOG" >&2
  exit 1
}

stop_daemon() {
  "$BIN" stop >/dev/null
  wait "$DAEMON_PID" 2>/dev/null || true
  DAEMON_PID=""
}

echo "== start daemon on 127.0.0.1:${PORT} (TCP token auth) =="
if "$BIN" status >/dev/null 2>&1; then
  echo "a runtime is already running; stop it with 'apollia-os stop' and run the demo again" >&2
  exit 1
fi
start_daemon

echo "== install the echo package =="
"$BIN" package install "$ARCHIVE"

echo "== restart the daemon so it reads the installed workflow =="
stop_daemon
start_daemon

echo "== drive the daemon through the generated SDK =="
APOLLIA_TCP_PORT="$PORT" "$VENV/bin/python" "$ROOT/clients/examples/demo_driver.py"
