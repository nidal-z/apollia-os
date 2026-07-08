#!/usr/bin/env bash
# Evaluate the whole shortlist, one model at a time (sequential = memory-safe),
# via a local llama-server. Produces results/<label>.json per model then prints a
# comparison matrix. Skips models whose GGUF is not present yet (still downloading).
#
# Usage: bash scripts/model-eval/run-matrix.sh [label ...]   (no args = all)
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
MB=/Users/nidalzoumita/.apollia/models

# label | gguf glob (first shard) | NP | CTX
MODELS=(
  "ministral-3-8b|$MB/Ministral-3-8B-Instruct-2512-Q5_K_M.gguf|8|16384"
  "qwen3-30b-a3b|$MB/Qwen3-30B-A3B.Q6_K.gguf|8|16384"
  "mistral-small-3.2-24b|$MB/mistral-small-3.2-24b/*Q4_K_M.gguf|8|16384"
  "qwen3.6-35b-a3b|$MB/qwen3.6-35b-a3b/*MXFP4_MOE*.gguf|8|16384"
  "mistral-small-4-119b|$MB/mistral-small-4-119b/MXFP4_MOE/*00001-of-*.gguf|4|8192"
  "qwen3-235b-a22b|$MB/qwen3-235b-a22b/UD-Q4_K_XL/*00001-of-*.gguf|2|8192"
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
