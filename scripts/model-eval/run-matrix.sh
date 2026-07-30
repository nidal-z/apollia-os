#!/usr/bin/env bash
# Evaluate the whole shortlist, one model at a time (sequential = memory-safe),
# via a local llama-server. Produces results/<label>.json per model then prints a
# comparison matrix. Skips models whose GGUF is not present yet (still downloading).
#
# Usage: bash scripts/model-eval/run-matrix.sh [label ...]   (no args = all)
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
MB="${APOLLIA_MODELS_DIR:-$HOME/.apollia_old/models}"

# label | gguf glob (first shard) | NP | CTX
#
# NP and CTX are the product's own values, not the -np 8 -c 16384 pair the
# pre-contract harness used. A throughput figure measured under a different
# launch vector does not describe what a user experiences, and the two are not
# comparable: that is why launch_args is recorded in every record.
#
# The four rows whose GGUF is gone stay in place. They self-skip, and they
# document the shortlist this campaign was designed around.
MODELS=(
  "ministral-3-8b|$MB/Ministral-3-8B-Instruct-2512-Q5_K_M.gguf|1|32768"
  "qwen3-30b-a3b|$MB/Qwen3-30B-A3B.Q6_K.gguf|1|32768"
  "mistral-small-3.2-24b|$MB/mistral-small-3.2-24b/*Q4_K_M.gguf|1|32768"
  "qwen3.6-35b-a3b|$MB/*Qwen3.6-35B-A3B*MXFP4_MOE*.gguf|1|32768"
  "mistral-small-4-119b|$MB/mistral-small-4-119b/MXFP4_MOE/*00001-of-*.gguf|1|32768"
  "qwen3-235b-a22b|$MB/qwen3-235b-a22b/UD-Q4_K_XL/*00001-of-*.gguf|1|32768"
)
WANT=("$@")

want(){ [ ${#WANT[@]} -eq 0 ] && return 0; for w in "${WANT[@]}"; do [ "$w" = "$1" ] && return 0; done; return 1; }

for row in "${MODELS[@]}"; do
  IFS='|' read -r label glob np ctx <<< "$row"
  want "$label" || continue
  gguf="$(ls -1 $glob 2>/dev/null | sort | head -1)"
  if [ -z "$gguf" ]; then echo "[matrix] SKIP $label (not downloaded yet: $glob)"; continue; fi
  NP="$np" CTX="$ctx" PORT=8090 bash "$HERE/eval-model.sh" "$label" "$gguf"
  pkill -f "llama-server.*--port 8090" 2>/dev/null; sleep 2
done

echo; echo "================ COMPARISON MATRIX ================"
python3 "$HERE/aggregate.py"
