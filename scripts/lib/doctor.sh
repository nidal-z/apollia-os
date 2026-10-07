#!/usr/bin/env bash
# List the tools the recipes of the justfile need, present and absent.
#
# Exit 0 when every tool answers, 2 as soon as one is missing: nothing is
# broken in the tree, the machine is simply not ready to measure it. LLAMA_BIN,
# when set, stands in for a llama-server on PATH.
set -uo pipefail

missing=0

# report <name> <needed for> <install hint> <probe command...>
report() {
    local name="$1" purpose="$2" hint="$3"
    shift 3
    if "$@" >/dev/null 2>&1; then
        printf '  ok       %-14s %s\n' "$name" "$purpose"
    else
        printf '  MISSING  %-14s %s\n' "$name" "$purpose"
        printf '           %-14s install: %s\n' "" "$hint"
        missing=$((missing + 1))
    fi
}

have_llama_server() {
    if [ -n "${LLAMA_BIN:-}" ]; then [ -x "$LLAMA_BIN" ]; else command -v llama-server; fi
}

echo "Tools the recipes need:"
report just "every recipe" "cargo install just" command -v just
report cargo-tauri "desktop, automation and release recipes" "cargo install tauri-cli --version '^2'" cargo tauri --version
report node "the desktop UI and the documentation site" "https://nodejs.org" command -v node
report python "guards, tests and the seed builder" "Python 3.12 or newer, as python3" command -v python3
report pytest "test-hitl and the SDK tests" "python3 -m pip install pytest" python3 -m pytest --version
report ruff "guards" "python3 -m pip install ruff" command -v ruff
report mypy "guards" "python3 -m pip install mypy" command -v mypy
report pip-audit "audit-deps" "python3 -m pip install pip-audit" command -v pip-audit
report cargo-audit "guards and audit-deps" "cargo install cargo-audit" cargo audit --version
report cargo-machete "guards" "cargo install cargo-machete" cargo machete --version
report cargo-deny "guards" "cargo install cargo-deny" cargo deny --version
report docker "linux-check and linux-test" "https://docs.docker.com/get-docker/" command -v docker
report llama-server "the llm recipes and the model books of the automaton" "install llama.cpp, or set APOLLIA_LLAMA_BIN" have_llama_server

echo
if [ "$missing" -ne 0 ]; then
    echo "$missing tool(s) missing. A recipe that needs one stops and names it."
    exit 2
fi
echo "Every tool is present."
