#!/usr/bin/env bash
# The decisive set behind 1.12 finding 1: when does a ReAct continuation stop
# reusing its prefix, and what does the engine say it is doing when it stops.
#
# One llama-server per condition, because a launch flag is the thing under test
# and history is the thing being controlled. Each server runs at -lv 5, which is
# where the engine prints the checkpoint decisions ("main/do_checkpoint",
# "restored context checkpoint", "forcing full prompt re-processing"). The log is
# kept so every record can be read against what the engine itself reported.
#
# Conditions vary one factor each against the product's own launch vector:
#   l1-control      pair alone on a fresh server
#   l2a-pre-short   one 64 token request before the pair
#   l2b-pre-long    one 16384 token request before the pair
#   l3-cms0         --checkpoint-min-step 0, otherwise l2a
#   l4-ctxcp0       --ctx-checkpoints 0, otherwise l1
#   l5-ubatch256    -ub 256, otherwise l1
#   l5a/l5b/l5c     -ub 128, 1024, 2048: the micro-batch line, four levels wide,
#                   because the recompute a warm continuation pays is a function
#                   of n_ubatch and one point cannot show a function
#   l6-dense        the dense model, otherwise l2a
#   l7-curve-preamble   the campaign's whole prefill curve before the pair
#   l7b-curve-cms0      --checkpoint-min-step 0, otherwise l7
#   l7c-dense-curve     the dense model, otherwise l7
#   l8-campaign-replay  the campaign's own probes replayed before the pair
#   l8b-campaign-cms0   --checkpoint-min-step 0, otherwise l8
#   l9-trap             one request of exactly 4 + n_ubatch + 1 tokens, then the pair
#   l9b-trap-cms0       --checkpoint-min-step 0, otherwise l9
#
# The trap has no dense counterpart: Ministral's chat template alone costs 535
# tokens, so a 517 token prompt cannot be built on it, and the probe says so
# rather than measuring a different length. l6 is the dense control, and it
# needs no trap, since that model creates no checkpoints at all.
#
# Usage: prefix-collapse.sh [condition ...]     (default: all of them)
# Env:  PORT (def 8092, 8090 belongs to the flag sweep), CTX (def 32768),
#       REPS (def 5), MODELS_DIR, LLAMA_BIN, OUT_DIR (def ./results).
set -u

HERE="$(cd "$(dirname "$0")" && pwd)"
PORT="${PORT:-8092}"
CTX="${CTX:-32768}"
REPS="${REPS:-5}"
MODELS_DIR="${MODELS_DIR:-$HOME/.apollia_old/models}"
LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server)}"
OUT_DIR="${OUT_DIR:-$HERE/results}"; mkdir -p "$OUT_DIR"
BASE_URL="http://127.0.0.1:${PORT}/v1"

HYBRID="$MODELS_DIR/Qwen3.6-35B-A3B-MXFP4_MOE.gguf"
DENSE="$MODELS_DIR/Ministral-3-8B-Instruct-2512-Q5_K_M.gguf"

# The curve the campaign ran immediately before the pair, which is the history
# under suspicion. One intervening request does not reproduce the collapse, as
# l2a and l2b showed, so the preamble is replayed whole.
CURVE="pre:512:cold,pre:1024:cold,pre:2048:cold,pre:4096:cold,pre:8192:cold,pre:16384:cold"

# condition | model | label | extra launch flags | preamble | sequence
CONDITIONS=(
  "l1-control|$HYBRID|qwen3.6-35b-a3b|||pair"
  "l2a-pre-short|$HYBRID|qwen3.6-35b-a3b|||pre:64:cold,pair"
  "l2b-pre-long|$HYBRID|qwen3.6-35b-a3b|||pre:16384:cold,pair"
  "l3-cms0|$HYBRID|qwen3.6-35b-a3b|-cms 0||pre:64:cold,pair"
  "l4-ctxcp0|$HYBRID|qwen3.6-35b-a3b|-ctxcp 0||pair"
  "l5-ubatch256|$HYBRID|qwen3.6-35b-a3b|-ub 256||pair"
  "l5a-ubatch128|$HYBRID|qwen3.6-35b-a3b|-ub 128||pair"
  "l5b-ubatch1024|$HYBRID|qwen3.6-35b-a3b|-ub 1024||pair"
  "l5c-ubatch2048|$HYBRID|qwen3.6-35b-a3b|-ub 2048||pair"
  "l6-dense|$DENSE|ministral-3-8b|||pre:64:cold,pair"
  "l7-curve-preamble|$HYBRID|qwen3.6-35b-a3b||$CURVE|pair"
  "l7b-curve-cms0|$HYBRID|qwen3.6-35b-a3b|-cms 0|$CURVE|pair"
  "l7c-dense-curve|$DENSE|ministral-3-8b||$CURVE|pair"
  "l8-campaign-replay|$HYBRID|qwen3.6-35b-a3b||campaign|pair"
  "l8b-campaign-cms0|$HYBRID|qwen3.6-35b-a3b|-cms 0|campaign|pair"
  "l9-trap|$HYBRID|qwen3.6-35b-a3b|||trap,pair"
  "l9b-trap-cms0|$HYBRID|qwen3.6-35b-a3b|-cms 0||trap,pair"
)

ARGC=$#
WANTED="$*"
selected() {
  [ "$ARGC" -eq 0 ] && return 0
  case " $WANTED " in *" $1 "*) return 0 ;; esac
  return 1
}

LPID=""
cleanup(){ [ -n "$LPID" ] && kill "$LPID" 2>/dev/null; pkill -f "llama-server.*--port ${PORT}" 2>/dev/null; }
trap cleanup EXIT

run_condition() {
  local condition="$1" gguf="$2" label="$3" extra="$4" preamble="$5" sequence="$6"
  local slog="/tmp/prefix-collapse-${condition}.log"

  if [ ! -f "$gguf" ]; then echo "[collapse] MISSING gguf: $gguf"; return 1; fi
  echo "[collapse] === $condition === (${label}, extra: ${extra:-none}, preamble: ${preamble:-none}, sequence: $sequence)"

  # The product's own vector, from llama_server/config.rs build_args, plus the
  # condition's single delta. Recorded verbatim in every record's provenance.
  local launch=(-m "$gguf" -ngl 999 -c "$CTX" -np 1 -cb --flash-attn on --jinja
                --reasoning-format none --host 127.0.0.1 --port "$PORT" -lv 5)
  # The trap step is defined relative to n_ubatch, so the condition's own value
  # is passed through rather than guessed at by the probe.
  local n_ubatch=512
  case "$extra" in *"-ub "*) n_ubatch="${extra##*-ub }"; n_ubatch="${n_ubatch%% *}" ;; esac
  if [ -n "$extra" ]; then
    # shellcheck disable=SC2206
    local delta=($extra)
    launch=("${launch[@]}" "${delta[@]}")
  fi

  "$LLAMA_BIN" "${launch[@]}" > "$slog" 2>&1 &
  LPID=$!

  local ready=0
  for _ in $(seq 1 300); do
    if curl -sf "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then ready=1; break; fi
    kill -0 $LPID 2>/dev/null || { echo "[collapse] llama-server died at load:"; tail -15 "$slog" | sed 's/^/    /'; break; }
    sleep 1
  done
  if [ "$ready" -ne 1 ]; then echo "[collapse] server not ready, skipping $condition"; cleanup; LPID=""; return 1; fi

  local launch_args
  launch_args="$(printf '%s\n' "$LLAMA_BIN" "${launch[@]}" | python3 -c 'import json,sys; print(json.dumps([l.rstrip("\n") for l in sys.stdin]))')"

  # `campaign` replays the two probes eval-model.sh runs before cache_reuse,
  # with their own parameters, rather than an approximation of them. A synthetic
  # preamble that merely resembles the campaign cannot settle whether the
  # campaign's own history is what collapsed the reuse.
  if [ "$preamble" = "campaign" ]; then
    echo "[collapse]   replaying the campaign preamble: speed probe, then prefill curve"
    BASE_URL="$BASE_URL" MODEL="$label" LABEL="$label" MAX_TOKENS=200 REPS="$REPS" \
    MODEL_PATH="$gguf" LLAMA_BIN="$LLAMA_BIN" LAUNCH_ARGS="$launch_args" N_CTX="$CTX" \
      python3 "$HERE/speed_probe.py" > /dev/null 2>/dev/null
    BASE_URL="$BASE_URL" MODEL="$label" LABEL="$label" MAX_TOKENS=8 REPS="$REPS" \
    MODEL_PATH="$gguf" LLAMA_BIN="$LLAMA_BIN" LAUNCH_ARGS="$launch_args" N_CTX="$CTX" \
      python3 "$HERE/prefill_curve_probe.py" > /dev/null 2>/dev/null
    preamble=""
  fi

  BASE_URL="$BASE_URL" MODEL="$label" LABEL="$label" CONDITION="$condition" \
  SEQUENCE="$sequence" PREAMBLE="$preamble" PREAMBLE_REPS="${PREAMBLE_REPS:-5}" \
  N_UBATCH="$n_ubatch" REPS="$REPS" MODEL_PATH="$gguf" LLAMA_BIN="$LLAMA_BIN" \
  LAUNCH_ARGS="$launch_args" N_CTX="$CTX" CAMPAIGN_ID="prefix-collapse-$condition" \
  OUT="$OUT_DIR/prefix-collapse-$condition.json" \
    python3 "$HERE/prefix_collapse_probe.py" > /dev/null
  local status=$?

  cleanup; LPID=""; sleep 2

  if [ $status -ne 0 ]; then echo "[collapse] probe failed for $condition"; return 1; fi

  # What the engine says it did, next to what the records say happened.
  echo "[collapse]   engine log $slog:"
  printf '    %-46s %s\n' \
    "restored context checkpoint"      "$(grep -c 'restored context checkpoint'   "$slog")" \
    "forcing full prompt re-processing" "$(grep -c 'forcing full prompt re-proces' "$slog")" \
    "created context checkpoint"       "$(grep -c 'created context checkpoint'    "$slog")" \
    "main/do_checkpoint = yes"         "$(grep -c 'main/do_checkpoint = yes'      "$slog")" \
    "main/do_checkpoint = no"          "$(grep -c 'main/do_checkpoint = no'       "$slog")"
  return 0
}

for row in "${CONDITIONS[@]}"; do
  IFS='|' read -r condition gguf label extra preamble sequence <<< "$row"
  selected "$condition" || continue
  run_condition "$condition" "$gguf" "$label" "$extra" "$preamble" "$sequence"
done

echo "[collapse] campaigns in $OUT_DIR/prefix-collapse-*.json"
