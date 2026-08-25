# Apollia OS - Build & Dev tasks
# Usage: just <recipe>
# Install just: cargo install just

set shell := ["bash", "-euo", "pipefail", "-c"]
# Windows has no bash on PATH by default, so `just` fails before running any
# recipe. Git for Windows ships one; point at it explicitly rather than asking
# every contributor to alter their PATH. Override with
# `just --shell <path>` if bash lives elsewhere.
set windows-shell := ["C:/Program Files/Git/bin/bash.exe", "-euo", "pipefail", "-c"]

# Defaults (override per command: `just <recipe> var=value`)
desktop_runners := "cpu metal"
macos_target := "aarch64-apple-darwin"
linux_target := "x86_64-unknown-linux-gnu"
linux_runners := "cpu"
windows_target := "x86_64-pc-windows-msvc"
windows_runners := "cpu"
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

# The envelope proves the run leaves ~/.apollia untouched
# (scripts/check_test_home_isolation.py, exit 1 on a write, 2 when nothing
# was measured).

# Full tests under a sentinel HOME
test:
    python3 scripts/check_test_home_isolation.py --wrap cargo test --workspace --no-fail-fast

# Tests with Python features
test-python:
    PYO3_PYTHON="$(command -v python3)" cargo test --workspace --features python-tests

# Full lint
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings

# Format code
fmt:
    cargo fmt --all

# Runs the blocking Clippy gate of CI inside a container against the working
# tree, mounted read-only. Exits 2, not 0, when the Docker daemon is absent.
#
#   just linux-check              x86_64-unknown-linux-gnu, the release target
#   just linux-check arm          aarch64-unknown-linux-gnu, faster on Apple
#                                 Silicon, both its presets are allow_fail

# Check that this tree compiles on Linux, from a machine that is not Linux.
linux-check arch="x86":
    bash scripts/linux-check.sh {{arch}} compile

# Runs the first step of the Rust Tests job of CI, ci.yml:143-144, in the same
# container and on the same read-only mount. It derives cargo's parallelism from
# the container's cores and memory and sets it itself: left unbounded, linking
# the test binaries is killed for memory. Nothing is exported by the caller.
#
#   just linux-test               aarch64-unknown-linux-gnu, native, measured
#   just linux-test x86           x86_64-unknown-linux-gnu, emulated, cost
#                                 unmeasured. The default differs from
#                                 linux-check on purpose: this question is the
#                                 slow one, and only arm was measured
#   just linux-test arm apollia-tools python_executor
#                                 one crate, one test filter: a scoped Linux
#                                 question at one crate's build cost

# Run the workspace suites on Linux, from a machine that is not Linux.
linux-test arch="arm" *scope:
    bash scripts/linux-check.sh {{arch}} test {{scope}}

# Groups are cumulative and there is no default: `just worktree-prep` lists them
# and exits 1. See scripts/worktree-prep.sh for what each group lays down.
#
#   just worktree-prep rust        cargo and the CLI end-to-end suite
#   just worktree-prep ui docs     the frontend and documentation guards
#   just worktree-prep full        all three

# Make a linked worktree measure the same repository the main tree measures.
worktree-prep *GROUPS:
    bash scripts/worktree-prep.sh {{GROUPS}}

# `worktree-compare` reads two such records, and refuses them unless they were
# made on the same commit.

# The containerised Linux test suite is among the guards (recorded, exempt
# from the comparison).

# Record the verdict of the expensive guards in this tree
worktree-verdicts OUT:
    python3 scripts/worktree_verdicts.py --record {{OUT}}

# The report (.apollia-automation/report.json) only exists after:
#   just desktop-dev-automation-seeded scripts/automation/master-det.json
# Exit 0 fresh and green, 1 fresh and red (red sections listed), 2 when the
# report is absent or predates HEAD (nothing measured, which is not a pass).

# Read the machine verdict of the last seeded desktop automation run
desktop-automation-verdict:
    python3 scripts/check_automation_report.py

worktree-compare MAIN WORKTREE:
    python3 scripts/worktree_verdicts.py --compare {{MAIN}} {{WORKTREE}}

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

# The suite needs a Chromium that Playwright can drive. Without one every spec
# died red in 0 ms, which read as a product regression while nothing had been
# measured. The install is the declared precondition; when it cannot provide a
# browser the recipe answers 2, nothing measured, distinct from a red run.

# Run the Playwright perf suite of the desktop UI against the built bundle
desktop-ui-perf:
    #!/usr/bin/env bash
    set -uo pipefail
    cd crates/apollia-desktop/ui
    if ! npx playwright install chromium; then
      echo "NOTHING MEASURED: 'npx playwright install chromium' provided no browser" >&2
      exit 2
    fi
    npx playwright test tests/perf

# Without this a dev build links whatever libpython the host resolves, pyenv
# here, while main.rs points PYTHONHOME at the bundle in target/python-bundle.
# Two different CPython builds, so the interpreter cannot find its own standard
# library and every agent dies at boot with `ModuleNotFoundError: No module
# named '_opcode'` or `'math'`. The application starts, the failure is a warning
# nobody reads, and the onboarding agent is simply absent from the registry.
#
# The automation recipes already did this, which is why they loaded four agents
# while `desktop-dev-qwen` loaded none. Same block, hoisted so every dev entry
# point gets it.
#
# It exits 1 when it has laid down nothing, the way scripts/worktree-prep.sh
# does for the same resolution. Its four callers read its standard output, so
# an exit 0 let them carry on with no interpreter and no link.

# Link PyO3 against the interpreter setup_bundled_python resolves at run time.
_bundle-python:
    #!/usr/bin/env bash
    set -euo pipefail
    BUNDLE_ROOT=""
    for c in "$PWD/target/python-bundle/aarch64-apple-darwin/python" "$PWD/target/debug/python"; do
      if [ -x "$c/bin/python3.13" ]; then BUNDLE_ROOT="$c"; break; fi
    done
    if [ -z "$BUNDLE_ROOT" ]; then
      echo "error: no Python bundle in target/python-bundle or target/debug." >&2
      echo "       Agents will fail to load at boot. Build one with:" >&2
      echo "       bash packaging/build-python-bundle.sh" >&2
      exit 1
    fi
    # The bundle is relocatable but its sysconfig still names its build path, so
    # PyO3 emits `-L /install/lib` and the link fails on "library 'python3.13'
    # not found". release.yml compensates with the same RUSTFLAGS.
    #
    # And the linked install_name is @executable_path/../Resources/python/lib,
    # the packaged layout. In a dev run @executable_path is target/debug, so
    # dyld looks in target/Resources and finds nothing, and the app dies before
    # main(). DYLD_FALLBACK_LIBRARY_PATH is too late: setup_bundled_python runs
    # inside the process, after dyld has resolved. A symlink makes the dev tree
    # answer the same path the bundle does.
    mkdir -p target/Resources
    ln -sfn "$BUNDLE_ROOT" target/Resources/python
    echo "$BUNDLE_ROOT"

# Run desktop in dev mode (expects runners in target/debug/)
desktop-dev:
    #!/usr/bin/env bash
    set -euo pipefail
    BUNDLE_ROOT="$(just _bundle-python | tail -1)"
    export PYO3_PYTHON="$BUNDLE_ROOT/bin/python3.13"
    export RUSTFLAGS="${RUSTFLAGS:-} -L $BUNDLE_ROOT/lib"
    cd crates/apollia-desktop && cargo tauri dev

# macOS dev shortcut: ensure metal+cpu runners then start desktop
desktop-dev-macos: runners-dev-macos
    #!/usr/bin/env bash
    set -euo pipefail
    BUNDLE_ROOT="$(just _bundle-python | tail -1)"
    export PYO3_PYTHON="$BUNDLE_ROOT/bin/python3.13"
    export RUSTFLAGS="${RUSTFLAGS:-} -L $BUNDLE_ROOT/lib"
    cd crates/apollia-desktop && RUST_LOG=debug cargo tauri dev

# Launched WITH --jinja so tool-calling is template-driven (native); a server
# started without --jinja falls back to a tool grammar it may fail to parse.
# `-c` is the TOTAL context, split across the `-np` parallel slots, so the usable
# context PER conversation is CTX/NP. Desktop is single-user, so NP=1 gives one
# conversation the whole CTX at no extra KV-cache cost (memory tracks CTX, not NP).
# The model is a POSITIONAL argument (not `model=...`): just llama-server /path/to/model.gguf
# Or export APOLLIA_LLAMA_MODEL. Override context via env: CTX=65536 NP=1 just llama-server ...

# Start a local llama-server as the external OpenAI-compat backend (:8899).
llama-server model=llama_model port=llama_port:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "{{model}}" ]; then
      echo "set a model: just llama-server /path/to/model.gguf (or export APOLLIA_LLAMA_MODEL)" >&2
      exit 1
    fi
    # `command -v` returns non-zero when the binary is absent, and under
    # `set -e` that kills the recipe before any echo runs: the operator sees a
    # bare "exit code 1" and has to read the justfile to find out why. Resolve
    # it explicitly and say what is missing.
    LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server || true)}"
    if [ -z "$LLAMA_BIN" ] || [ ! -x "$LLAMA_BIN" ]; then
      echo "llama-server not found on PATH." >&2
      echo "  Install it, or set LLAMA_BIN=/path/to/llama-server." >&2
      echo "  If you masked it for a packaging test, restore it:" >&2
      echo "    mv /opt/homebrew/bin/llama-server.masked-for-dmg-test \\" >&2
      echo "       /opt/homebrew/bin/llama-server" >&2
      exit 1
    fi
    echo "→ llama-server (--jinja) on :{{port}} : $(basename "{{model}}") [ctx=${CTX:-32768} np=${NP:-1}]"
    exec "$LLAMA_BIN" -m "{{model}}" -ngl 999 -c "${CTX:-32768}" -np "${NP:-1}" -cb \
      --flash-attn on --jinja --host 127.0.0.1 --port "{{port}}"

# Starts llama-server (--jinja) in the background, waits for /health, then runs
# the desktop with RUST_LOG=debug. Kills the server on exit. The model is a
# POSITIONAL argument: just desktop-dev-llama /path/to/model.gguf (or export
# APOLLIA_LLAMA_MODEL). For the baked-in Qwen dev model, use `just desktop-dev-qwen`.

# macOS dev with the external llama-server (:8899) + desktop together.
desktop-dev-llama model=llama_model: runners-dev-macos
    #!/usr/bin/env bash
    set -euo pipefail
    BUNDLE_ROOT="$(just _bundle-python | tail -1)"
    export PYO3_PYTHON="$BUNDLE_ROOT/bin/python3.13"
    export RUSTFLAGS="${RUSTFLAGS:-} -L $BUNDLE_ROOT/lib"
    if [ -z "{{model}}" ]; then
      echo "set a model: just desktop-dev-llama /path/to/model.gguf (or export APOLLIA_LLAMA_MODEL)" >&2
      exit 1
    fi
    # `command -v` returns non-zero when the binary is absent, and under
    # `set -e` that kills the recipe before any echo runs: the operator sees a
    # bare "exit code 1" and has to read the justfile to find out why. Resolve
    # it explicitly and say what is missing.
    LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server || true)}"
    if [ -z "$LLAMA_BIN" ] || [ ! -x "$LLAMA_BIN" ]; then
      echo "llama-server not found on PATH." >&2
      echo "  Install it, or set LLAMA_BIN=/path/to/llama-server." >&2
      echo "  If you masked it for a packaging test, restore it:" >&2
      echo "    mv /opt/homebrew/bin/llama-server.masked-for-dmg-test \\" >&2
      echo "       /opt/homebrew/bin/llama-server" >&2
      exit 1
    fi
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

# Tuned for a single desktop conversation (NP=1) with a large context and the
# exact tool-calling flags.
# Defaults to ~/.apollia/models/Qwen3.6-35B-A3B-MXFP4_MOE.gguf; override the model via
# APOLLIA_LLAMA_MODEL and context/slots/port via CTX / NP / PORT env. Server only:
# run the desktop separately (or `just desktop-dev-qwen` for both).

# Dedicated dev backend: local Qwen3.6-35B-A3B MoE on :8899.
llama-qwen:
    #!/usr/bin/env bash
    set -euo pipefail
    MODEL="${APOLLIA_LLAMA_MODEL:-$HOME/.apollia/models/Qwen3.6-35B-A3B-MXFP4_MOE.gguf}"
    if [ ! -f "$MODEL" ]; then
      echo "model not found: $MODEL (set APOLLIA_LLAMA_MODEL to override)" >&2
      exit 1
    fi
    # A GGUF under a megabyte is not a model, it is the automation seed's
    # placeholder. load.sh moves the real ~/.apollia aside and installs a profile
    # whose models/ holds 8 KB stubs, so this path keeps resolving and
    # llama-server dies on `unknown model architecture: ''`, which describes a
    # corrupt model rather than a swapped profile. Say which it is.
    if [ "$(wc -c < "$MODEL")" -lt 1000000 ]; then
      echo "model file is only $(wc -c < "$MODEL") bytes: $MODEL" >&2
      if [ -d "$HOME/.apollia.before-seed" ]; then
        echo "  The automation seed is loaded. Your real profile is at" >&2
        echo "  ~/.apollia.before-seed, and the real model with it." >&2
        echo "  Close the application, then: bash tests/cli/seed/unload.sh" >&2
      else
        echo "  That is a placeholder, not a model. Point APOLLIA_LLAMA_MODEL at a real one." >&2
      fi
      exit 1
    fi
    # `command -v` returns non-zero when the binary is absent, and under
    # `set -e` that kills the recipe before any echo runs: the operator sees a
    # bare "exit code 1" and has to read the justfile to find out why. Resolve
    # it explicitly and say what is missing.
    LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server || true)}"
    if [ -z "$LLAMA_BIN" ] || [ ! -x "$LLAMA_BIN" ]; then
      echo "llama-server not found on PATH." >&2
      echo "  Install it, or set LLAMA_BIN=/path/to/llama-server." >&2
      echo "  If you masked it for a packaging test, restore it:" >&2
      echo "    mv /opt/homebrew/bin/llama-server.masked-for-dmg-test \\" >&2
      echo "       /opt/homebrew/bin/llama-server" >&2
      exit 1
    fi
    PORT="${PORT:-8899}"
    echo "→ llama-server (--jinja) on :$PORT : $(basename "$MODEL") [ctx=${CTX:-131072} np=${NP:-1}]"
    exec "$LLAMA_BIN" -m "$MODEL" -ngl 999 -c "${CTX:-131072}" -np "${NP:-1}" -cb \
      --flash-attn on --jinja --chat-template-kwargs '{"enable_thinking":false}' \
      --host 127.0.0.1 --port "$PORT"

# Waits for /health, runs the desktop with RUST_LOG=debug, kills the server on exit.
# Same env overrides as `llama-qwen`.

# Dedicated: Qwen dev backend (llama-qwen, background) + desktop together on macOS.
desktop-dev-qwen: runners-dev-macos
    #!/usr/bin/env bash
    set -euo pipefail
    BUNDLE_ROOT="$(just _bundle-python | tail -1)"
    export PYO3_PYTHON="$BUNDLE_ROOT/bin/python3.13"
    export RUSTFLAGS="${RUSTFLAGS:-} -L $BUNDLE_ROOT/lib"
    MODEL="${APOLLIA_LLAMA_MODEL:-$HOME/.apollia/models/Qwen3.6-35B-A3B-MXFP4_MOE.gguf}"
    if [ ! -f "$MODEL" ]; then
      echo "model not found: $MODEL (set APOLLIA_LLAMA_MODEL to override)" >&2
      exit 1
    fi
    # A GGUF under a megabyte is not a model, it is the automation seed's
    # placeholder. load.sh moves the real ~/.apollia aside and installs a profile
    # whose models/ holds 8 KB stubs, so this path keeps resolving and
    # llama-server dies on `unknown model architecture: ''`, which describes a
    # corrupt model rather than a swapped profile. Say which it is.
    if [ "$(wc -c < "$MODEL")" -lt 1000000 ]; then
      echo "model file is only $(wc -c < "$MODEL") bytes: $MODEL" >&2
      if [ -d "$HOME/.apollia.before-seed" ]; then
        echo "  The automation seed is loaded. Your real profile is at" >&2
        echo "  ~/.apollia.before-seed, and the real model with it." >&2
        echo "  Close the application, then: bash tests/cli/seed/unload.sh" >&2
      else
        echo "  That is a placeholder, not a model. Point APOLLIA_LLAMA_MODEL at a real one." >&2
      fi
      exit 1
    fi
    # `command -v` returns non-zero when the binary is absent, and under
    # `set -e` that kills the recipe before any echo runs: the operator sees a
    # bare "exit code 1" and has to read the justfile to find out why. Resolve
    # it explicitly and say what is missing.
    LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server || true)}"
    if [ -z "$LLAMA_BIN" ] || [ ! -x "$LLAMA_BIN" ]; then
      echo "llama-server not found on PATH." >&2
      echo "  Install it, or set LLAMA_BIN=/path/to/llama-server." >&2
      echo "  If you masked it for a packaging test, restore it:" >&2
      echo "    mv /opt/homebrew/bin/llama-server.masked-for-dmg-test \\" >&2
      echo "       /opt/homebrew/bin/llama-server" >&2
      exit 1
    fi
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

# `runners` selects which apollia-runner-{backend} sidecars are staged and
# which llama-server GPU build is bundled (first gpu backend in the list wins).
# Examples:
#   just desktop-build x86_64-pc-windows-msvc "cpu vulkan"
#   just desktop-build aarch64-apple-darwin "cpu metal"

# Build desktop bundle (uses bundle-cli.sh + APOLLIA_DESKTOP_RUNNERS).
desktop-build target=macos_target runners=desktop_runners:
    cd crates/apollia-desktop && APOLLIA_DESKTOP_RUNNERS="{{runners}}" CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded cargo tauri build --target "{{target}}"

# Build desktop bundle for current host target
desktop-build-host runners=desktop_runners:
    cd crates/apollia-desktop && APOLLIA_DESKTOP_RUNNERS="{{runners}}" CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded cargo tauri build

# -----------------------------------------------------------------------------
# CLI / release helpers
# -----------------------------------------------------------------------------

cli-build:
    cargo build -p apollia-cli

# The GGUF path is pointed at a file that does not exist so Track 3 records a
# justified skip instead of silently loading the model of the real HOME.

# CLI E2E suite, Tracks 1 + 2: boots the daemon on a throwaway seeded HOME
cli-e2e-runtime:
    APOLLIA_REQUIRE_RUNTIME=1 APOLLIA_TEST_MODEL_GGUF=/nonexistent-apollia-e2e.gguf bash tests/cli/cli-e2e.sh

# CLI E2E suite, all three tracks: Track 3 loads the given GGUF read-only.
cli-e2e-model gguf:
    APOLLIA_REQUIRE_RUNTIME=1 APOLLIA_TEST_MODEL_GGUF="{{gguf}}" bash tests/cli/cli-e2e.sh

cli-release target="":
    if [ -n "{{target}}" ]; then cargo build -p apollia-cli --release --target "{{target}}"; else cargo build -p apollia-cli --release; fi

# Example: just release-desktop x86_64-pc-windows-msvc "cpu cuda"

# Build CLI + desktop bundle for any rust triple and runner set.
release-desktop target runners=desktop_runners:
    just cli-release {{target}}
    just desktop-build {{target}} "{{runners}}"

# Common release presets (override target and/or runners per command)
release-macos target=macos_target runners="cpu metal":
    just release-desktop {{target}} "{{runners}}"

release-linux target=linux_target runners=linux_runners:
    just release-desktop {{target}} "{{runners}}"

release-windows target=windows_target runners=windows_runners:
    just release-desktop {{target}} "{{runners}}"

# -----------------------------------------------------------------------------
# Combined tasks
# -----------------------------------------------------------------------------

# The surfaces are the bundled Python site-packages, the desktop UI runtime
# deps, and the Rust crates. The three advisory databases live upstream, so
# the network is required: offline, the recipe answers 2 (nothing measured),
# never a green.

# Audit the dependency surfaces the release ships
audit-deps:
    #!/usr/bin/env bash
    set -uo pipefail
    if ! curl -fsI --max-time 10 https://pypi.org >/dev/null 2>&1; then
      echo "audit-deps: network unreachable, nothing was audited" >&2
      exit 2
    fi
    reds=0
    pip-audit -r packaging/requirements-bundled.txt --no-deps --progress-spinner off || reds=1
    ( cd crates/apollia-desktop/ui && npm audit --omit=dev ) || reds=1
    cargo audit || reds=1
    if [ "$reds" -ne 0 ]; then
      echo "audit-deps: at least one surface carries an open advisory" >&2
      exit 1
    fi
    echo "audit-deps: three surfaces audited, zero advisory"

# Run every tracked guard script and the external gates, and report every red one.
guards:
    #!/usr/bin/env bash
    set -uo pipefail
    # Named one by one rather than globbed: the crossing carried by
    # scripts/check_selftest.py looks for each guard's basename inside the
    # files that declare a boundary, and a glob would name none of them.
    guards=(
      "scripts/automation/tools/validate.py"
      "scripts/check_automation_derived.py"
      "scripts/check_ci_workflows.py"
      "scripts/check_claim_anchors.py"
      "scripts/check_claims.py"
      "scripts/check_cli_e2e_coverage.py"
      "scripts/check_crate_lints.py"
      "scripts/check_ctx_contract.py"
      "scripts/check_docs_anchors.py"
      "scripts/check_docs_frontmatter.py"
      "scripts/check_docs_lang.py"
      "scripts/check_docs_mirror.py"
      "scripts/check_docs_routes.py"
      "scripts/check_download_sums.py"
      "scripts/check_guard_verdicts.py"
      "scripts/check_i18n_catalogue.py"
      "scripts/check_instrument_verdicts.py"
      "scripts/check_no_font_cdn.py"
      "scripts/check_optional_builders.py --strict"
      "scripts/check_panic_free.py"
      "scripts/check_prose.py"
      "scripts/check_python_rules.py"
      "scripts/check_rust_rules.py"
      "scripts/check_selftest.py"
      "scripts/check_subprocess_window.py"
      "scripts/check_tauri_ipc_args.py"
      "scripts/check_tauri_ipc_callers.py"
      "scripts/check_testid_anchors.py"
      "scripts/check_custom_event_listeners.py"
      "scripts/check_entry_doc_commands.py"
      "scripts/check_playwright_specs.py"
    )
    # External gates: not check_*.py scripts, but rules of the same corpus.
    # The crossing in scripts/check_selftest.py requires each to be launched
    # by at least one boundary; this recipe is the local boundary for the
    # five below, which previously ran only as CI jobs while the CI was not
    # running. cargo audit needs the advisory database (network on first
    # run); mypy comes from the SDK toolchain.
    externals=(
      "cargo machete"
      "cargo audit"
      "cargo deny check advisories"
      "cd sdk && mypy apollia"
      "cd crates/apollia-desktop/ui && npm run audit:i18n"
    )
    # Reds accumulate instead of stopping the run: an operator wants the whole
    # list, and stopping on the first one hides the twelve behind it.
    reds=()
    for guard in "${guards[@]}"; do
      # Word splitting is wanted here, one entry carries an argument. This body
      # runs under the bash of the shebang above, whose splitting is reliable.
      # shellcheck disable=SC2086
      if python3 $guard; then
        echo "== ok   $guard"
      else
        echo "== RED  $guard"
        reds+=("$guard")
      fi
    done
    for gate in "${externals[@]}"; do
      if bash -c "$gate"; then
        echo "== ok   $gate"
      else
        echo "== RED  $gate"
        reds+=("$gate")
      fi
    done
    echo
    if [ "${#reds[@]}" -ne 0 ]; then
      echo "${#reds[@]} guard(s) red:" >&2
      for guard in "${reds[@]}"; do echo "  $guard" >&2; done
      exit 1
    fi
    echo "$(( ${#guards[@]} + ${#externals[@]} )) guards green"

# Local CI: guards + lint + tests
ci: guards lint test
    @echo "✅ CI locale passée"

# Clean generated artifacts
clean:
    cargo clean
    @echo "✅ Artefacts nettoyés"

# -----------------------------------------------------------------------------
# Desktop test automaton (dev-only gestural runner)
# -----------------------------------------------------------------------------

# Captures + a report.json land in .apollia-automation/ (gitignored). macOS
# prompts for Screen Recording once on the first capture.
# Usage: just desktop-dev-automation scripts/automation/master-det.json

# Run a gestural automation script against the real desktop app.
desktop-dev-automation script: runners-dev-macos
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
    echo "→ automation: {{script}}  (out: $OUT)"
    cd crates/apollia-desktop && \
      APOLLIA_AUTOMATION="$SCRIPT_ABS" APOLLIA_AUTOMATION_OUT="$OUT" \
      RUST_LOG=debug cargo tauri dev

# The background server is there for real inference (chat / HITL / A2A scripts
# need a live backend). The model is the 2nd positional arg (or export
# APOLLIA_LLAMA_MODEL). Usage:
# just desktop-dev-automation-llama scripts/automation/chat-llm.json /path/to/model.gguf
# Context/slots default to ctx=131072 np=1 (aligned with desktop-dev-qwen so a real
# chat prompt fits the slot; np=8/ctx=16384 gave 2048 tokens/slot and a 400 overflow).
# Override via CTX / NP env.

# Same as desktop-dev-automation, plus a background llama-server (--jinja, :8899).
desktop-dev-automation-llama script model=llama_model: runners-dev-macos
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f "{{script}}" ]; then
      echo "script not found: {{script}} (try scripts/automation/chat-llm.json)" >&2
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
    # `command -v` returns non-zero when the binary is absent, and under
    # `set -e` that kills the recipe before any echo runs: the operator sees a
    # bare "exit code 1" and has to read the justfile to find out why. Resolve
    # it explicitly and say what is missing.
    LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server || true)}"
    if [ -z "$LLAMA_BIN" ] || [ ! -x "$LLAMA_BIN" ]; then
      echo "llama-server not found on PATH." >&2
      echo "  Install it, or set LLAMA_BIN=/path/to/llama-server." >&2
      echo "  If you masked it for a packaging test, restore it:" >&2
      echo "    mv /opt/homebrew/bin/llama-server.masked-for-dmg-test \\" >&2
      echo "       /opt/homebrew/bin/llama-server" >&2
      exit 1
    fi
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

# The ecosystem is SQLite DBs + agents + memory + models + config, so the det
# scripts find data (projects, triggers, permissions, tasks, backends, memory,
# installed models, mcp servers, transcriptions...).
# The real ~/.apollia profile is NOT touched. Only HOME is swapped; the build
# toolchain env (CARGO_HOME / RUSTUP_HOME) is preserved so cargo/rustc still work.
# Seed dir defaults to $PWD/.apollia-seed-home (override via APOLLIA_SEED_HOME).
# Usage: just desktop-dev-automation-seeded scripts/automation/master-det.json

# Seeded variant: the app runs against a throwaway, fully-populated HOME.
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
    # The seed builder lives next to the script (tests/cli/seed/), which
    # may be in a worktree while the app is run from main. Derive it from the script.
    #
    # The seed builder lives under `tests/cli/seed/`, not next to the driving
    # scripts: the automaton was moved out of the public tree and its fixture
    # stayed behind with the suite that asserts its row counts. Deriving the path
    # from the driving script's own directory outlived that move and pointed at
    # an emptied directory, so this recipe failed at 127 before booting anything.
    # `env -u APOLLIA_SEED_OVERLAY`: the narrative overlay is for the screenshot
    # session, not for assertions. A maintainer who exports it in their shell
    # would otherwise change the row counts this suite asserts on, and the
    # failures would read as product regressions.
    # The overlay is stripped by default: it changes row counts, and an
    # assertion suite run with it reads its own failures as product
    # regressions. `desktop-screenshots` sets APOLLIA_SEED_SHOOTING=1 to opt
    # back in, because for a screenshot an empty timeline is the defect.
    if [ "${APOLLIA_SEED_SHOOTING:-0}" = "1" ]; then
      bash "{{justfile_directory()}}/tests/cli/seed/build-seed.sh" "$SEED_HOME"
    else
      env -u APOLLIA_SEED_OVERLAY bash "{{justfile_directory()}}/tests/cli/seed/build-seed.sh" "$SEED_HOME"
    fi
    # Preserve the toolchain env (defaults derive from the REAL home) before the swap.
    export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
    export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
    # Build PyO3 against the SAME interpreter the app will run with, which is
    # what release.yml does and what the dev recipes did not. Without it, PyO3
    # links whatever python3 the developer happens to have (pyenv, homebrew)
    # while PYTHONHOME points at the staged bundle: two different 3.13 builds,
    # so the embedded interpreter fails on its own builtins and EVERY agent
    # fails to load at boot with "ModuleNotFoundError: No module named
    # '_opcode'". The suite then runs against zero agents and quietly loses the
    # chat, the agent logs and everything downstream of them.
    # Two dev layouts exist: the one release.yml stages per triple, and the one
    # a plain build leaves in target/debug. Take whichever is present, because
    # what matters is linking against the SAME bundle setup_bundled_python will
    # resolve at run time, not against a particular path.
    BUNDLE_ROOT=""
    for c in "$PWD/target/python-bundle/aarch64-apple-darwin/python" "$PWD/target/debug/python"; do
      if [ -x "$c/bin/python3.13" ]; then BUNDLE_ROOT="$c"; break; fi
    done
    BUNDLE_PY="$BUNDLE_ROOT/bin/python3.13"
    if [ -n "$BUNDLE_ROOT" ]; then
      export PYO3_PYTHON="$BUNDLE_PY"
      # The bundle is relocatable but its sysconfig still names the path it was
      # built at, so PyO3 emits `-L /install/lib` and the link fails on
      # "library 'python3.13' not found". release.yml compensates with the same
      # RUSTFLAGS; the dev recipes did not, which is the other half of why a
      # local build never used the bundled interpreter.
      export RUSTFLAGS="${RUSTFLAGS:-} -L $BUNDLE_ROOT/lib"
      # The linked install_name is @executable_path/../Resources/python/lib,
      # which is the packaged layout. In a dev run @executable_path is
      # target/debug, so dyld looks in target/Resources and finds nothing, and
      # the app dies before main(). Setting DYLD_FALLBACK_LIBRARY_PATH would be
      # too late: setup_bundled_python runs inside the process, after dyld has
      # already resolved. A symlink makes the dev tree answer the same path the
      # bundle does, and as a bonus setup_bundled_python then matches on its
      # macOS candidate instead of falling through to the Windows one.
      mkdir -p target/Resources
      ln -sfn "$BUNDLE_ROOT" target/Resources/python
    else
      echo "warning: no Python bundle found in target/python-bundle or target/debug" >&2
      echo "         agents will fail to load; run packaging/build-python-bundle.sh" >&2
    fi
    echo "→ seeded automation: {{script}}  (HOME=$SEED_HOME, out: $OUT)"
    cd crates/apollia-desktop && \
      HOME="$SEED_HOME" APOLLIA_AUTOMATION="$SCRIPT_ABS" APOLLIA_AUTOMATION_OUT="$OUT" \
      RUST_LOG=info cargo tauri dev

# `desktop-dev-automation-seeded` deliberately strips APOLLIA_SEED_OVERLAY,
# because the overlay changes row counts and would make the assertion suites
# read as product regressions. Screenshots want the opposite: the narrative is
# the whole point, an empty timeline photographs as a broken product. So the
# shooting runs get their own recipe rather than an env var a maintainer has to
# remember, which is how the two ended up conflated in the first place.
#
# Usage: just desktop-screenshots scripts/automation/screenshots-en.json

# Screenshot runs: seeded WITH the narrative overlay.
desktop-screenshots script:
    #!/usr/bin/env bash
    set -euo pipefail
    OVERLAY="${APOLLIA_SEED_OVERLAY:-$HOME/.apollia-seed-overlay}"
    if [ ! -d "$OVERLAY" ]; then
      echo "narrative overlay not found at $OVERLAY." >&2
      echo "  It lives outside the repository on purpose. Set APOLLIA_SEED_OVERLAY" >&2
      echo "  to its directory, or shoot without it via desktop-dev-automation-seeded" >&2
      echo "  and accept empty timelines in the images." >&2
      exit 1
    fi
    echo "→ screenshots with narrative overlay: $OVERLAY"
    APOLLIA_SEED_SHOOTING=1 APOLLIA_SEED_OVERLAY="$OVERLAY" \
      just desktop-dev-automation-seeded "{{script}}"

# The app runs under the seeded HOME, but llama-server loads the REAL model from
# the real home (the seed's models/ holds tiny placeholder GGUFs, not a runnable
# model). Model = 2nd arg or $APOLLIA_LLAMA_MODEL, default the real Qwen dev
# model.
# Usage: just desktop-dev-automation-seeded-llama scripts/automation/chat-llm.json

# Seeded + llama-server, for the -llama scripts.
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
    # See the deterministic recipe above for why the overlay is unset here.
    env -u APOLLIA_SEED_OVERLAY bash "{{justfile_directory()}}/tests/cli/seed/build-seed.sh" "$SEED_HOME"
    export CARGO_HOME="${CARGO_HOME:-$REAL_HOME/.cargo}"
    export RUSTUP_HOME="${RUSTUP_HOME:-$REAL_HOME/.rustup}"
    # Build PyO3 against the SAME interpreter the app will run with, which is
    # what release.yml does and what the dev recipes did not. Without it, PyO3
    # links whatever python3 the developer happens to have (pyenv, homebrew)
    # while PYTHONHOME points at the staged bundle: two different 3.13 builds,
    # so the embedded interpreter fails on its own builtins and EVERY agent
    # fails to load at boot with "ModuleNotFoundError: No module named
    # '_opcode'". The suite then runs against zero agents and quietly loses the
    # chat, the agent logs and everything downstream of them.
    # Two dev layouts exist: the one release.yml stages per triple, and the one
    # a plain build leaves in target/debug. Take whichever is present, because
    # what matters is linking against the SAME bundle setup_bundled_python will
    # resolve at run time, not against a particular path.
    BUNDLE_ROOT=""
    for c in "$PWD/target/python-bundle/aarch64-apple-darwin/python" "$PWD/target/debug/python"; do
      if [ -x "$c/bin/python3.13" ]; then BUNDLE_ROOT="$c"; break; fi
    done
    BUNDLE_PY="$BUNDLE_ROOT/bin/python3.13"
    if [ -n "$BUNDLE_ROOT" ]; then
      export PYO3_PYTHON="$BUNDLE_PY"
      # The bundle is relocatable but its sysconfig still names the path it was
      # built at, so PyO3 emits `-L /install/lib` and the link fails on
      # "library 'python3.13' not found". release.yml compensates with the same
      # RUSTFLAGS; the dev recipes did not, which is the other half of why a
      # local build never used the bundled interpreter.
      export RUSTFLAGS="${RUSTFLAGS:-} -L $BUNDLE_ROOT/lib"
      # The linked install_name is @executable_path/../Resources/python/lib,
      # which is the packaged layout. In a dev run @executable_path is
      # target/debug, so dyld looks in target/Resources and finds nothing, and
      # the app dies before main(). Setting DYLD_FALLBACK_LIBRARY_PATH would be
      # too late: setup_bundled_python runs inside the process, after dyld has
      # already resolved. A symlink makes the dev tree answer the same path the
      # bundle does, and as a bonus setup_bundled_python then matches on its
      # macOS candidate instead of falling through to the Windows one.
      mkdir -p target/Resources
      ln -sfn "$BUNDLE_ROOT" target/Resources/python
    else
      echo "warning: no Python bundle found in target/python-bundle or target/debug" >&2
      echo "         agents will fail to load; run packaging/build-python-bundle.sh" >&2
    fi
    # `command -v` returns non-zero when the binary is absent, and under
    # `set -e` that kills the recipe before any echo runs: the operator sees a
    # bare "exit code 1" and has to read the justfile to find out why. Resolve
    # it explicitly and say what is missing.
    LLAMA_BIN="${LLAMA_BIN:-$(command -v llama-server || true)}"
    if [ -z "$LLAMA_BIN" ] || [ ! -x "$LLAMA_BIN" ]; then
      echo "llama-server not found on PATH." >&2
      echo "  Install it, or set LLAMA_BIN=/path/to/llama-server." >&2
      echo "  If you masked it for a packaging test, restore it:" >&2
      echo "    mv /opt/homebrew/bin/llama-server.masked-for-dmg-test \\" >&2
      echo "       /opt/homebrew/bin/llama-server" >&2
      exit 1
    fi
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
