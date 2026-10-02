#!/usr/bin/env bash
# Measure whether the embedded Vulkan llama-server still answers when no Vulkan
# device is usable, by falling back to the CPU variants of ggml that ship in the
# same upstream archive.
#
# Windows and Linux installers embed the Vulkan archive only
# (`packaging/fetch-llama-server.sh`, backend `vulkan`). That archive also holds
# the dynamically loaded CPU backends (`ggml-cpu-*.dll`, `libggml-cpu-*.so`),
# so a machine without a usable GPU is meant to run on them rather than on a
# second binary. This script asks that question on a real machine and answers
# it with an exit code.
#
# Usage:
#   measure-cpu-fallback.sh <runners_dir | llama-server path> <model.gguf>
#
# The first argument is what `fetch-llama-server.sh vulkan <dir>` staged, or the
# llama-server binary inside it. The model should be small: the question is
# whether one token comes back, not how fast.
#
# How Vulkan is made unavailable. ggml reads `GGML_VK_VISIBLE_DEVICES` at
# instance creation (ggml-vulkan.cpp at the pinned tag): when the variable is
# set, only the listed indices are used, so an empty value leaves the Vulkan
# backend loaded with zero devices. The script sets it to the empty string by
# default. On a machine that has no Vulkan driver or loader at all, set
# MEASURE_HIDE_VULKAN=0 to measure the machine as it is.
#
# What "answered on the CPU" means, all four required:
#   1. `llama-server --list-devices` exits 0 and lists no Vulkan device;
#   2. the server reaches /health within the timeout;
#   3. POST /completion with n_predict=1 returns tokens_predicted >= 1;
#   4. the server log (debug verbosity) reports a CPU model buffer and no
#      Vulkan buffer of any kind. The "offloaded N/N layers to GPU" line is not
#      read: it counts layers assigned to a non-CPU slot even when the only
#      device left is the CPU, so it reads as a GPU run when it is not.
#
# Exit codes:
#   0  the completion came back, computed on the CPU
#   1  it did not: the fallback failed, or a GPU was still used
#   2  no measurement happened: bad arguments, missing binary or model, no
#      Vulkan backend library next to the binary, curl missing
#
# Env:
#   MEASURE_HIDE_VULKAN  "1" (default) sets GGML_VK_VISIBLE_DEVICES to ""; "0"
#                        leaves the environment untouched
#   MEASURE_PORT         TCP port for the server (default 18099)
#   MEASURE_TIMEOUT      seconds to wait for /health and for the completion
#                        (default 120, emulated hosts are slow)
set -uo pipefail

usage() {
    echo "usage: measure-cpu-fallback.sh <runners_dir | llama-server path> <model.gguf>" >&2
    exit 2
}

case "${1:-}" in
    -h | --help)
        sed -n '2,/^set -uo pipefail$/p' "$0" | sed -e '$d' -e 's/^# \{0,1\}//'
        exit 0
        ;;
esac
[ $# -eq 2 ] || usage
TARGET="$1"
MODEL="$2"
PORT="${MEASURE_PORT:-18099}"
TIMEOUT="${MEASURE_TIMEOUT:-120}"
HIDE="${MEASURE_HIDE_VULKAN:-1}"

command -v curl >/dev/null 2>&1 || { echo "==> curl is required" >&2; exit 2; }

bin_ext=""
case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) bin_ext=".exe" ;; esac

if [ -d "$TARGET" ]; then
    SERVER="${TARGET%/}/llama-server${bin_ext}"
else
    SERVER="$TARGET"
fi
[ -f "$SERVER" ] || { echo "==> no llama-server at ${SERVER}" >&2; exit 2; }
[ -f "$MODEL" ] || { echo "==> no model at ${MODEL}" >&2; exit 2; }

# A CPU-only archive would answer green without ever having had Vulkan to lose.
# Refuse to measure it.
server_dir="$(cd "$(dirname "$SERVER")" && pwd)"
vk_lib="$(find "$server_dir" -maxdepth 1 \( -name 'ggml-vulkan.dll' -o -name 'libggml-vulkan.so*' -o -name 'libggml-vulkan.dylib' \) | head -1)"
if [ -z "$vk_lib" ]; then
    echo "==> no ggml Vulkan backend next to ${SERVER}: this is not the Vulkan archive" >&2
    exit 2
fi
cpu_libs="$(find "$server_dir" -maxdepth 1 \( -name 'ggml-cpu*.dll' -o -name 'libggml-cpu*.so*' \) | wc -l | tr -d ' ')"

if [ "$HIDE" = "1" ]; then
    export GGML_VK_VISIBLE_DEVICES=""
    hide_note="GGML_VK_VISIBLE_DEVICES set to the empty string"
else
    hide_note="environment untouched"
fi

echo "==> host: $(uname -s) $(uname -m)"
echo "==> server: ${SERVER}"
echo "==> vulkan backend: $(basename "$vk_lib"), cpu backends beside it: ${cpu_libs}"
echo "==> model: ${MODEL}"
echo "==> vulkan hidden: ${hide_note}"

failures=0
fail() {
    echo "==> FAIL: $1"
    failures=$((failures + 1))
}

echo
echo "==> llama-server --list-devices"
devices_out="$("$SERVER" --list-devices 2>&1)"
devices_rc=$?
echo "$devices_out"
echo "==> exit ${devices_rc}"
[ "$devices_rc" -eq 0 ] || fail "--list-devices exited ${devices_rc}"
if echo "$devices_out" | grep -Eq '^[[:space:]]*Vulkan[0-9]+:'; then
    fail "a Vulkan device is still listed"
fi

log="$(mktemp)"
"$SERVER" -lv 4 -m "$MODEL" -c 256 -ngl 99 --host 127.0.0.1 --port "$PORT" >"$log" 2>&1 &
server_pid=$!
cleanup() {
    kill "$server_pid" >/dev/null 2>&1 || true
    wait "$server_pid" >/dev/null 2>&1 || true
    rm -f "$log"
}
trap cleanup EXIT

echo
echo "==> waiting for /health on port ${PORT} (up to ${TIMEOUT}s)"
healthy=0
waited=0
while [ "$waited" -lt "$TIMEOUT" ]; do
    if ! kill -0 "$server_pid" >/dev/null 2>&1; then
        break
    fi
    if curl -fsS "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
        healthy=1
        break
    fi
    sleep 1
    waited=$((waited + 1))
done

response=""
if [ "$healthy" -eq 1 ]; then
    echo "==> healthy after ${waited}s"
    echo
    echo "==> POST /completion n_predict=1"
    response="$(curl -fsS --max-time "$TIMEOUT" "http://127.0.0.1:${PORT}/completion" \
        -H 'Content-Type: application/json' \
        -d '{"prompt":"Once upon a time","n_predict":1}' 2>&1)"
    echo "$response" | grep -oE '"(content|tokens_predicted)":("[^"]*"|[0-9]+)'
    predicted="$(echo "$response" | grep -oE '"tokens_predicted":[0-9]+' | head -1 | cut -d: -f2)"
    if [ -z "$predicted" ] || [ "$predicted" -lt 1 ]; then
        fail "the completion returned no token: ${response}"
    fi
else
    fail "the server never answered /health"
fi

echo
echo "==> server log, device and buffer lines"
grep -E 'common_param:   - |ggml_vulkan|buffer size' "$log" || true
if ! grep -Eq 'CPU(_Mapped|_REPACK)? model buffer size' "$log"; then
    fail "the server log reports no CPU model buffer"
fi
if grep -Eq 'Vulkan[0-9]*[^ ]* .*buffer size' "$log"; then
    fail "the server log reports a Vulkan buffer"
fi
if [ "$healthy" -ne 1 ]; then
    echo
    echo "==> server log tail"
    tail -n 40 "$log"
fi

echo
if [ "$failures" -eq 0 ]; then
    echo "==> VERDICT: the Vulkan archive answered on the CPU"
    exit 0
fi
echo "==> VERDICT: no CPU fallback (${failures} failed check(s))"
exit 1
