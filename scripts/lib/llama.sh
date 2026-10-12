# shellcheck shell=bash
# llama-server helpers for the dev and automation recipes. Sourced by the
# justfile, which hands its settings over as LLAMA_* shell variables (see the
# head of the justfile and .env.example for the environment variable behind
# each one).

# The one message every recipe that needs a model answers with. The second
# argument names the environment variable that recipe reads.
llama_require_model() {
    local model="${1:-}" variable="${2:-APOLLIA_LLAMA_MODEL}" size
    if [ -z "$model" ] || [ ! -f "$model" ]; then
        echo "error: this recipe needs a GGUF model and found none at '${model}'." >&2
        echo "       Pass the path as the recipe argument, or set $variable" >&2
        echo "       in your environment or in .env (see .env.example)." >&2
        return 1
    fi
    # A GGUF under a megabyte is a placeholder, not a model: the automation
    # seed installs stubs of a few kilobytes under models/, and llama-server
    # dies on them with `unknown model architecture: ''`, which describes a
    # corrupt model rather than a swapped profile.
    size="$(($(wc -c < "$model")))"
    if [ "$size" -lt 1000000 ]; then
        echo "error: $model is only $size bytes, which is a placeholder and not a model." >&2
        if [ -d "$HOME/.apollia.before-seed" ]; then
            echo "       The automation seed is loaded: your real profile, and the real" >&2
            echo "       model with it, is at ~/.apollia.before-seed. Close the" >&2
            echo "       application, then run: bash tests/cli/seed/unload.sh" >&2
        else
            echo "       Point $variable at a real GGUF file." >&2
        fi
        return 1
    fi
}

# Echo the llama-server binary: LLAMA_BIN when set, else the engine staged in
# this checkout, else the one on PATH. The staged engine comes first because it
# is the pinned build the application ships: a model that needs it is refused
# by an older server found on PATH ("unknown model architecture").
# `command -v` answers non-zero when the binary is absent, which under
# `set -e` would end a recipe on a bare exit code, so it is resolved here and
# the absence is named.
llama_resolve_bin() {
    local bin="${LLAMA_BIN:-}" staged
    staged="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/target/debug/runners/llama-server"
    if [ -z "$bin" ] && [ -x "$staged" ]; then bin="$staged"; fi
    [ -n "$bin" ] || bin="$(command -v llama-server || true)"
    if [ -z "$bin" ] || [ ! -x "$bin" ]; then
        echo "error: llama-server not found." >&2
        echo "       Stage the pinned engine (packaging/fetch-llama-server.sh)," >&2
        echo "       install llama.cpp so that llama-server is on PATH, or set" >&2
        echo "       APOLLIA_LLAMA_BIN to the binary (see .env.example)." >&2
        return 1
    fi
    printf '%s\n' "$bin"
}

# Fill LLAMA_ARGV for a model and a profile, `dev` or `automation`.
#
# `--jinja` makes tool calling template-driven; a server started without it
# falls back on a tool grammar it may fail to parse. `-c` is the total context,
# split across the `-np` slots, so one conversation gets CTX/SLOTS tokens: one
# slot gives a single-user desktop the whole window at no extra memory.
#
# The automation profile holds what the model books assert on: a context large
# enough for one slot to carry a whole prompt (eight slots over 16384 tokens
# left 2048 per slot and the server answered 400), greedy sampling on a fixed
# seed so two runs give the same plan, tool call or question, and thinking
# turned off. Each default yields to its setting.
llama_argv() {
    local model="$1" profile="${2:-dev}"
    local ctx="${LLAMA_CTX:-}" temp="${LLAMA_TEMP:-}" seed="${LLAMA_SEED:-}"
    LLAMA_ARGV=(-m "$model" -ngl 999 -np "${LLAMA_SLOTS:-1}" -cb --flash-attn on
        --jinja --host 127.0.0.1 --port "${LLAMA_PORT:-8899}")
    if [ "$profile" = "automation" ]; then
        ctx="${ctx:-131072}"
        temp="${temp:-0}"
        seed="${seed:-42}"
        LLAMA_ARGV+=(--chat-template-kwargs '{"enable_thinking":false}')
    fi
    LLAMA_ARGV+=(-c "${ctx:-32768}")
    if [ -n "$temp" ]; then LLAMA_ARGV+=(--temp "$temp"); fi
    if [ -n "$seed" ]; then LLAMA_ARGV+=(--seed "$seed"); fi
    if [ -n "${LLAMA_EXTRA_ARGS:-}" ]; then
        local extra
        # Word splitting is the point: the setting carries several flags.
        # shellcheck disable=SC2206
        extra=(${LLAMA_EXTRA_ARGS})
        LLAMA_ARGV+=("${extra[@]}")
    fi
}

# Run llama-server in the foreground, replacing the shell.
llama_serve() {
    local model="${1:-}" bin
    llama_require_model "$model" || return 1
    bin="$(llama_resolve_bin)" || return 1
    llama_argv "$model" dev
    echo "-> llama-server on :${LLAMA_PORT:-8899}: $(basename "$model")"
    exec "$bin" "${LLAMA_ARGV[@]}"
}

# Start llama-server in the background, wait for it, and stop it when the
# calling shell exits. Sets LLAMA_PID.
llama_start() {
    local model="${1:-}" profile="${2:-dev}" bin
    local log="${LLAMA_LOG:-/tmp/apollia-dev-llama-server.log}"
    llama_require_model "$model" || return 1
    bin="$(llama_resolve_bin)" || return 1
    llama_argv "$model" "$profile"
    echo "-> starting llama-server ($profile) on :${LLAMA_PORT:-8899}: $(basename "$model")"
    "$bin" "${LLAMA_ARGV[@]}" > "$log" 2>&1 &
    LLAMA_PID=$!
    trap 'kill "$LLAMA_PID" 2>/dev/null || true' EXIT
    llama_wait_health "$LLAMA_PID" "${LLAMA_PORT:-8899}" "$log"
}

# Wait until /health answers. Returns 1 when the server dies while loading, or
# has not answered after five minutes.
llama_wait_health() {
    local pid="$1" port="$2" log="$3"
    for _ in $(seq 1 300); do
        if curl -sf "http://127.0.0.1:$port/health" >/dev/null 2>&1; then
            echo "-> llama-server ready on :$port (log: $log)"
            return 0
        fi
        if ! kill -0 "$pid" 2>/dev/null; then
            echo "error: llama-server died while loading:" >&2
            tail -15 "$log" >&2
            return 1
        fi
        sleep 1
    done
    echo "error: llama-server did not answer /health within 300 s (log: $log)" >&2
    return 1
}
