#!/usr/bin/env bash
# Evaluate ONE model via a local llama-server: speed, prefill curve, cache
# reuse, tool-calling, ESRS, batch throughput.
#
# Launches with the product's own argument vector by default, not the old
# -np 8 -c 16384 pair. Throughput measured under a different launch vector is
# not comparable to what a user experiences, which is why LAUNCH_ARGS is
# recorded in every record's provenance rather than assumed.
#
# Usage: eval-model.sh <label> <gguf-first-shard-path>
# Env:  NP (slots, def 1), CTX (context, def 32768), PORT (def 8090),
#       REPS (def 5), LLAMA_BIN (def: which llama-server), MAX_TOKENS (def 200),
#       LENGTHS (prefill curve targets), OUT_DIR (def ./results).
set -u

HERE="$(cd "$(dirname "$0")" && pwd)"
LABEL="${1:?label}"; GGUF="${2:?gguf path}"
NP="${NP:-1}"; CTX="${CTX:-32768}"; PORT="${PORT:-8090}"; MAX_TOKENS="${MAX_TOKENS:-200}"
REPS="${REPS:-5}"
LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server)}"
OUT_DIR="${OUT_DIR:-$HERE/results}"; mkdir -p "$OUT_DIR"
BASE_URL="http://127.0.0.1:${PORT}/v1"
SLOG="/tmp/model-eval-llama-${LABEL}.log"
CAMPAIGN_ID="${CAMPAIGN_ID:-$LABEL-$(date -u +%Y%m%dT%H%M%SZ)}"

echo "[eval] === $LABEL ==="
if [ ! -f "$GGUF" ]; then echo "[eval] MISSING gguf: $GGUF"; exit 0; fi

LPID=""
cleanup(){ [ -n "$LPID" ] && kill "$LPID" 2>/dev/null; pkill -f "llama-server.*--port ${PORT}" 2>/dev/null; }
trap cleanup EXIT

# The frozen baseline vector, plus host and port. Recorded verbatim below.
LAUNCH=(-m "$GGUF" -ngl 999 -c "$CTX" -np "$NP" -cb --flash-attn on --jinja
        --reasoning-format none --host 127.0.0.1 --port "$PORT")
echo "[eval] start llama-server (np=$NP ctx=$CTX) : $(basename "$GGUF")"
"$LLAMA_BIN" "${LAUNCH[@]}" > "$SLOG" 2>&1 &
LPID=$!

ready=0
for _ in $(seq 1 300); do
  if curl -sf "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then ready=1; break; fi
  kill -0 $LPID 2>/dev/null || { echo "[eval] llama-server died at load:"; tail -15 "$SLOG" | sed 's/^/    /'; break; }
  sleep 1
done
if [ "$ready" -ne 1 ]; then echo "[eval] server not ready, aborting $LABEL"; exit 1; fi

LAUNCH_ARGS="$(printf '%s\n' "$LLAMA_BIN" "${LAUNCH[@]}" | python3 -c 'import json,sys; print(json.dumps([l.rstrip("\n") for l in sys.stdin]))')"
export BASE_URL MODEL="$LABEL" LABEL MAX_TOKENS REPS CAMPAIGN_ID LAUNCH_ARGS
export MODEL_PATH="$GGUF" LLAMA_BIN N_CTX="$CTX"

# The cache reuse probe runs FIRST, and the order is load-bearing rather than a
# preference. 1.12 finding 1: a single request of exactly `4 + n_ubatch + 1`
# tokens leaves a context checkpoint the engine's invalidation loop can never
# erase, and every continuation for the rest of the process then recomputes its
# whole prompt. The prefill curve issues such a request at its 512-token point
# under the default `-ub 512`. A continuation measured after it is measuring
# whether the trap fired, not how much the engine reuses: on this model that is
# 1 cached token of 4214 instead of 3626, and nothing in the record looks wrong.
# `sweep.py` has enforced this order since finding 1 landed; this script had not.
#
# MAX_TOKENS is pinned here rather than inherited. The exported 200 is the speed
# probe's decode budget, and the continuation figures in 1.12 finding 1 were
# measured at 32. The hit ratio does not move with it; comparability does.
echo "[eval] cache reuse probe (first, before anything can poison the slot)"
OUT="$OUT_DIR/$CAMPAIGN_ID-cache-reuse.json" MAX_TOKENS=32 \
  python3 "$HERE/cache_reuse_probe.py" > /dev/null

echo "[eval] speed probe (cold + warm, $REPS reps each)"
OUT="$OUT_DIR/$CAMPAIGN_ID-speed.json" python3 "$HERE/speed_probe.py" > /dev/null

echo "[eval] prefill curve"
OUT="$OUT_DIR/$CAMPAIGN_ID-prefill-curve.json" python3 "$HERE/prefill_curve_probe.py" > /dev/null

# Quality probes: scoring logic untouched, output shape unchanged. They are
# scored rather than timed, so neither defect 1 nor defect 3 applies to them.
echo "[eval] tool-calling probe"
TOOL="$(python3 "$HERE/toolcall_probe.py")"; echo "    $TOOL"
echo "[eval] ESRS probe"
ESRS="$(python3 "$HERE/esrs_probe.py")"; echo "    $ESRS"
echo "[eval] batch throughput probe (map vs sequential)"
BATCH="$(N="${BATCH_N:-16}" CONCURRENCY="$NP" MAX_TOKENS=128 python3 "$HERE/../validate-llm-map.py" 2>/dev/null \
  | python3 -c 'import sys,re;t=sys.stdin.read()
def g(p):
  m=re.search(p,t); return float(m.group(1)) if m else None
import json;print(json.dumps({"seq_tps":g(r"sequentiel\].*agg_tps=([0-9.]+)"),"map_tps":g(r"map\s*\].*agg_tps=([0-9.]+)"),"speedup":g(r"speedup.*=\s*([0-9.]+)x")}))' 2>/dev/null || echo '{"error":"batch failed"}')"
echo "    $BATCH"

QUALITY_OUT="$OUT_DIR/$CAMPAIGN_ID-quality.json" \
  python3 "$HERE/quality_record.py" "$LABEL" "$TOOL" "$ESRS" "$BATCH"

echo "[eval] merging campaign"
python3 "$HERE/merge_campaign.py" "$OUT_DIR/$CAMPAIGN_ID.json" \
  "$OUT_DIR/$CAMPAIGN_ID-cache-reuse.json" \
  "$OUT_DIR/$CAMPAIGN_ID-speed.json" \
  "$OUT_DIR/$CAMPAIGN_ID-prefill-curve.json" \
  "$OUT_DIR/$CAMPAIGN_ID-quality.json"
echo "[eval] done $LABEL -> $OUT_DIR/$CAMPAIGN_ID.json"
