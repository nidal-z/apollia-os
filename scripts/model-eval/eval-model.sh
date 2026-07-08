#!/usr/bin/env bash
# Evaluate ONE model via a local llama-server: speed + tool-calling + ESRS.
# Arch-agnostic (upstream llama.cpp loads mistral4/qwen3moe/llama4/...), so the
# whole 2026 shortlist is testable without touching the embedded engine.
#
# Usage: eval-model.sh <label> <gguf-first-shard-path>
# Env:  NP (parallel slots, def 8), CTX (context, def 16384), PORT (def 8090),
#       LLAMA_BIN (def: which llama-server), MAX_TOKENS (def 200).
set -u

HERE="$(cd "$(dirname "$0")" && pwd)"
LABEL="${1:?label}"; GGUF="${2:?gguf path}"
NP="${NP:-8}"; CTX="${CTX:-16384}"; PORT="${PORT:-8090}"; MAX_TOKENS="${MAX_TOKENS:-200}"
LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server)}"
OUT_DIR="${OUT_DIR:-$HERE/results}"; mkdir -p "$OUT_DIR"
BASE_URL="http://127.0.0.1:${PORT}/v1"
SLOG="/tmp/model-eval-llama-${LABEL}.log"

echo "[eval] === $LABEL ==="
if [ ! -f "$GGUF" ]; then echo "[eval] MISSING gguf: $GGUF"; echo "{\"label\":\"$LABEL\",\"error\":\"gguf missing\"}" > "$OUT_DIR/$LABEL.json"; exit 0; fi

LPID=""
cleanup(){ [ -n "$LPID" ] && kill "$LPID" 2>/dev/null; pkill -f "llama-server.*--port ${PORT}" 2>/dev/null; }
trap cleanup EXIT

echo "[eval] start llama-server (np=$NP ctx=$CTX) : $(basename "$GGUF")"
"$LLAMA_BIN" -m "$GGUF" -ngl 999 -c "$CTX" -np "$NP" -cb --flash-attn on --jinja \
  --host 127.0.0.1 --port "$PORT" > "$SLOG" 2>&1 &
LPID=$!

ready=0
for _ in $(seq 1 300); do
  if curl -sf "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then ready=1; break; fi
  kill -0 $LPID 2>/dev/null || { echo "[eval] llama-server died at load:"; tail -15 "$SLOG" | sed 's/^/    /'; break; }
  sleep 1
done
if [ "$ready" -ne 1 ]; then
  echo "{\"label\":\"$LABEL\",\"error\":\"server not ready\",\"gguf\":\"$GGUF\"}" > "$OUT_DIR/$LABEL.json"; exit 0
fi

export BASE_URL MODEL="$LABEL" MAX_TOKENS
echo "[eval] speed probe (sanity)"
SPEED="$(python3 "$HERE/speed_probe.py")"; echo "    $SPEED"
EMPTY="$(printf '%s' "$SPEED" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("empty",True))' 2>/dev/null || echo True)"

TOOL='{"skipped":"unstable"}'; ESRS='{"skipped":"unstable"}'; BATCH='{"skipped":"unstable"}'
if [ "$EMPTY" = "False" ]; then
  echo "[eval] tool-calling probe"; TOOL="$(python3 "$HERE/toolcall_probe.py")"; echo "    $TOOL"
  echo "[eval] ESRS probe"; ESRS="$(python3 "$HERE/esrs_probe.py")"; echo "    $ESRS"
  echo "[eval] batch throughput probe (map vs sequential)"
  BATCH="$(N="${BATCH_N:-16}" CONCURRENCY="$NP" MAX_TOKENS=128 python3 "$HERE/../validate-llm-map.py" 2>/dev/null \
    | python3 -c 'import sys,re;t=sys.stdin.read()
def g(p):
  m=re.search(p,t); return float(m.group(1)) if m else None
import json;print(json.dumps({"seq_tps":g(r"sequentiel\].*agg_tps=([0-9.]+)"),"map_tps":g(r"map\s*\].*agg_tps=([0-9.]+)"),"speedup":g(r"speedup.*=\s*([0-9.]+)x")}))' 2>/dev/null || echo '{"error":"batch failed"}')"
  echo "    $BATCH"
else
  echo "[eval] SANITY FAIL (empty/degenerate output) - skipping quality probes"
fi

python3 - "$OUT_DIR/$LABEL.json" "$LABEL" "$GGUF" "$SPEED" "$TOOL" "$ESRS" "$BATCH" <<'PY'
import json,sys
out,label,gguf,speed,tool,esrs,batch=sys.argv[1:8]
def j(s):
    try: return json.loads(s)
    except Exception: return {"raw":s[:200]}
json.dump({"label":label,"gguf":gguf,"speed":j(speed),"toolcall":j(tool),"esrs":j(esrs),"batch":j(batch)},
          open(out,"w"),ensure_ascii=False,indent=1)
print("[eval] wrote",out)
PY
echo "[eval] done $LABEL"
