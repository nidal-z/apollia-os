#!/usr/bin/env bash
# Self-test of the recipe libraries, with every external command stubbed: no
# build, no model, no network. Launched by the `guards` recipe.
#
# Exit 0 when every case holds, 1 as soon as one does not.
set -uo pipefail

LIB="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
failed=0

check() {
    local label="$1" expected="$2" actual="$3"
    if [ "$expected" = "$actual" ]; then
        echo "  ok    $label"
    else
        echo "  FAIL  $label: expected '$expected', got '$actual'"
        failed=$((failed + 1))
    fi
}

# Run automation_run in a subshell where cargo, the seed builder, the bundle
# and llama-server are stubs, and print what the application would be started
# with.
drive() {
    (
        cd "$WORK" || exit 1
        # shellcheck source=automation.sh
        source "$LIB/automation.sh"
        python_bundle_activate() { :; }
        llama_start() { echo "llama=$2"; }
        bash() { :; }
        cargo() { echo "home=$HOME registry=${APOLLIA_MCP_REGISTRY_URL:-unset}"; }
        unset APOLLIA_MCP_REGISTRY_URL
        SEED_HOME="$WORK/seed" AUTOMATION_OUT="$WORK/out"
        automation_run "$@"
    ) 2>&1 | grep -E '^(home|llama)=' | tr '\n' ' '
}

echo "recipe libraries: the automaton meets the home and the registry it was asked for"
touch "$WORK/book.json"
head -c 2000000 /dev/zero > "$WORK/model.gguf"
printf 'stub' > "$WORK/placeholder.gguf"

# GIVEN a seeded run
# WHEN the application is started
# THEN it runs on the throwaway home and the registry is pinned to a closed port
check "a seeded run pins the MCP registry" \
    "home=$WORK/seed registry=http://127.0.0.1:1 " "$(drive seeded "$WORK/book.json")"

# GIVEN a screenshot run with its overlay
# WHEN the application is started
# THEN the registry is pinned as well: the overlay changes rows, not the network
check "a screenshot run pins the MCP registry too" \
    "home=$WORK/seed registry=http://127.0.0.1:1 " \
    "$(APOLLIA_SEED_OVERLAY="$WORK" drive shooting "$WORK/book.json")"

# GIVEN a first-launch run on a blank home
# WHEN the application is started
# THEN the public registry stays in reach, as a new installation has it
check "a blank home leaves the public registry in reach" \
    "home=$WORK/seed registry=unset " "$(drive blank "$WORK/book.json")"

# GIVEN a run on the real profile with a model
# WHEN the application is started
# THEN HOME is untouched and llama-server gets the automation profile
check "a real-profile run keeps HOME and starts the automation profile" \
    "llama=automation home=$HOME registry=unset " \
    "$(drive real "$WORK/book.json" "$WORK/model.gguf")"

echo "recipe libraries: a recipe that needs a model refuses anything else"
# shellcheck source=llama.sh
source "$LIB/llama.sh"

# GIVEN no model, then a placeholder of a few bytes, then a file of model size
# WHEN the requirement is checked
# THEN the first two are refused and the third passes
llama_require_model "" 2>/dev/null
check "no model is refused" "1" "$?"
llama_require_model "$WORK/placeholder.gguf" 2>/dev/null
check "a placeholder under a megabyte is refused" "1" "$?"
llama_require_model "$WORK/model.gguf" 2>/dev/null
check "a file of model size passes" "0" "$?"
check "the refusal names the variable the recipe reads" "1" \
    "$(llama_require_model "" APOLLIA_TEST_MODEL_GGUF 2>&1 | grep -c 'set APOLLIA_TEST_MODEL_GGUF')"

# GIVEN no setting, then a context set by the caller
# WHEN the arguments are built for each profile
# THEN each profile carries its defaults and the setting wins over them
unset LLAMA_CTX LLAMA_TEMP LLAMA_SEED LLAMA_EXTRA_ARGS LLAMA_PORT LLAMA_SLOTS
llama_argv model.gguf dev
check "the dev profile defaults to 32768 tokens and the server's sampling" \
    "-m model.gguf -ngl 999 -np 1 -cb --flash-attn on --jinja --host 127.0.0.1 --port 8899 -c 32768" \
    "${LLAMA_ARGV[*]}"
llama_argv model.gguf automation
check "the automation profile is greedy, seeded, and holds a whole prompt" \
    "-c 131072 --temp 0 --seed 42" "${LLAMA_ARGV[*]: -6}"
LLAMA_CTX=4096 llama_argv model.gguf automation
check "a context setting wins over the profile" "-c 4096 --temp 0 --seed 42" "${LLAMA_ARGV[*]: -6}"

echo
if [ "$failed" -ne 0 ]; then
    echo "$failed case(s) failed"
    exit 1
fi
echo "recipe libraries: every case holds"
