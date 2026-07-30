#!/usr/bin/env bash
# E2E through-Apollia de ctx.llm.map, route llama-server (backend OpenAI-compatible).
#
# Complement de run-map-e2e.sh (runner embarque) : valide le MEME chemin Apollia
# (dispatch -> ctx.llm.map -> LlmRouter -> backend), mais avec le backend par
# defaut pointe sur un llama-server local en continuous batching. C'est la route
# 1c : le gain map vs sequentiel reflete alors le VRAI batching serveur, pas les
# slots du runner embarque.
#
# Le script est reversible : il sauvegarde le backend par defaut courant, cree un
# backend `local-fast` temporaire pointe sur llama-server, invoque l'agent, puis
# restaure le defaut et supprime `local-fast`. Il arrete le daemon et llama-server
# en sortie.
#
# Usage : bash scripts/e2e/run-map-e2e-server.sh   (N=8 par defaut : N=16 ...)
set -u

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
BIN="$ROOT/target/debug/apollia-os"
SOCK="${APOLLIA_SOCK:-/tmp/apollia.sock}"
AGENT="$HERE/map_e2e_agent.py"
N="${N:-8}"
# No default: GGUF weights are not shipped with the repository, so any baked-in
# path would only ever resolve on the machine that recorded it.
MODEL="${MODEL:?set MODEL=/absolute/path/to/model.gguf}"
PORT="${LLAMA_PORT:-8080}"
BASE_URL="http://127.0.0.1:${PORT}/v1"
DAEMON_LOG="/tmp/apollia-e2e-srv-daemon.log"
LLAMA_LOG="/tmp/apollia-e2e-llama-server.log"
LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server)}"

SAVED_DEFAULT=""
LLAMA_PID=""
DAEMON_PID=""

cleanup() {
  # Restaure le backend par defaut d'origine et supprime le backend temporaire.
  if [ -n "$SAVED_DEFAULT" ]; then
    "$BIN" llm backends set-default "$SAVED_DEFAULT" >/dev/null 2>&1
  fi
  "$BIN" llm backends delete local-fast --confirm >/dev/null 2>&1
  "$BIN" stop >/dev/null 2>&1
  [ -n "$DAEMON_PID" ] && kill "$DAEMON_PID" 2>/dev/null
  [ -n "$LLAMA_PID" ] && kill "$LLAMA_PID" 2>/dev/null
}
trap cleanup EXIT

wait_daemon() {
  for _ in $(seq 1 120); do
    if [ -S "$SOCK" ] && "$BIN" --socket "$SOCK" status >/dev/null 2>&1; then return 0; fi
    if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
      echo "[e2e] daemon arrete au demarrage ; tail :"; tail -20 "$DAEMON_LOG" | sed 's/^/    /'; return 1
    fi
    sleep 1
  done
  echo "[e2e] daemon non pret apres 120s"; return 1
}

echo "[e2e] build apollia-os (daemon + CLI)"
( cd "$ROOT" && cargo build -p apollia-cli ) || { echo "[e2e] build echoue"; exit 1; }

if [ -z "$LLAMA_BIN" ] || [ ! -x "$LLAMA_BIN" ]; then
  echo "[e2e] llama-server introuvable (LLAMA_BIN)"; exit 1
fi

echo "[e2e] start llama-server (continuous batching, np=8, flash-attn)"
"$LLAMA_BIN" -m "$MODEL" -ngl 999 -c 16384 -np 8 -cb --flash-attn on \
  --host 127.0.0.1 --port "$PORT" >"$LLAMA_LOG" 2>&1 &
LLAMA_PID=$!

echo "[e2e] attente readiness llama-server"
srv_ready=0
for _ in $(seq 1 180); do
  if curl -sf "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then srv_ready=1; break; fi
  if ! kill -0 "$LLAMA_PID" 2>/dev/null; then
    echo "[e2e] llama-server arrete ; tail :"; tail -20 "$LLAMA_LOG" | sed 's/^/    /'; exit 1
  fi
  sleep 1
done
[ "$srv_ready" -eq 1 ] || { echo "[e2e] llama-server non pret"; exit 1; }

echo "[e2e] start daemon (config du backend)"
"$BIN" start --port 7771 >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!
wait_daemon || exit 1

echo "[e2e] sauvegarde du backend par defaut courant"
SAVED_DEFAULT="$("$BIN" llm backends list --json 2>/dev/null \
  | python3 -c 'import sys,json
try:
    data=json.load(sys.stdin)
except Exception:
    print(""); sys.exit()
items=data.get("backends", data) if isinstance(data, dict) else data
for b in (items or []):
    if isinstance(b, dict) and (b.get("is_default") or b.get("default")):
        print(b.get("name","")); break')"
echo "[e2e] backend par defaut d'origine = '${SAVED_DEFAULT:-<aucun>}'"

echo "[e2e] cree backend local-fast -> llama-server, le passe par defaut, installe l'agent"
"$BIN" llm backends create local-fast --provider openai --model ministral-local \
  --base-url "$BASE_URL" >/dev/null 2>&1
"$BIN" llm backends set-default local-fast >/dev/null 2>&1 || { echo "[e2e] set-default echoue"; exit 1; }
"$BIN" agent install "$AGENT" --skip-tests >/dev/null 2>&1 || { echo "[e2e] install echoue"; exit 1; }

# Redemarre le daemon pour qu'il boote avec local-fast comme defaut (le router
# lit le backend par defaut au boot).
echo "[e2e] restart daemon (boot avec local-fast par defaut)"
"$BIN" stop >/dev/null 2>&1
kill "$DAEMON_PID" 2>/dev/null
sleep 1
"$BIN" start --port 7771 >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!
wait_daemon || exit 1

echo "[e2e] invoke map-e2e (route llama-server)"
rc=1
for _ in $(seq 1 30); do
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
