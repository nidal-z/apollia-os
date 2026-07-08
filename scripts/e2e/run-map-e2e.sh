#!/usr/bin/env bash
# E2E through-Apollia de ctx.llm.map : build -> install agent -> start daemon ->
# invoke -> lit le resultat -> stop. Valide le VRAI chemin (PyO3 map -> router ->
# backend par defaut = runner embarque + Ministral, ou llama-server si configure).
#
# Le SDK est auto-decouvert depuis sdk/ relatif au binaire in-tree : aucun pip
# install ni PYTHONPATH. Le premier appel LLM charge le modele (~5.6 GB) : patience.
#
# Usage : bash scripts/e2e/run-map-e2e.sh   (N=8 par defaut, surchargeable : N=16 ...)
set -u

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
BIN="$ROOT/target/debug/apollia-os"
SOCK="${APOLLIA_SOCK:-/tmp/apollia.sock}"
AGENT="$HERE/map_e2e_agent.py"
N="${N:-8}"
DAEMON_LOG="/tmp/apollia-e2e-daemon.log"

echo "[e2e] build apollia-os (daemon + CLI)"
( cd "$ROOT" && cargo build -p apollia-cli ) || { echo "[e2e] build echoue"; exit 1; }

# Le daemon spawne target/debug/apollia-runner-<backend>. On construit un runner
# RELEASE a jour (Metal) et on le place la ou le daemon debug le cherche : sinon
# il utilise un binaire perime/lent (debug) qui peut mourir au chargement modele.
echo "[e2e] build + place le runner release metal"
( cd "$ROOT" && cargo build -p apollia-runner --release --features local-metal --bin apollia-runner ) \
  || { echo "[e2e] build runner echoue"; exit 1; }
cp "$ROOT/target/release/apollia-runner" "$ROOT/target/debug/apollia-runner-metal" \
  || { echo "[e2e] placement runner echoue"; exit 1; }
# Re-sign after copy: on Apple Silicon, cp invalidates the Mach-O code signature
# and the kernel SIGKILLs the binary at exec (exit 137), which the daemon sees as
# a malformed READY line. An ad-hoc re-sign restores a valid signature.
codesign --force --sign - "$ROOT/target/debug/apollia-runner-metal" >/dev/null 2>&1 || true

echo "[e2e] install agent (valide par import ; SDK auto-decouvert)"
"$BIN" agent install "$AGENT" --skip-tests || { echo "[e2e] install echoue"; exit 1; }

echo "[e2e] start daemon en arriere-plan"
"$BIN" start --port 7771 >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!

cleanup() {
  "$BIN" stop >/dev/null 2>&1
  kill "$DAEMON_PID" 2>/dev/null
}
trap cleanup EXIT

echo "[e2e] attente readiness (socket + health)"
ready=0
for _ in $(seq 1 120); do
  if [ -S "$SOCK" ] && "$BIN" --socket "$SOCK" status >/dev/null 2>&1; then ready=1; break; fi
  if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
    echo "[e2e] le daemon s'est arrete au demarrage ; tail du log :"
    tail -25 "$DAEMON_LOG" | sed 's/^/    /'
    exit 1
  fi
  sleep 1
done
if [ "$ready" -ne 1 ]; then
  echo "[e2e] daemon non pret apres 120s ; tail du log :"
  tail -25 "$DAEMON_LOG" | sed 's/^/    /'
  exit 1
fi

echo "[e2e] invoke map-e2e (1er appel = chargement Ministral, peut prendre des dizaines de s)"
# L'auto-load des agents se termine juste apres que /health reponde : un "not
# found" est une course transitoire. On retente ; une fois l'agent enregistre,
# `run` bloque (chargement modele + inference) puis rend le resultat.
rc=1
for attempt in $(seq 1 30); do
  out="$("$BIN" run map-e2e "$N" 2>&1)"
  rc=$?
  if [ "$rc" -eq 0 ] || ! printf '%s' "$out" | grep -q "not found"; then
    printf '%s\n' "$out"
    break
  fi
  sleep 1
done
echo "[e2e] termine (exit $rc)"
exit "$rc"
