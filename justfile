# Apollia OS - Build & Dev tasks
# Usage: just <recipe>
# Install just: cargo install just

set shell := ["bash", "-euo", "pipefail", "-c"]

# Defaults (override per command: `just <recipe> var=value`)
desktop_runners := "cpu metal"
macos_target := "aarch64-apple-darwin"
# Local llama-server used as the external OpenAI-compat backend in dev.
# Override the model per command or export APOLLIA_LLAMA_MODEL.
llama_model := env_var_or_default("APOLLIA_LLAMA_MODEL", "")
llama_port := "8899"

# -----------------------------------------------------------------------------
# Documentation
# -----------------------------------------------------------------------------

# Generate Rust API docs (rustdoc)
rustdoc:
    @echo "→ Génération rustdoc..."
    cargo doc --no-deps --workspace --document-private-items
    @echo "✅ rustdoc → target/doc/"

# Regenerate the site's machine references (CLI / API / SDK) from the code
docs-regen:
    bash docs/site/regen.sh

# Build the public documentation site (Docusaurus, en + fr)
docs:
    cd docs/site && npm ci && npm run build
    @echo "✅ Site → docs/site/build/"

# -----------------------------------------------------------------------------
# Rust workspace
# -----------------------------------------------------------------------------

# Full workspace build
build:
    cargo build --workspace

# Release workspace build
build-release:
    cargo build --workspace --release

# Full tests
test:
    cargo test --workspace

# Tests with Python features
test-python:
    PYO3_PYTHON=/opt/homebrew/bin/python3.13 cargo test --workspace --features python-tests

# Full lint
lint:
    cargo fmt --check
    cargo clippy --workspace -- -D warnings

# Format code
fmt:
    cargo fmt --all

# -----------------------------------------------------------------------------
# Runner sidecar
# -----------------------------------------------------------------------------

# Build debug runner for one backend and keep unsuffixed binary
runner-debug backend:
    cargo build -p apollia-runner --features local-{{backend}}

# Build release runner for one backend and keep unsuffixed binary
runner-release backend target="":
    if [ -n "{{target}}" ]; then cargo build -p apollia-runner --release --target "{{target}}" --features local-{{backend}}; else cargo build -p apollia-runner --release --features local-{{backend}}; fi

# Build + suffix debug runner binary so daemon can auto-detect it
runner-debug-suffixed backend:
    cargo build -p apollia-runner --features local-{{backend}}
    cp "target/debug/apollia-runner" "target/debug/apollia-runner-{{backend}}"
    chmod +x "target/debug/apollia-runner-{{backend}}" || true
    # On macOS, `cp` invalidates the linker-signed adhoc signature, so the
    # kernel SIGKILLs the copy at launch ("Code Signature Invalid"). Re-sign
    # adhoc to restore execution.
    if [ "$(uname)" = "Darwin" ]; then codesign --force --sign - "target/debug/apollia-runner-{{backend}}"; fi

# macOS dev defaults: Metal + CPU fallback
runners-dev-macos:
    just runner-debug-suffixed metal
    just runner-debug-suffixed cpu

# -----------------------------------------------------------------------------
# Desktop (Tauri)
# -----------------------------------------------------------------------------

# Install desktop frontend dependencies
desktop-ui-install:
    cd crates/apollia-desktop/ui && npm ci

# Run desktop in dev mode (expects runners in target/debug/)
desktop-dev:
    cd crates/apollia-desktop && cargo tauri dev

# macOS dev shortcut: ensure metal+cpu runners then start desktop
desktop-dev-macos: runners-dev-macos
    cd crates/apollia-desktop && RUST_LOG=debug cargo tauri dev

# Start a local llama-server as the external OpenAI-compat backend (:8899).
# Launched WITH --jinja so tool-calling is template-driven (native); a server
# started without --jinja falls back to a tool grammar it may fail to parse.
# `-c` is the TOTAL context, split across the `-np` parallel slots, so the usable
# context PER conversation is CTX/NP. Desktop is single-user, so NP=1 gives one
# conversation the whole CTX at no extra KV-cache cost (memory tracks CTX, not NP).
# The model is a POSITIONAL argument (not `model=...`): just llama-server /path/to/model.gguf
# Or export APOLLIA_LLAMA_MODEL. Override context via env: CTX=65536 NP=1 just llama-server ...
llama-server model=llama_model port=llama_port:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "{{model}}" ]; then
      echo "set a model: just llama-server /path/to/model.gguf (or export APOLLIA_LLAMA_MODEL)" >&2
      exit 1
    fi
    LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server)}"
    echo "→ llama-server (--jinja) on :{{port}} : $(basename "{{model}}") [ctx=${CTX:-32768} np=${NP:-1}]"
    exec "$LLAMA_BIN" -m "{{model}}" -ngl 999 -c "${CTX:-32768}" -np "${NP:-1}" -cb \
      --flash-attn on --jinja --host 127.0.0.1 --port "{{port}}"

# macOS dev with the external llama-server (:8899) + desktop together.
# Starts llama-server (--jinja) in the background, waits for /health, then runs
# the desktop with RUST_LOG=debug. Kills the server on exit. The model is a
# POSITIONAL argument: just desktop-dev-llama /path/to/model.gguf (or export
# APOLLIA_LLAMA_MODEL). For the baked-in Qwen dev model, use `just desktop-dev-qwen`.
desktop-dev-llama model=llama_model: runners-dev-macos
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "{{model}}" ]; then
      echo "set a model: just desktop-dev-llama /path/to/model.gguf (or export APOLLIA_LLAMA_MODEL)" >&2
      exit 1
    fi
    LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server)}"
    SLOG="/tmp/apollia-dev-llama-server.log"
    echo "→ starting llama-server (--jinja) on :{{llama_port}} ..."
    "$LLAMA_BIN" -m "{{model}}" -ngl 999 -c "${CTX:-32768}" -np "${NP:-1}" -cb \
      --flash-attn on --jinja --host 127.0.0.1 --port {{llama_port}} > "$SLOG" 2>&1 &
    LPID=$!
    trap 'kill $LPID 2>/dev/null || true' EXIT
    for _ in $(seq 1 300); do
      curl -sf http://127.0.0.1:{{llama_port}}/health >/dev/null 2>&1 && break
      kill -0 $LPID 2>/dev/null || { echo "llama-server died at load:" >&2; tail -15 "$SLOG" >&2; exit 1; }
      sleep 1
    done
    echo "✅ llama-server ready on :{{llama_port}} (log: $SLOG)"
    cd crates/apollia-desktop && RUST_LOG=debug cargo tauri dev

# Dedicated dev backend: local Qwen3.6-35B-A3B MoE on :8899, tuned for a single
# desktop conversation (NP=1) with a large context and the exact tool-calling flags.
# Defaults to ~/.apollia/models/Qwen3.6-35B-A3B-MXFP4_MOE.gguf; override the model via
# APOLLIA_LLAMA_MODEL and context/slots/port via CTX / NP / PORT env. Server only:
# run the desktop separately (or `just desktop-dev-qwen` for both).
llama-qwen:
    #!/usr/bin/env bash
    set -euo pipefail
    MODEL="${APOLLIA_LLAMA_MODEL:-$HOME/.apollia/models/Qwen3.6-35B-A3B-MXFP4_MOE.gguf}"
    if [ ! -f "$MODEL" ]; then
      echo "model not found: $MODEL (set APOLLIA_LLAMA_MODEL to override)" >&2
      exit 1
    fi
    LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server)}"
    PORT="${PORT:-8899}"
    echo "→ llama-server (--jinja) on :$PORT : $(basename "$MODEL") [ctx=${CTX:-131072} np=${NP:-1}]"
    exec "$LLAMA_BIN" -m "$MODEL" -ngl 999 -c "${CTX:-131072}" -np "${NP:-1}" -cb \
      --flash-attn on --jinja --chat-template-kwargs '{"enable_thinking":false}' \
      --host 127.0.0.1 --port "$PORT"

# Dedicated: Qwen dev backend (llama-qwen, background) + desktop together on macOS.
# Waits for /health, runs the desktop with RUST_LOG=debug, kills the server on exit.
# Same env overrides as `llama-qwen`.
desktop-dev-qwen: runners-dev-macos
    #!/usr/bin/env bash
    set -euo pipefail
    MODEL="${APOLLIA_LLAMA_MODEL:-$HOME/.apollia/models/Qwen3.6-35B-A3B-MXFP4_MOE.gguf}"
    if [ ! -f "$MODEL" ]; then
      echo "model not found: $MODEL (set APOLLIA_LLAMA_MODEL to override)" >&2
      exit 1
    fi
    LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server)}"
    PORT="${PORT:-{{llama_port}}}"
    SLOG="/tmp/apollia-dev-llama-server.log"
    echo "→ starting llama-server (--jinja) on :$PORT : $(basename "$MODEL") [ctx=${CTX:-131072} np=${NP:-1}] ..."
    "$LLAMA_BIN" -m "$MODEL" -ngl 999 -c "${CTX:-131072}" -np "${NP:-1}" -cb \
      --flash-attn on --jinja --chat-template-kwargs '{"enable_thinking":false}' \
      --host 127.0.0.1 --port "$PORT" > "$SLOG" 2>&1 &
    LPID=$!
    trap 'kill $LPID 2>/dev/null || true' EXIT
    for _ in $(seq 1 300); do
      curl -sf "http://127.0.0.1:$PORT/health" >/dev/null 2>&1 && break
      kill -0 $LPID 2>/dev/null || { echo "llama-server died at load:" >&2; tail -15 "$SLOG" >&2; exit 1; }
      sleep 1
    done
    echo "✅ llama-server ready on :$PORT (log: $SLOG)"
    cd crates/apollia-desktop && RUST_LOG=debug cargo tauri dev

# Build desktop bundle (uses bundle-cli.sh + APOLLIA_DESKTOP_RUNNERS)
desktop-build target="{{macos_target}}" runners="{{desktop_runners}}":
    cd crates/apollia-desktop && APOLLIA_DESKTOP_RUNNERS="{{runners}}" cargo tauri build --target "{{target}}"

# Build desktop bundle for current host target
desktop-build-host runners="{{desktop_runners}}":
    cd crates/apollia-desktop && APOLLIA_DESKTOP_RUNNERS="{{runners}}" cargo tauri build

# -----------------------------------------------------------------------------
# CLI / release helpers
# -----------------------------------------------------------------------------

cli-build:
    cargo build -p apollia-cli

cli-release target="":
    if [ -n "{{target}}" ]; then cargo build -p apollia-cli --release --target "{{target}}"; else cargo build -p apollia-cli --release; fi

# Common release presets
release-macos:
    just cli-release target={{macos_target}}
    just desktop-build target={{macos_target}} runners="cpu metal"

release-linux:
    just cli-release target=x86_64-unknown-linux-gnu
    just desktop-build target=x86_64-unknown-linux-gnu runners="cpu"

release-windows:
    just cli-release target=x86_64-pc-windows-msvc
    just desktop-build target=x86_64-pc-windows-msvc runners="cpu"

# -----------------------------------------------------------------------------
# Combined tasks
# -----------------------------------------------------------------------------

# Local CI: lint + tests
ci: lint test
    @echo "✅ CI locale passée"

# Clean generated artifacts
clean:
    cargo clean
    @echo "✅ Artefacts nettoyés"

# -----------------------------------------------------------------------------
# Desktop test automaton (dev-only gestural runner)
# -----------------------------------------------------------------------------

# Run a gestural automation script against the real desktop app. Captures + a
# report.json land in .apollia-automation/ (gitignored). macOS prompts for
# Screen Recording once on the first capture.
# Usage: just desktop-dev-automation scripts/automation/smoke-nav.json
desktop-dev-automation script: runners-dev-macos
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f "{{script}}" ]; then
      echo "script not found: {{script}} (try scripts/automation/smoke-nav.json)" >&2
      exit 1
    fi
    SCRIPT_ABS="$(cd "$(dirname "{{script}}")" >/dev/null 2>&1 && pwd)/$(basename "{{script}}")"
    OUT="${APOLLIA_AUTOMATION_OUT:-$PWD/.apollia-automation}"
    mkdir -p "$OUT"
    OUT="$(cd "$OUT" >/dev/null 2>&1 && pwd)"
    echo "→ automation: {{script}}  (out: $OUT)"
    cd crates/apollia-desktop && \
      APOLLIA_AUTOMATION="$SCRIPT_ABS" APOLLIA_AUTOMATION_OUT="$OUT" \
      RUST_LOG=debug cargo tauri dev

# Same as desktop-dev-automation, plus a background llama-server (--jinja, :8899)
# for real inference (chat / HITL / A2A scripts need a live backend). The model
# is the 2nd positional arg (or export APOLLIA_LLAMA_MODEL). Usage:
# just desktop-dev-automation-llama scripts/automation/chat-libre.json /path/to/model.gguf
# Context/slots default to ctx=131072 np=1 (aligned with desktop-dev-qwen so a real
# chat prompt fits the slot; np=8/ctx=16384 gave 2048 tokens/slot and a 400 overflow).
# Override via CTX / NP env.
desktop-dev-automation-llama script model=llama_model: runners-dev-macos
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f "{{script}}" ]; then
      echo "script not found: {{script}} (try scripts/automation/chat-libre.json)" >&2
      exit 1
    fi
    # Model: the positional arg or $APOLLIA_LLAMA_MODEL; defaults to the baked
    # Qwen dev model (better tool selection than a small model for this suite).
    MODEL="{{model}}"
    if [ -z "$MODEL" ]; then
      MODEL="$HOME/.apollia/models/Qwen3.6-35B-A3B-MXFP4_MOE.gguf"
    fi
    if [ ! -f "$MODEL" ]; then
      echo "model not found: $MODEL (pass a path or export APOLLIA_LLAMA_MODEL)" >&2
      exit 1
    fi
    SCRIPT_ABS="$(cd "$(dirname "{{script}}")" >/dev/null 2>&1 && pwd)/$(basename "{{script}}")"
    OUT="${APOLLIA_AUTOMATION_OUT:-$PWD/.apollia-automation}"
    mkdir -p "$OUT"
    OUT="$(cd "$OUT" >/dev/null 2>&1 && pwd)"
    LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server)}"
    SLOG="/tmp/apollia-dev-llama-server.log"
    echo "→ starting llama-server (--jinja) on :{{llama_port}} : $(basename "$MODEL") [ctx=${CTX:-131072} np=${NP:-1}] ..."
    "$LLAMA_BIN" -m "$MODEL" -ngl 999 -c "${CTX:-131072}" -np "${NP:-1}" -cb \
      --flash-attn on --jinja --chat-template-kwargs '{"enable_thinking":false}' \
      --host 127.0.0.1 --port {{llama_port}} > "$SLOG" 2>&1 &
    LPID=$!
    trap 'kill $LPID 2>/dev/null || true' EXIT
    for _ in $(seq 1 300); do
      curl -sf http://127.0.0.1:{{llama_port}}/health >/dev/null 2>&1 && break
      kill -0 $LPID 2>/dev/null || { echo "llama-server died at load:" >&2; tail -15 "$SLOG" >&2; exit 1; }
      sleep 1
    done
    echo "✅ llama-server ready on :{{llama_port}} (log: $SLOG)"
    echo "→ automation: {{script}}  (out: $OUT)"
    cd crates/apollia-desktop && \
      APOLLIA_AUTOMATION="$SCRIPT_ABS" APOLLIA_AUTOMATION_OUT="$OUT" \
      RUST_LOG=debug cargo tauri dev

# Seeded variant: builds an isolated, fully-populated data ecosystem (SQLite DBs
# + agents + memory + models + config) under a throwaway HOME, then runs the app
# pointed at it so the det scripts find data (projects, triggers, permissions,
# tasks, backends, memory, installed models, mcp servers, transcriptions...).
# The real ~/.apollia profile is NOT touched. Only HOME is swapped; the build
# toolchain env (CARGO_HOME / RUSTUP_HOME) is preserved so cargo/rustc still work.
# Seed dir defaults to $PWD/.apollia-seed-home (override via APOLLIA_SEED_HOME).
# Usage: just desktop-dev-automation-seeded scripts/automation/master-det.json
desktop-dev-automation-seeded script: runners-dev-macos
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f "{{script}}" ]; then
      echo "script not found: {{script}} (try scripts/automation/master-det.json)" >&2
      exit 1
    fi
    SCRIPT_ABS="$(cd "$(dirname "{{script}}")" >/dev/null 2>&1 && pwd)/$(basename "{{script}}")"
    OUT="${APOLLIA_AUTOMATION_OUT:-$PWD/.apollia-automation}"
    mkdir -p "$OUT"
    OUT="$(cd "$OUT" >/dev/null 2>&1 && pwd)"
    SEED_HOME="${APOLLIA_SEED_HOME:-$PWD/.apollia-seed-home}"
    # The seed builder lives next to the script (scripts/automation/seed/), which
    # may be in a worktree while the app is run from main. Derive it from the script.
    bash "$(dirname "$SCRIPT_ABS")/seed/build-seed.sh" "$SEED_HOME"
    # Preserve the toolchain env (defaults derive from the REAL home) before the swap.
    export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
    export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
    echo "→ seeded automation: {{script}}  (HOME=$SEED_HOME, out: $OUT)"
    cd crates/apollia-desktop && \
      HOME="$SEED_HOME" APOLLIA_AUTOMATION="$SCRIPT_ABS" APOLLIA_AUTOMATION_OUT="$OUT" \
      RUST_LOG=info cargo tauri dev

# Seeded + llama-server (for the -llama scripts). The app runs under the seeded
# HOME, but llama-server loads the REAL model from the real home (the seed's
# models/ holds tiny placeholder GGUFs, not a runnable model). Model = 2nd arg
# or $APOLLIA_LLAMA_MODEL, default the real Qwen dev model.
# Usage: just desktop-dev-automation-seeded-llama scripts/automation/chat-llm.json
desktop-dev-automation-seeded-llama script model=llama_model: runners-dev-macos
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f "{{script}}" ]; then
      echo "script not found: {{script}}" >&2
      exit 1
    fi
    REAL_HOME="$HOME"
    MODEL="{{model}}"
    if [ -z "$MODEL" ]; then
      MODEL="$REAL_HOME/.apollia/models/Qwen3.6-35B-A3B-MXFP4_MOE.gguf"
    fi
    if [ ! -f "$MODEL" ]; then
      echo "model not found: $MODEL (pass a path or export APOLLIA_LLAMA_MODEL)" >&2
      exit 1
    fi
    SCRIPT_ABS="$(cd "$(dirname "{{script}}")" >/dev/null 2>&1 && pwd)/$(basename "{{script}}")"
    OUT="${APOLLIA_AUTOMATION_OUT:-$PWD/.apollia-automation}"
    mkdir -p "$OUT"
    OUT="$(cd "$OUT" >/dev/null 2>&1 && pwd)"
    SEED_HOME="${APOLLIA_SEED_HOME:-$PWD/.apollia-seed-home}"
    bash "$(dirname "$SCRIPT_ABS")/seed/build-seed.sh" "$SEED_HOME"
    export CARGO_HOME="${CARGO_HOME:-$REAL_HOME/.cargo}"
    export RUSTUP_HOME="${RUSTUP_HOME:-$REAL_HOME/.rustup}"
    LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server)}"
    SLOG="/tmp/apollia-dev-llama-server.log"
    echo "→ starting llama-server (--jinja) on :{{llama_port}} : $(basename "$MODEL") [ctx=${CTX:-131072} np=${NP:-1}] ..."
    "$LLAMA_BIN" -m "$MODEL" -ngl 999 -c "${CTX:-131072}" -np "${NP:-1}" -cb \
      --flash-attn on --jinja --chat-template-kwargs '{"enable_thinking":false}' \
      --host 127.0.0.1 --port {{llama_port}} > "$SLOG" 2>&1 &
    LPID=$!
    trap 'kill $LPID 2>/dev/null || true' EXIT
    for _ in $(seq 1 300); do
      curl -sf http://127.0.0.1:{{llama_port}}/health >/dev/null 2>&1 && break
      kill -0 $LPID 2>/dev/null || { echo "llama-server died at load:" >&2; tail -15 "$SLOG" >&2; exit 1; }
      sleep 1
    done
    echo "✅ llama-server ready on :{{llama_port}} (log: $SLOG)"
    echo "→ seeded automation: {{script}}  (HOME=$SEED_HOME, out: $OUT)"
    cd crates/apollia-desktop && \
      HOME="$SEED_HOME" APOLLIA_AUTOMATION="$SCRIPT_ABS" APOLLIA_AUTOMATION_OUT="$OUT" \
      RUST_LOG=info cargo tauri dev
